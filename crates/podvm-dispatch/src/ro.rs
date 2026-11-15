//! PodUtils/podrunner#111 (TODO/enter.md T-1901): a `:ro` volume on the
//! native-static rung. A call that writes through a path that resolves
//! into a volume of `PODVM_RO` (`ro:GUEST`), or through a descriptor of a
//! file in one, answers `EROFS` before it runs, as a read-only bind mount
//! does. The rootfs keeps its own rule: `--read-only` stays the interposer's
//! (T-1655).
//!
//! ⛔ The handler sees each system call of the payload (Syscall User
//! Dispatch), so the gate holds for a static program, unlike the interposer;
//! a write through a shared mapping that was opened before is no call and
//! passes, as on a bind mount that becomes read-only later.

use core::sync::atomic::Ordering;

use crate::path::{self, target, Target};
use crate::state::{self, PATH_MAX};
use crate::sys::{self, nr};

/// `EROFS`: a read-only file system.
pub const EROFS: i64 = 30;

// The calls of x86_64 that write through a descriptor.
const FTRUNCATE: i64 = 77;
const FCHMOD: i64 = 91;
const FCHOWN: i64 = 93;
const FSETXATTR: i64 = 190;
const FREMOVEXATTR: i64 = 199;
const FALLOCATE: i64 = 285;

// The flags of open(2) that write: O_WRONLY, O_RDWR, O_CREAT, O_TRUNC.
const WRITE_FLAGS: i64 = 0o1 | 0o2 | 0o100 | 0o1000;

/// Mark the volumes whose guest path `PODVM_RO` names with `ro:`.
pub fn mark(table: &[u8]) {
    for e in table.split(|&c| c == b',') {
        let Some(g) = e.strip_prefix(b"ro:") else {
            continue;
        };
        let g = g.strip_suffix(b"/").unwrap_or(g);
        for v in state::vols() {
            if v.guest.get() == g {
                v.ro.store(true, Ordering::Relaxed);
            }
        }
    }
}

/// True when the guest path is in a `:ro` volume: the volume with the
/// longest guest prefix decides, so a writable volume inside a read-only
/// one stays writable.
pub fn read_only(guest: &[u8]) -> bool {
    let mut best: Option<(&state::Vol, usize)> = None;
    for v in state::vols() {
        let g = v.guest.get();
        let hit =
            guest == g || (guest.len() > g.len() && guest.starts_with(g) && guest[g.len()] == b'/');
        if hit && best.is_none_or(|(_, n)| g.len() > n) {
            best = Some((v, g.len()));
        }
    }
    best.is_some_and(|(v, _)| v.ro.load(Ordering::Relaxed))
}

/// True when the host path is in the host side of a `:ro` volume.
fn host_read_only(host: &[u8]) -> bool {
    let mut best: Option<(&state::Vol, usize)> = None;
    for v in state::vols() {
        let h = v.host.get();
        let hit =
            host == h || (host.len() > h.len() && host.starts_with(h) && host[h.len()] == b'/');
        if hit && best.is_none_or(|(_, n)| h.len() > n) {
            best = Some((v, h.len()));
        }
    }
    best.is_some_and(|(v, _)| v.ro.load(Ordering::Relaxed))
}

/// True when `(dirfd, p)` names a path in a `:ro` volume.
fn path_ro(dirfd: i64, p: i64) -> bool {
    if p == 0 {
        return false;
    }
    // SAFETY: the payload gave p as a path; see `sys::cstr`.
    let Some(path) = (unsafe { sys::cstr(p, PATH_MAX) }) else {
        return false;
    };
    let Some(mut g) = crate::pool::Buf::get() else {
        return false;
    };
    let buf = g.buf();
    match target(dirfd, path, buf) {
        Ok(Target::Root(n)) => read_only(&buf[..n]),
        _ => false,
    }
}

/// True when the descriptor `fd` is a file in a `:ro` volume.
fn fd_ro(fd: i64) -> bool {
    let mut link = [0u8; 32];
    let Some(mut g) = crate::pool::Buf::get() else {
        return false;
    };
    let buf = g.buf();
    let n = sys::readlinkat(sys::AT_FDCWD, path::fd_link(fd, &mut link), &mut buf[..]);
    n > 0 && host_read_only(&buf[..n as usize])
}

/// `Some(-EROFS)` for a call that writes into a `:ro` volume; `None` for
/// each other call, which runs on.
pub fn gate(n: i64, a: [i64; 6]) -> Option<i64> {
    if !state::vols().iter().any(|v| v.ro.load(Ordering::Relaxed)) {
        return None;
    }
    let f = sys::AT_FDCWD;
    let refused = match n {
        nr::OPEN => a[1] & WRITE_FLAGS != 0 && path_ro(f, a[0]),
        nr::OPENAT => a[2] & WRITE_FLAGS != 0 && path_ro(a[0], a[1]),
        nr::CREAT | nr::MKDIR | nr::MKNOD | nr::UNLINK | nr::RMDIR => path_ro(f, a[0]),
        nr::CHMOD | nr::CHOWN | nr::LCHOWN | nr::TRUNCATE | nr::UTIME | nr::UTIMES => {
            path_ro(f, a[0])
        }
        nr::SETXATTR | nr::LSETXATTR | nr::REMOVEXATTR | nr::LREMOVEXATTR => path_ro(f, a[0]),
        nr::MKDIRAT | nr::MKNODAT | nr::UNLINKAT | nr::FCHMODAT | nr::FCHMODAT2 => {
            path_ro(a[0], a[1])
        }
        nr::FCHOWNAT | nr::FUTIMESAT => path_ro(a[0], a[1]),
        // A NULL path is the descriptor itself.
        nr::UTIMENSAT if a[1] == 0 => fd_ro(a[0]),
        nr::UTIMENSAT => path_ro(a[0], a[1]),
        nr::SYMLINK => path_ro(f, a[1]),
        nr::SYMLINKAT => path_ro(a[1], a[2]),
        // A hard link of a read-only file is a writable name of it.
        nr::RENAME | nr::LINK => path_ro(f, a[0]) || path_ro(f, a[1]),
        nr::RENAMEAT | nr::RENAMEAT2 | nr::LINKAT => path_ro(a[0], a[1]) || path_ro(a[2], a[3]),
        FTRUNCATE | FCHMOD | FCHOWN | FSETXATTR | FREMOVEXATTR | FALLOCATE => fd_ro(a[0]),
        _ => false,
    };
    refused.then_some(-EROFS)
}
