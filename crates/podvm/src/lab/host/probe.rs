//! The probe of `podvm lab check`: podvm runs itself in the lab with
//! [`PROBE_VERB`] and prints one line for each part of each profile. A part
//! holds where the call that the profile refuses fails with its errno, and
//! the call that it keeps still works.

use std::io::ErrorKind;
use std::os::unix::net::UnixListener;

use podvm_image::error::{EXIT_FLAG_ERROR, EXIT_RUNTIME_ERROR};
use podvm_probe::sys as k;

use super::{parse, LAST_FD, TMP_BYTES};

/// The internal verb: `podvm lab --podvm-lab-probe PROFILE,...`.
pub const PROBE_VERB: &str = "--podvm-lab-probe";

/// One part: its name, and None where it holds, else why not.
type Part = (&'static str, Option<String>);

/// `podvm lab check PROFILE,...`: run the probe in the lab, print its
/// lines, and give 0 where each part holds.
pub fn check(args: &[String]) -> i32 {
    let [list] = args else {
        eprintln!("{}", super::CHECK_USAGE);
        return EXIT_FLAG_ERROR;
    };
    if let Err(e) = parse(list) {
        eprintln!("podvm lab check: {e}");
        return EXIT_FLAG_ERROR;
    }
    let me = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("podvm lab check: the path of podvm: {e}");
            return EXIT_RUNTIME_ERROR;
        }
    };
    // ⛔ `/proc/self/exe` in the lab, not the path: the tmpfs of
    // `small-tmp` hides a podvm that lives under /tmp.
    let status = std::process::Command::new(&me)
        .args([
            "lab",
            "run",
            list,
            "--",
            "/proc/self/exe",
            "lab",
            PROBE_VERB,
            list,
        ])
        .status();
    match status.map(|s| s.code()) {
        Ok(Some(c)) => c,
        Ok(None) => {
            eprintln!("podvm lab check: the probe ended on a signal");
            EXIT_RUNTIME_ERROR
        }
        Err(e) => {
            eprintln!("podvm lab check: {e}");
            EXIT_RUNTIME_ERROR
        }
    }
}

/// The probe, inside the lab.
pub fn main(args: &[String]) -> i32 {
    let Some(Ok(profiles)) = args.first().map(|l| parse(l)) else {
        return EXIT_FLAG_ERROR;
    };
    let mut held = true;
    for p in profiles {
        for (part, why) in parts(p) {
            match why {
                None => println!("{p}: {part}: holds"),
                Some(w) => {
                    held = false;
                    println!("{p}: {part}: DOES NOT HOLD: {w}");
                }
            }
        }
    }
    i32::from(!held)
}

fn parts(profile: &str) -> Vec<Part> {
    match profile {
        "bind-denied" => vec![
            (
                "bind 127.0.0.1",
                refused(std::net::TcpListener::bind("127.0.0.1:0").map(drop), 13),
            ),
            (
                "bind ::1",
                refused_or_absent(std::net::TcpListener::bind("[::1]:0").map(drop), 13),
            ),
            (
                "connect 127.0.0.1",
                refused(std::net::TcpStream::connect("127.0.0.1:9").map(drop), 13),
            ),
            ("bind AF_UNIX", unix_bind().err()),
        ],
        "getsockname-denied" => vec![("getsockname", getsockname())],
        "userns-off" => vec![(
            "unshare(CLONE_NEWUSER)",
            match k::unshare(k::CLONE_NEWUSER) {
                Err(e) if e.0 == 1 => None,
                Err(e) => Some(format!("errno {}, not EPERM", e.0)),
                Ok(_) => Some("it passed".to_string()),
            },
        )],
        "small-tmp" => vec![("/tmp", tmp_mount(false)), ("TMPDIR", tmpdir())],
        "noexec-tmp" => vec![("/tmp", tmp_mount(true)), ("TMPDIR", tmpdir())],
        "inherited-fds" => vec![("descriptors 3 to 20", open_fds())],
        _ => vec![("the profile", Some("unknown".to_string()))],
    }
}

fn refused(r: std::io::Result<()>, errno: i32) -> Option<String> {
    match r {
        Err(e) if e.raw_os_error() == Some(errno) => None,
        Err(e) => Some(format!("{e}, not errno {errno}")),
        Ok(()) => Some("it passed".to_string()),
    }
}

/// As [`refused`]; a host with no IPv6 has nothing to refuse.
fn refused_or_absent(r: std::io::Result<()>, errno: i32) -> Option<String> {
    match &r {
        Err(e) if e.kind() == ErrorKind::AddrNotAvailable || e.raw_os_error() == Some(97) => None,
        _ => refused(r, errno),
    }
}

fn unix_bind() -> Result<UnixListener, String> {
    // A directory with a random part, made with create_dir: another user
    // cannot plant it (docs/conventions/forbidden-patterns.md).
    use std::io::Read;
    let mut r = [0u8; 8];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut r))
        .map_err(|e| format!("/dev/urandom: {e}"))?;
    let tag: String = r.iter().map(|b| format!("{b:02x}")).collect();
    let dir = std::env::temp_dir().join(format!("podvm-lab-{tag}"));
    std::fs::create_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join("probe.sock");
    let r = UnixListener::bind(&path).map_err(|e| format!("AF_UNIX bind: {e}"));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
    r
}

fn getsockname() -> Option<String> {
    let l = match std::os::unix::net::UnixDatagram::unbound() {
        Ok(l) => l,
        Err(e) => return Some(format!("a socket: {e}")),
    };
    refused(l.local_addr().map(drop), 95)
}

/// The line of `/tmp` in mountinfo: a tmpfs, of [`TMP_BYTES`] at most, and
/// noexec where asked.
fn tmp_mount(noexec: bool) -> Option<String> {
    let info = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    let Some(line) = info
        .lines()
        .rev()
        .find(|l| l.split(' ').nth(4) == Some("/tmp"))
    else {
        return Some("/tmp is not a mount".to_string());
    };
    let (left, right) = line.split_once(" - ").unwrap_or((line, ""));
    let mut r = right.split(' ');
    if r.next() != Some("tmpfs") {
        return Some(format!("/tmp is not a tmpfs: {right}"));
    }
    let opts = left.split(' ').nth(5).unwrap_or("");
    if noexec && !opts.split(',').any(|o| o == "noexec") {
        return Some(format!("/tmp is not noexec: {opts}"));
    }
    let sup = r.nth(1).unwrap_or("");
    let size = sup
        .split(',')
        .find_map(|o| o.strip_prefix("size="))
        .and_then(kib);
    match size {
        Some(b) if b <= TMP_BYTES => None,
        Some(b) => Some(format!("/tmp holds {b} bytes, over {TMP_BYTES}")),
        None => Some(format!("/tmp has no size: {sup}")),
    }
}

/// A size of mountinfo (`1048576k`) in bytes.
pub fn kib(s: &str) -> Option<u64> {
    let (n, unit) = s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()));
    let n: u64 = n.parse().ok()?;
    Some(match unit {
        "k" => n << 10,
        "m" => n << 20,
        "g" => n << 30,
        "" => n,
        _ => return None,
    })
}

fn tmpdir() -> Option<String> {
    match std::env::var("TMPDIR").as_deref() {
        Ok("/tmp") => None,
        other => Some(format!("TMPDIR is {other:?}")),
    }
}

fn open_fds() -> Option<String> {
    const F_GETFD: u64 = 1;
    let shut: Vec<i32> = (3..=LAST_FD)
        .filter(|&n| k::fcntl(i64::from(n), F_GETFD, 0).is_err())
        .collect();
    if shut.is_empty() {
        None
    } else {
        Some(format!("closed: {shut:?}"))
    }
}
