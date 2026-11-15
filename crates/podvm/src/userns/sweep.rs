//! The end of the PID namespace of a run: the sweep at the exit of pid 2
//! and the reaper that pid 1 becomes (TODO/probe.md T-1909).

use super::*;

/// In pid 2: name the detached prefill of T-1781 (`keep`), which the
/// sweep at the exit of pid 2 spares. The sweep sends SIGKILL to each
/// other process of the PID namespace but pid 1 and pid 2: a process of
/// the payload that the sweep of the launcher (T-1708) did not end would
/// keep pid 1 waiting. Outside a namespace of podvm's own it does nothing.
pub fn sweep(keep: Option<u32>) {
    if !active() {
        return;
    }
    if let Some(k) = keep {
        KEEP.store(k, Ordering::Relaxed);
    }
}

/// The prefill that [`sweep`] keeps, 0 for none.
pub(super) static KEEP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The sweep itself, at each exit of pid 2 (`atexit`): `main`'s own end,
/// and each `std::process::exit` of a verb (the bound of `--timeout`).
/// The pid that registered [`sweep_at_exit`]: a child of fork inherits the
/// handler, and must not sweep.
pub(super) static OWNER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub(super) extern "C" fn sweep_at_exit() {
    let me = std::process::id();
    if OWNER.load(Ordering::Relaxed) != me {
        return;
    }
    let keep = Some(KEEP.load(Ordering::Relaxed)).filter(|k| *k != 0);
    let Ok(rd) = std::fs::read_dir("/proc") else {
        return;
    };
    for e in rd.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        if pid == 1 || pid == me || Some(pid) == keep {
            continue;
        }
        // SAFETY: kill takes two integers; the pids are of this namespace.
        unsafe { kill(pid as i32, 9) };
    }
}

pub(super) const SIGALRM: i32 = 14;
/// How long pid 1 waits for the detached prefill after pid 2 ended.
pub(super) const LINGER_SECS: u32 = 600;

/// In pid 1: SIGKILL to each process of the PID namespace.
pub(super) extern "C" fn end_namespace(_: i32) {
    // SAFETY: kill is async-signal-safe; -1 in pid 1 names each process of
    // the namespace but pid 1.
    unsafe { kill(-1, 9) };
}

/// pid 1 after pid 2 ended: the same process with no word of the run in
/// its command line, which reaps until no process of the namespace is
/// left, for at most [`LINGER_SECS`].
pub const REAPER_ARG: &str = "--podvm-ns-reaper";

pub(super) fn reap_all() -> ! {
    // SAFETY: the handler only calls kill.
    unsafe {
        signal(SIGALRM, end_namespace as *const () as usize);
        alarm(LINGER_SECS);
    }
    loop {
        let mut st = 0;
        // SAFETY: st lives for the call.
        let r = unsafe { waitpid(-1, &mut st, 0) };
        // SAFETY: errno of this thread.
        if r < 0 && unsafe { *__errno_location() } != EINTR {
            std::process::exit(0);
        }
    }
}
