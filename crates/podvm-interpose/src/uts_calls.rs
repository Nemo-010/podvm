//! PodUtils/podvm#402 and PodUtils/podrunner#115 (TODO/enter.md T-1908):
//! `--hostname` on the loader family. The rung has no UTS namespace, so
//! `uname` and `gethostname` answer the name of `PODVM_HOSTNAME` where it is
//! set, and forward unchanged where it is not. libc reaches the kernel for
//! `gethostname` with no call through the symbol table, so both symbols are
//! needed. `sethostname` keeps the honest `EPERM` of the kernel.
//!
//! ⛔ A static program makes the system call itself and sees the host's
//! name; the native-static rung covers that case in its handler.

use super::*;

/// The variable that holds the name.
pub const HOSTNAME_VAR: &[u8] = b"PODVM_HOSTNAME";

/// The offset of the nodename field in a `struct utsname` (6 fields of 65
/// bytes, glibc and musl).
const NODENAME: usize = 65;

const ENAMETOOLONG: c_int = 36;

/// The name, where it is set and at most 64 bytes.
///
/// # Safety
/// Reads the process's environ, which the kernel keeps valid for the
/// process's life.
unsafe fn name() -> Option<&'static [u8]> {
    let (p, n) = unsafe { crate::map::lookup_env(HOSTNAME_VAR) }?;
    (n > 0 && n <= 64).then(|| unsafe { core::slice::from_raw_parts(p, n) })
}

/// # Safety
/// The payload's own contract for `uname(2)`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn uname(buf: *mut c_void) -> c_int {
    let Some(f) = next_uname() else {
        set_errno(EINVAL);
        return -1;
    };
    let rc = unsafe { f(buf) };
    if rc == 0
        && !buf.is_null()
        && let Some(n) = unsafe { name() }
    {
        // SAFETY: the payload gave buf as a struct utsname, and the real
        // call filled it.
        let field = unsafe { core::slice::from_raw_parts_mut((buf as *mut u8).add(NODENAME), 65) };
        field.fill(0);
        field[..n.len()].copy_from_slice(n);
    }
    rc
}

/// # Safety
/// The payload's own contract for `gethostname(2)`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gethostname(out: *mut c_char, len: usize) -> c_int {
    let Some(n) = (unsafe { name() }) else {
        let Some(f) = next_gethostname() else {
            set_errno(EINVAL);
            return -1;
        };
        return unsafe { f(out, len) };
    };
    // As glibc: a buffer with no room for the name and its NUL fails.
    if len <= n.len() {
        set_errno(ENAMETOOLONG);
        return -1;
    }
    // SAFETY: the payload gave out with room for len bytes.
    unsafe {
        core::ptr::copy_nonoverlapping(n.as_ptr(), out as *mut u8, n.len());
        *out.add(n.len()) = 0;
    }
    0
}
