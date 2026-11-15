//! What tells a removal that a tree is in use, across PID namespaces
//! (podvm's TODO/probe.md T-1909: a foreground run can live in a user and
//! a PID namespace of its own).
//!
//! * A fill (`.fill-PID-ID`) holds an `flock` on `.fill-PID-ID.lock` beside
//!   it while it copies. A sweep of another PID namespace cannot read the
//!   pid of the name, and took a live fill for a dead one.
//! * A run in a user namespace binds four device nodes of the host onto
//!   its tree; a removal takes them off first.

use std::fs;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

unsafe extern "C" {
    fn flock(fd: i32, op: i32) -> i32;
    fn umount2(target: *const std::ffi::c_char, flags: i32) -> i32;
}
const LOCK_EX: i32 = 2;
const LOCK_NB: i32 = 4;

/// The lock of a fill, held until drop.
pub struct FillLock {
    _file: fs::File,
    path: PathBuf,
}

fn lock_path(partial: &Path) -> PathBuf {
    let mut p = partial.as_os_str().to_owned();
    p.push(".lock");
    PathBuf::from(p)
}

impl FillLock {
    /// Make and lock the file of the fill `partial`.
    pub fn take(partial: &Path) -> std::io::Result<FillLock> {
        let path = lock_path(partial);
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        // SAFETY: a descriptor of this call.
        if unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(FillLock { _file: file, path })
    }

    /// `Some(true)` where a process holds the lock of the fill `partial`,
    /// `Some(false)` where its lock file is free, `None` where it has none.
    pub fn held(partial: &Path) -> Option<bool> {
        let file = fs::OpenOptions::new()
            .write(true)
            .open(lock_path(partial))
            .ok()?;
        // SAFETY: a descriptor of this call; a free lock is released with it.
        Some(unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } != 0)
    }

    /// Remove the lock file of a fill that a sweep removed.
    pub fn forget(partial: &Path) {
        let _ = fs::remove_file(lock_path(partial));
    }
}

impl Drop for FillLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// The device nodes that a run in a user namespace binds from the host
/// onto its tree.
pub const BOUND_DEVICES: [&str; 4] = ["dev/null", "dev/zero", "dev/random", "dev/urandom"];

/// Take the binds of [`BOUND_DEVICES`] off the tree `dir` (its `rootfs/`)
/// before its removal: a mount point refuses `unlink` with `EBUSY`. Where
/// nothing is mounted, or this process may not unmount, it does nothing.
/// `UMOUNT_NOFOLLOW`: a link of the payload never aims it out of the tree.
pub fn unbind_devices(dir: &Path) {
    const MNT_DETACH: i32 = 2;
    const UMOUNT_NOFOLLOW: i32 = 8;
    for d in BOUND_DEVICES {
        let path = dir.join("rootfs").join(d);
        let Ok(c) = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) else {
            continue;
        };
        // SAFETY: a NUL-ended path.
        unsafe { umount2(c.as_ptr(), MNT_DETACH | UMOUNT_NOFOLLOW) };
    }
}

/// TODO/probe.md T-1911: the removal of a tree in a user namespace with
/// the subordinate range of the caller, which podvm registers. A run with
/// that range leaves directories of subordinate ids, whose entries the
/// caller cannot remove from outside.
pub static RANGE_REMOVER: std::sync::OnceLock<fn(&std::path::Path) -> bool> =
    std::sync::OnceLock::new();

/// Remove the tree `dir` (the owner's bits added to each directory first);
/// where that fails, through [`RANGE_REMOVER`].
pub fn remove_any(dir: &std::path::Path) -> std::io::Result<bool> {
    match remove_plain(dir) {
        Ok(b) => Ok(b),
        Err(e) => match RANGE_REMOVER.get() {
            Some(f) if f(dir) => Ok(true),
            _ => Err(e),
        },
    }
}

/// Remove the tree `dir`, the owner's bits added to each directory first.
pub fn remove_plain(dir: &std::path::Path) -> std::io::Result<bool> {
    crate::tree::open_dirs(dir).and_then(|()| {
        pod_extract::remove_tree(dir).map_err(|e| std::io::Error::other(e.to_string()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fill_is_live_while_its_lock_is_held() {
        let d = std::env::temp_dir().join(format!("podvm-live-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let partial = d.join(".fill-1-abc");
        assert_eq!(FillLock::held(&partial), None, "no lock file");
        let lock = FillLock::take(&partial).unwrap();
        assert_eq!(FillLock::held(&partial), Some(true));
        drop(lock);
        assert_eq!(FillLock::held(&partial), None, "the drop removes the file");
        let _ = fs::remove_dir_all(&d);
    }
}
