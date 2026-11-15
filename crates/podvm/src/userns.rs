//! PodUtils/podvm#397 (TODO/probe.md T-1909): the namespace rung for an
//! unprivileged caller. pod-enter calls `unshare(CLONE_NEWNS)` in the user
//! namespace of the caller, which needs `CAP_SYS_ADMIN` there, so a caller
//! with no capability entered the userland rung also where the host opens
//! user namespaces. Here podvm opens a user, a PID and a UTS namespace at the
//! start of a foreground `run`, while it has one thread, and maps the caller
//! to 0 in it (as `unshare -Ur`). The probe then sees the rights of the user
//! namespace, the ladder selects the namespace rung, `/proc` mounts in the PID
//! namespace, and `--hostname` names the UTS namespace.
//!
//! Three processes: this one waits outside and forwards the shutdown
//! signals; pid 1 of the PID namespace reaps and forwards; pid 2 runs the
//! rest of podvm. When pid 2 ends, pid 1 exits with its code, and the kernel
//! kills each process that stayed in the namespace.
//!
//! ⛔ Only a foreground `run`: `run -d`, `create` and `exec` keep pids in the
//! record, and a pid of the namespace would name another process outside.
//!
//! It is the default where the trial of the whole entry works (call C21 of
//! `docs/decisions.md`); `PODVM_USERNS=0` leaves the entry as before.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// The variable that turns the entry off (`0`).
pub const VAR: &str = "PODVM_USERNS";

const CLONE_NEWNS: i32 = 0x0002_0000;
const CLONE_NEWUTS: i32 = 0x0400_0000;
const MS_REC: u64 = 0x4000;
const MS_PRIVATE: u64 = 0x4_0000;
const CLONE_NEWUSER: i32 = 0x1000_0000;
const CLONE_NEWPID: i32 = 0x2000_0000;
const SIGHUP: i32 = 1;
const SIGINT: i32 = 2;
const SIGQUIT: i32 = 3;
const SIGUSR1: i32 = 10;
const SIGUSR2: i32 = 12;
const SIGTERM: i32 = 15;
const SIG_IGN: usize = 1;
const EINTR: i32 = 4;

unsafe extern "C" {
    fn unshare(flags: i32) -> i32;
    fn fork() -> i32;
    fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
    fn kill(pid: i32, sig: i32) -> i32;
    fn signal(sig: i32, handler: usize) -> usize;
    fn getuid() -> u32;
    fn getgid() -> u32;
    fn sethostname(name: *const u8, len: usize) -> i32;
    fn mount(
        src: *const std::ffi::c_char,
        target: *const std::ffi::c_char,
        fstype: *const std::ffi::c_char,
        flags: u64,
        data: *const u8,
    ) -> i32;
    fn __errno_location() -> *mut i32;
    fn _exit(code: i32) -> !;
    fn syscall(n: i64, ...) -> i64;
    fn close(fd: i32) -> i32;
    fn open(path: *const std::ffi::c_char, flags: i32, ...) -> i32;
}

static CHILD: AtomicI32 = AtomicI32::new(0);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static UTS: AtomicBool = AtomicBool::new(false);
/// A descriptor of the host's `/proc`, opened in pid 2 before its own
/// `/proc` covers it.
static HOST_PROC: AtomicI32 = AtomicI32::new(-1);

/// The pid that the host gives the process `pid` of this namespace, or
/// `pid` where podvm entered no namespace. A pidfd of the process, read
/// through the `fdinfo` of the host's `/proc`, gives its `Pid:` in the
/// namespace of that `/proc`.
pub fn host_pid(pid: i64) -> Option<i64> {
    let host = HOST_PROC.load(Ordering::Relaxed);
    if !active() || host < 0 {
        return Some(pid);
    }
    const PIDFD_OPEN: i64 = 434;
    // SAFETY: pidfd_open takes two integers.
    let fd = unsafe { syscall(PIDFD_OPEN, pid, 0) };
    if fd < 0 {
        return None;
    }
    let text = std::fs::read_to_string(format!("/proc/self/fd/{host}/self/fdinfo/{fd}"));
    // SAFETY: fd is the pidfd of this call.
    unsafe { close(fd as i32) };
    text.ok()?
        .lines()
        .find_map(|l| l.strip_prefix("Pid:"))
        .and_then(|v| v.trim().parse().ok())
        .filter(|p: &i64| *p > 0)
}

/// True where this process owns a UTS namespace of its own: the entry of
/// [`enter_if`], or a `run --hostname` of uid 0.
pub fn owns_uts() -> bool {
    UTS.load(Ordering::Relaxed)
}

/// True in pid 2, where this process owns the UTS namespace.
pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Name the UTS namespace of the run (`--hostname`, T-1908).
pub fn set_hostname(name: &str) -> std::io::Result<()> {
    // SAFETY: the pointer and the length name the bytes of `name`.
    if unsafe { sethostname(name.as_ptr(), name.len()) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

extern "C" fn forward(sig: i32) {
    let c = CHILD.load(Ordering::Relaxed);
    if c > 0 {
        // SAFETY: kill is async-signal-safe.
        unsafe { kill(c, sig) };
    }
}

/// True for an argument vector of a foreground `run` of a caller that is
/// not uid 0, where `PODVM_USERNS` is not `0`.
pub fn wanted(argv: &[String], euid: u32, var: Option<&str>) -> bool {
    if var == Some("0") || euid == 0 || argv.get(1).map(String::as_str) != Some("run") {
        return false;
    }
    // ⚠ A word of the payload that looks like `-d` also keeps the entry as
    // before: the cost is a weaker rung, never a wrong pid.
    !argv[2..].iter().any(|a| {
        a == "--detach"
            || a.starts_with("--detach=")
            || a.starts_with("--podvm-tier")
            || (a.len() > 1
                && a.starts_with('-')
                && !a.starts_with("--")
                && a[1..].bytes().all(|c| c.is_ascii_alphabetic())
                && a.contains('d'))
    })
}

/// The lines of the three map files for `uid` and `gid`.
pub fn maps(uid: u32, gid: u32) -> [(&'static str, String); 3] {
    [
        ("/proc/self/setgroups", "deny".to_string()),
        ("/proc/self/uid_map", format!("0 {uid} 1")),
        ("/proc/self/gid_map", format!("0 {gid} 1")),
    ]
}

/// Wait for `pid`; its exit code, or 128 and the signal.
fn wait_for(pid: i32) -> i32 {
    let mut st = 0;
    loop {
        // SAFETY: st lives for the call.
        let r = unsafe { waitpid(pid, &mut st, 0) };
        // SAFETY: errno of this thread.
        if r < 0 && unsafe { *__errno_location() } == EINTR {
            continue;
        }
        if r < 0 {
            return 125;
        }
        return if st & 0x7f == 0 {
            (st >> 8) & 0xff
        } else {
            128 + (st & 0x7f)
        };
    }
}

/// Install `forward` for the shutdown signals, and ignore the two that the
/// terminal sends to the whole process group (pid 2 and the payload get
/// them from the terminal).
fn relay_signals() {
    // SAFETY: forward is async-signal-safe; the handlers stay for the life
    // of the process.
    unsafe {
        for s in [SIGHUP, SIGTERM, SIGUSR1, SIGUSR2] {
            signal(s, forward as *const () as usize);
        }
        for s in [SIGINT, SIGQUIT] {
            signal(s, SIG_IGN);
        }
    }
}

/// In pid 2: a mount namespace with a `/proc` of the PID namespace. The
/// host's `/proc` names host pids, and `kill` takes pids of the namespace:
/// the sweep of the processes that stayed (T-1708) killed pid 2 itself.
fn own_proc() -> bool {
    let none = c"none".as_ptr();
    let slash = c"/".as_ptr();
    let proc_ = c"proc".as_ptr();
    let target = c"/proc".as_ptr();
    // SAFETY: each pointer is a NUL-ended literal.
    unsafe {
        unshare(CLONE_NEWNS) == 0
            && mount(
                none,
                slash,
                core::ptr::null(),
                MS_REC | MS_PRIVATE,
                core::ptr::null(),
            ) == 0
            && mount(proc_, target, proc_, 0, core::ptr::null()) == 0
    }
}

/// The user, PID and UTS namespaces, and the maps of `uid` and `gid`.
fn open_namespaces(uid: u32, gid: u32) -> bool {
    // SAFETY: one thread (the contract of the callers).
    let opened = unsafe { unshare(CLONE_NEWUSER | CLONE_NEWPID | CLONE_NEWUTS) } == 0;
    opened
        && maps(uid, gid)
            .iter()
            .all(|(file, line)| std::fs::write(file, line).is_ok())
}

/// The whole entry, tried in a child that is thrown away: true where each
/// step works. A host can open a user namespace and still refuse the
/// mounts in it (AppArmor's `apparmor_restrict_unprivileged_userns` of
/// Ubuntu 24.04), and a process cannot leave a user namespace again.
fn trial(uid: u32, gid: u32) -> bool {
    // SAFETY: one thread; the child only makes system calls and `_exit`s.
    let pid = unsafe { fork() };
    if pid < 0 {
        return false;
    }
    if pid == 0 {
        let code = if open_namespaces(uid, gid) {
            // SAFETY: as above.
            let inner = unsafe { fork() };
            if inner == 0 {
                // SAFETY: the end of the thrown-away child.
                unsafe { _exit(if own_proc() { 0 } else { 1 }) };
            }
            if inner < 0 {
                1
            } else {
                wait_for(inner)
            }
        } else {
            1
        };
        // SAFETY: the end of the thrown-away child.
        unsafe { _exit(code) };
    }
    wait_for(pid) == 0
}

/// Enter the namespaces where [`wanted`] says so. Returns in pid 2, or
/// where the kernel refuses the user namespace (the ladder decides as
/// before); the other two processes exit with the code of pid 2.
///
/// ⛔ Call it first in `main`, with one thread.
pub fn enter_if(argv: &[String]) {
    // SAFETY: getuid and getgid cannot fail.
    let (uid, gid) = unsafe { (getuid(), getgid()) };
    // T-1908: uid 0 has the right to a UTS namespace of its own; `run
    // --hostname` takes one, and each other namespace stays pod-enter's.
    if uid == 0
        && argv.get(1).map(String::as_str) == Some("run")
        && argv[2..].iter().any(|a| a == "--hostname" || a.starts_with("--hostname="))
        // SAFETY: one thread.
        && unsafe { unshare(CLONE_NEWUTS) } == 0
    {
        UTS.store(true, Ordering::Relaxed);
    }
    if !wanted(argv, uid, std::env::var(VAR).ok().as_deref()) {
        return;
    }
    if !trial(uid, gid) {
        return;
    }
    if !open_namespaces(uid, gid) {
        // The trial passed, so this is a change of the host in between.
        eprintln!("podvm run: the user namespace closed after its trial (TODO/probe.md T-1909)");
        std::process::exit(125);
    }
    // SAFETY: one thread; the child returns into main.
    let pid1 = unsafe { fork() };
    if pid1 < 0 {
        eprintln!(
            "podvm run: fork: {} (TODO/probe.md T-1909)",
            std::io::Error::last_os_error()
        );
        std::process::exit(125);
    }
    if pid1 > 0 {
        CHILD.store(pid1, Ordering::Relaxed);
        relay_signals();
        std::process::exit(wait_for(pid1));
    }
    // pid 1 of the PID namespace.
    // SAFETY: one thread.
    let pid2 = unsafe { fork() };
    if pid2 < 0 {
        std::process::exit(125);
    }
    if pid2 == 0 {
        const O_PATH_DIR_CLOEXEC: i32 = 0o10000000 | 0o200000 | 0o2000000;
        // SAFETY: a NUL-ended literal.
        let host = unsafe { open(c"/proc".as_ptr(), O_PATH_DIR_CLOEXEC) };
        HOST_PROC.store(host, Ordering::Relaxed);
        if !own_proc() {
            eprintln!(
                "podvm run: /proc of the PID namespace: {} (TODO/probe.md T-1909)",
                std::io::Error::last_os_error()
            );
            std::process::exit(125);
        }
        ACTIVE.store(true, Ordering::Relaxed);
        UTS.store(true, Ordering::Relaxed);
        return;
    }
    CHILD.store(pid2, Ordering::Relaxed);
    relay_signals();
    let mut code = 125;
    loop {
        let mut st = 0;
        // SAFETY: st lives for the call.
        let r = unsafe { waitpid(-1, &mut st, 0) };
        // SAFETY: errno of this thread.
        if r < 0 && unsafe { *__errno_location() } == EINTR {
            continue;
        }
        if r < 0 {
            break;
        }
        if r == pid2 {
            code = if st & 0x7f == 0 {
                (st >> 8) & 0xff
            } else {
                128 + (st & 0x7f)
            };
            break;
        }
    }
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn only_a_foreground_run_of_a_caller_that_asks_enters() {
        let run = v(&["podvm", "run", "--rm", "img", "true"]);
        assert!(wanted(&run, 1000, Some("1")));
        assert!(!wanted(&run, 0, Some("1")), "uid 0 has the rights");
        assert!(wanted(&run, 1000, None), "the default");
        assert!(!wanted(&run, 1000, Some("0")), "turned off");
        for d in ["-d", "--detach", "-itd", "--podvm-tier=machine"] {
            let a = v(&["podvm", "run", d, "img", "true"]);
            assert!(!wanted(&a, 1000, Some("1")), "{d}");
        }
        assert!(wanted(&v(&["podvm", "run", "-it", "img"]), 1000, Some("1")));
        assert!(!wanted(
            &v(&["podvm", "exec", "c", "true"]),
            1000,
            Some("1")
        ));
        let m = maps(1000, 100);
        assert_eq!(m[1].1, "0 1000 1");
        assert_eq!(m[2].1, "0 100 1");
    }
}
