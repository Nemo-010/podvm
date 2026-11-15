//! PodUtils/podvm#402 and PodUtils/podrunner#115 (TODO/enter.md T-1908):
//! `--hostname` on the native-static rung. The rung has no UTS namespace,
//! so the handler answers `uname` with the name of `PODVM_HOSTNAME` in the
//! nodename field, over the host's answer. `sethostname` keeps the honest
//! `EPERM` of the kernel.
//!
//! ⛔ The handler sees each system call of the payload (Syscall User
//! Dispatch), so a static program sees the name too, unlike the
//! interposer.

use crate::sys::{self, sc};

const UNAME: i64 = 63;
/// The offset of the nodename field in a `struct utsname`.
const NODENAME: usize = 65;

static NAME: crate::state::Fixed = crate::state::Fixed::new();

/// Set the name from the value of `PODVM_HOSTNAME`. False where it is
/// empty or longer than 64 bytes.
pub fn set(name: &[u8]) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    NAME.set(name)
}

/// Put the name into the nodename field of `u`, where a name is set.
pub fn patch(u: &mut [u8; 390]) {
    let name = NAME.get();
    if name.is_empty() {
        return;
    }
    let field = &mut u[NODENAME..NODENAME + 65];
    field.fill(0);
    field[..name.len()].copy_from_slice(name);
}

/// The call of `uname` where a name is set, else None.
pub fn call(n: i64, a: [i64; 6]) -> Option<i64> {
    if n != UNAME || NAME.get().is_empty() {
        return None;
    }
    let mut u = [0u8; 390];
    // SAFETY: uname writes 390 bytes.
    let r = unsafe { sc(UNAME, u.as_mut_ptr() as i64, 0, 0) };
    if r < 0 {
        return Some(r);
    }
    if crate::tier::on() {
        crate::tier::fill_uname(&mut u);
    }
    patch(&mut u);
    if a[0] == 0 {
        return Some(-sys::EFAULT);
    }
    // SAFETY: the payload gave a[0] as a struct utsname.
    unsafe { core::ptr::copy_nonoverlapping(u.as_ptr(), a[0] as *mut u8, u.len()) };
    Some(0)
}
