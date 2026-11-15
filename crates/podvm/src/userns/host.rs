//! The host's view of a run in the user namespace: its pids and its
//! device nodes (TODO/probe.md T-1909).

use super::*;

/// A descriptor of the host's `/proc`, opened in pid 2 before its own
/// `/proc` covers it.
pub(super) static HOST_PROC: AtomicI32 = AtomicI32::new(-1);

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

/// The device nodes of the host on the tree of a run in the user
/// namespace: `mknod` is refused there, and the fixups of `pod-rootfs`
/// would leave a regular file in their place (a `/dev/urandom` that ends
/// after 1 MiB). A bind in the mount namespace of pid 2, which the mount
/// namespace of the payload copies; the removal of the tree takes it off
/// first (`podvm_extract::tree::unbind_devices`). A path that is a link or
/// no regular file of the tree is left alone.
pub fn bind_devices(rootfs: &std::path::Path) {
    if !active() {
        return;
    }
    const MS_BIND: u64 = 0x1000;
    let dev = rootfs.join("dev");
    if !std::fs::symlink_metadata(&dev).is_ok_and(|m| m.is_dir()) {
        return;
    }
    for d in podvm_extract::tree::BOUND_DEVICES {
        let target = rootfs.join(d);
        let host = std::path::Path::new("/").join(d);
        match std::fs::symlink_metadata(&target) {
            Ok(m) if !m.is_file() => continue,
            Ok(_) => {}
            Err(_) => {
                if std::fs::File::create(&target).is_err() {
                    continue;
                }
            }
        }
        let (Ok(src), Ok(dst)) = (
            std::ffi::CString::new(host.as_os_str().as_encoded_bytes()),
            std::ffi::CString::new(target.as_os_str().as_encoded_bytes()),
        ) else {
            continue;
        };
        // SAFETY: NUL-ended paths; a bind takes no type and no data.
        unsafe {
            mount(
                src.as_ptr(),
                dst.as_ptr(),
                core::ptr::null(),
                MS_BIND,
                core::ptr::null(),
            )
        };
    }
}
