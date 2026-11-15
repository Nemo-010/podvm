//! `podvm lab run PROFILE -- CMD` and `podvm lab check PROFILE`: a hostile
//! host class as a lab on this host, with no root and no guest
//! (TODO/net.md T-1916, PodUtils/podrunner#182).
//!
//! Each profile is a part that a process with no privilege sets on itself
//! before the exec of CMD: a seccomp filter (`bind-denied`,
//! `getsockname-denied`, `userns-off`), a tmpfs on `/tmp` in a user and a
//! mount namespace (`small-tmp`, `noexec-tmp`), or open descriptors
//! (`inherited-fds`). `check` runs podvm's own probe inside the same lab,
//! so a lab that allows what it names as refused fails its own check.
//!
//! ⛔ A part that the host cannot give refuses by name (exit 125). The lab
//! never runs CMD with a part missing.
//!
//! The names follow the table of PodUtils/podrunner#180; the profiles of
//! `pod-lab` (podcrates@T-692) replace this table when a tag carries them.

mod probe;

use std::os::unix::process::CommandExt;

use pod_confine::Deny;
use podvm_image::error::{EXIT_FLAG_ERROR, EXIT_RUNTIME_ERROR};
use podvm_probe::sys as k;

pub use probe::{check, main as probe_main, PROBE_VERB};

/// One profile: its name, what it gives, and the source of the class.
pub struct Profile {
    pub name: &'static str,
    pub gives: &'static str,
    pub source: &'static str,
}

pub const PROFILES: &[Profile] = &[
    Profile {
        name: "bind-denied",
        gives: "bind(2) and connect(2) of an AF_INET or AF_INET6 address give EACCES, the loopback too; AF_UNIX holds",
        source: "PodUtils/podrunner#162, #167",
    },
    Profile {
        name: "getsockname-denied",
        gives: "getsockname(2) and getpeername(2) give EOPNOTSUPP, as under `podnet pod`",
        source: "PodUtils/podrunner#167",
    },
    Profile {
        name: "userns-off",
        gives: "unshare(2) and clone(2) with CLONE_NEWUSER give EPERM; clone3(2) gives ENOSYS, so the C library uses clone(2)",
        source: "PodUtils/podrunner#142",
    },
    Profile {
        name: "small-tmp",
        gives: "/tmp is a tmpfs of 1 GiB and TMPDIR=/tmp (a user and a mount namespace; the ids stay)",
        source: "PodUtils/podrunner#174",
    },
    Profile {
        name: "noexec-tmp",
        gives: "/tmp is a tmpfs mounted noexec and TMPDIR=/tmp (a user and a mount namespace; the ids stay)",
        source: "PodUtils/podrunner#182",
    },
    Profile {
        name: "inherited-fds",
        gives: "descriptors 3 to 20 are open with no FD_CLOEXEC: /dev/null and the two ends of a pipe",
        source: "PodUtils/podrunner#176",
    },
];

/// The size of the tmpfs of `small-tmp`, in bytes.
pub const TMP_BYTES: u64 = 1 << 30;
/// The descriptors of `inherited-fds`: 3 to this number.
pub const LAST_FD: i32 = 20;

/// Is NAME a profile of the host form?
pub fn is_profile(name: &str) -> bool {
    name.split(',')
        .all(|n| PROFILES.iter().any(|p| p.name == n))
}

/// The profiles of a list `A,B`, in the order of [`PROFILES`]. A name that
/// is not a profile refuses by name.
pub fn parse(list: &str) -> Result<Vec<&'static str>, String> {
    let mut out = Vec::new();
    for n in list.split(',') {
        if !PROFILES.iter().any(|p| p.name == n) {
            return Err(format!(
                "{n} is not a profile; `podvm lab profiles` lists them"
            ));
        }
    }
    for p in PROFILES {
        if list.split(',').any(|n| n == p.name) {
            out.push(p.name);
        }
    }
    Ok(out)
}

pub const RUN_USAGE: &str = "usage: podvm lab run PROFILE[,PROFILE...] -- CMD [ARG...]\n\n\
Run CMD in a hostile host class on this host, with no root and no guest.\n\
`podvm lab profiles` lists the profiles. A part that this host cannot give\n\
refuses with exit 125; CMD never runs with a part missing. The exit code\n\
is the code of CMD (127: CMD was not found, 126: it did not run).\n\
small-tmp and noexec-tmp mount a new /tmp: a CMD under /tmp is hidden.\n";

pub const CHECK_USAGE: &str = "usage: podvm lab check PROFILE[,PROFILE...]\n\n\
Prove that each part of the profiles holds on this host: podvm runs its own\n\
probe in the lab and prints one line for each part. Exit 0 when each part\n\
holds, 1 when one does not, 125 when the lab cannot be made.\n";

/// The list of `podvm lab profiles`.
pub fn profiles_text() -> String {
    let mut s = String::new();
    for p in PROFILES {
        s.push_str(&format!(
            "{:<20} {}\n{:<20} ({})\n",
            p.name, p.gives, "", p.source
        ));
    }
    s
}

/// `podvm lab run`.
pub fn run(args: &[String]) -> i32 {
    let (list, cmd) = match args {
        [list, dash, cmd @ ..] if dash == "--" && !cmd.is_empty() => (list, cmd),
        _ => {
            eprintln!("{RUN_USAGE}");
            return EXIT_FLAG_ERROR;
        }
    };
    let profiles = match parse(list) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("podvm lab run: {e}");
            return EXIT_FLAG_ERROR;
        }
    };
    if let Err(e) = enter(&profiles) {
        eprintln!("podvm lab run: {e}");
        return EXIT_RUNTIME_ERROR;
    }
    let mut c = std::process::Command::new(&cmd[0]);
    c.args(&cmd[1..]);
    if profiles.iter().any(|p| p.ends_with("-tmp")) {
        c.env("TMPDIR", "/tmp");
    }
    let err = c.exec();
    eprintln!("podvm lab run: {}: {err}", cmd[0]);
    if err.kind() == std::io::ErrorKind::NotFound {
        127
    } else {
        126
    }
}

/// Set each part of the profiles on this process, in order: the
/// namespaces and the tmpfs, the descriptors, then the seccomp filter (the
/// filter last, so that it does not refuse a step of the lab itself).
pub fn enter(profiles: &[&str]) -> Result<(), String> {
    let has = |n: &str| profiles.contains(&n);
    if has("small-tmp") || has("noexec-tmp") {
        tmp(has("noexec-tmp"))?;
    }
    if has("inherited-fds") {
        fds()?;
    }
    let deny = denials(profiles);
    if !deny.is_empty() {
        let prog =
            pod_confine::deny::program(&deny).map_err(|f| format!("the seccomp program: {f}"))?;
        k::set_no_new_privs().map_err(|e| format!("PR_SET_NO_NEW_PRIVS: errno {}", e.0))?;
        k::seccomp_set_filter(&prog, 0)
            .map_err(|e| format!("the seccomp filter: errno {}", e.0))?;
    }
    Ok(())
}

#[cfg(target_arch = "x86_64")]
const SYS_GETSOCKNAME: i64 = 51;
#[cfg(target_arch = "x86_64")]
const SYS_GETPEERNAME: i64 = 52;
#[cfg(target_arch = "aarch64")]
const SYS_GETSOCKNAME: i64 = 204;
#[cfg(target_arch = "aarch64")]
const SYS_GETPEERNAME: i64 = 205;

const EPERM: u16 = 1;
const EACCES: u16 = 13;
const ENOSYS: u16 = 38;
const EOPNOTSUPP: u16 = 95;
/// The length of a `sockaddr_in` and of a `sockaddr_in6`. A filter cannot
/// read the family behind the pointer; the length tells them from an
/// `AF_UNIX` address, but for a path of exactly 13 or 25 bytes.
const INET_LENGTHS: [u32; 2] = [16, 28];

/// The seccomp rules of the profiles.
pub fn denials(profiles: &[&str]) -> Vec<Deny> {
    let mut d = Vec::new();
    if profiles.contains(&"bind-denied") {
        for (name, nr) in [("bind", k::SYS_BIND), ("connect", k::SYS_CONNECT)] {
            for len in INET_LENGTHS {
                d.push(Deny::call(name, nr, EACCES).when_equal(2, u32::MAX, len));
            }
        }
    }
    if profiles.contains(&"getsockname-denied") {
        d.push(Deny::call("getsockname", SYS_GETSOCKNAME, EOPNOTSUPP));
        d.push(Deny::call("getpeername", SYS_GETPEERNAME, EOPNOTSUPP));
    }
    if profiles.contains(&"userns-off") {
        let newuser = k::CLONE_NEWUSER as u32;
        d.push(Deny::call("unshare", k::SYS_UNSHARE, EPERM).when_flags(0, newuser));
        d.push(Deny::call("clone", k::SYS_CLONE, EPERM).when_flags(0, newuser));
        d.push(Deny::call("clone3", k::SYS_CLONE3, ENOSYS));
    }
    d
}

/// A user and a mount namespace that keep the ids of the caller, and a
/// tmpfs on `/tmp`.
fn tmp(noexec: bool) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let me = std::fs::metadata("/proc/self").map_err(|e| format!("/proc/self: {e}"))?;
    let (uid, gid) = (me.uid(), me.gid());
    k::unshare(k::CLONE_NEWUSER | k::CLONE_NEWNS).map_err(|e| {
        format!("small-tmp and noexec-tmp need a user namespace, and unshare(CLONE_NEWUSER|CLONE_NEWNS) gave errno {}: this host cannot give them with no root", e.0)
    })?;
    for (path, line) in [
        ("/proc/self/setgroups", "deny".to_string()),
        ("/proc/self/uid_map", format!("{uid} {uid} 1")),
        ("/proc/self/gid_map", format!("{gid} {gid} 1")),
    ] {
        std::fs::write(path, line).map_err(|e| format!("{path}: {e}"))?;
    }
    let c = |s: &str| k::CBuf::new(s).ok_or_else(|| format!("{s}: a NUL byte"));
    let none = c("none")?;
    k::mount(&none, &c("/")?, &none, k::MS_REC | k::MS_PRIVATE)
        .map_err(|e| format!("make / private: errno {}", e.0))?;
    let mut flags = k::MS_NOSUID | k::MS_NODEV;
    if noexec {
        flags |= k::MS_NOEXEC;
    }
    let data = c(&format!("size={TMP_BYTES},mode=1777"))?;
    k::mount_data(&c("tmpfs")?, &c("/tmp")?, &c("tmpfs")?, flags, &data)
        .map_err(|e| format!("mount a tmpfs on /tmp: errno {}", e.0))?;
    Ok(())
}

/// Descriptors 3 to [`LAST_FD`], open with no `FD_CLOEXEC`.
///
/// The three sources move above the range first (`F_DUPFD_CLOEXEC` from
/// 64), so that a copy into the range never closes a source that it needs.
fn fds() -> Result<(), String> {
    use std::os::fd::IntoRawFd;
    const F_DUPFD_CLOEXEC: u64 = 1030;
    let null = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/null");
    let null = null.map_err(|e| format!("/dev/null: {e}"))?.into_raw_fd();
    let mut ends = [0i32; 2];
    k::pipe2(&mut ends, 0).map_err(|e| format!("pipe: errno {}", e.0))?;
    let mut high = [0i64; 3];
    for (h, fd) in high.iter_mut().zip([ends[0], ends[1], null]) {
        *h = k::fcntl(i64::from(fd), F_DUPFD_CLOEXEC, 64)
            .map_err(|e| format!("F_DUPFD_CLOEXEC: errno {}", e.0))?;
        let _ = k::close(i64::from(fd));
    }
    for n in 3..=LAST_FD {
        let from = high[(n as usize - 3).min(2)];
        k::dup2(from, i64::from(n)).map_err(|e| format!("dup2 to {n}: errno {}", e.0))?;
    }
    for h in high {
        let _ = k::close(h);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
