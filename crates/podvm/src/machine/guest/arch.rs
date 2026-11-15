//! TODO/machine.md T-1890: the architecture of the guest of the machine
//! tier follows the image (`--platform linux/arm64`). An x86_64 guest is
//! the pinned path (`qemu-system-x86_64 -M pc`, the Alpine kernel of
//! `pod-services`); an arm64 guest boots with `qemu-system-aarch64 -M virt`
//! and an aarch64 kernel. A guest of another architecture than the host
//! runs under TCG only.
//!
//! ⛔ The kernel of an arm64 guest has no pin in `pod-services` yet
//! (COORD-128): until a tag of podcrates carries one,
//! `PODVM_MACHINE_KERNEL_ARM64` names the kernel file, as
//! `PODVM_MACHINE_KERNEL` does for x86_64, and podvm records its first use.

use std::path::{Path, PathBuf};

use podvm_windows::Accel;

use super::kernel::Assets;

/// The architecture of a guest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    Amd64,
    Arm64,
}

impl Arch {
    /// The guest of an OCI architecture, or `None` for one with no guest.
    pub fn of(oci: &str) -> Option<Arch> {
        match oci {
            "amd64" => Some(Arch::Amd64),
            "arm64" => Some(Arch::Arm64),
            _ => None,
        }
    }

    /// The architecture of this host.
    pub fn host() -> Arch {
        if cfg!(target_arch = "aarch64") {
            Arch::Arm64
        } else {
            Arch::Amd64
        }
    }

    /// The OCI name.
    pub fn oci(self) -> &'static str {
        match self {
            Arch::Amd64 => "amd64",
            Arch::Arm64 => "arm64",
        }
    }

    /// The emulator of the architecture.
    pub fn emulator(self) -> &'static str {
        match self {
            Arch::Amd64 => "qemu-system-x86_64",
            Arch::Arm64 => "qemu-system-aarch64",
        }
    }

    /// The machine of `-M`.
    pub fn machine(self) -> &'static str {
        match self {
            Arch::Amd64 => "pc,acpi=off",
            Arch::Arm64 => "virt",
        }
    }

    /// The first serial port in the guest: the console and the markers.
    pub fn console(self) -> &'static str {
        match self {
            Arch::Amd64 => "ttyS0",
            Arch::Arm64 => "ttyAMA0",
        }
    }
}

/// The accelerator of a guest of `arch`: the rule of the native guest, and
/// TCG only for another architecture (KVM runs the architecture of the
/// host only).
pub fn accel(arch: Arch, want: Option<&str>, kvm_opens: bool) -> Result<Accel, String> {
    if arch == Arch::host() {
        return super::accel(want, kvm_opens);
    }
    match want {
        Some("kvm") => Err(format!(
            "PODVM_MACHINE_ACCEL=kvm, and KVM runs a guest of the architecture of the host \
             only; a linux/{} guest runs under TCG on this host (TODO/machine.md T-1890)",
            arch.oci()
        )),
        _ => Ok(Accel::Tcg),
    }
}

/// The emulator of a guest of `arch`: for x86_64 the host one or the store
/// copy (T-1875); for arm64 the one on PATH.
pub fn emulator(verb: &str, arch: Arch) -> Result<PathBuf, String> {
    match arch {
        Arch::Amd64 => crate::windows::tools::emulator_for(verb)
            .map_err(|_| "no qemu-system-x86_64 and no store copy".to_string()),
        Arch::Arm64 => crate::tier::which_qemu(arch.emulator()).ok_or_else(|| {
            format!(
                "a linux/arm64 guest needs {} on PATH (the package qemu-system-arm), and this \
                 host has none (TODO/machine.md T-1890)",
                arch.emulator()
            )
        }),
    }
}

/// The variable that names the kernel of an arm64 guest.
pub const KERNEL_ARM64: &str = "PODVM_MACHINE_KERNEL_ARM64";

/// The kernel of a guest of `arch`.
pub fn kernel(store_root: &Path, arch: Arch) -> Result<Assets, String> {
    match arch {
        Arch::Amd64 => super::kernel::ensure(store_root),
        Arch::Arm64 => {
            let Some(p) = std::env::var_os(KERNEL_ARM64).map(PathBuf::from) else {
                return Err(format!(
                    "the kernel of a linux/arm64 guest has no pin in pod-services yet \
                     (COORD-128); {KERNEL_ARM64} names an aarch64 kernel file, for example \
                     boot/vmlinuz-virt of alpine-netboot-3.22.6-aarch64.tar.gz \
                     (TODO/machine.md T-1890)"
                ));
            };
            if !p.is_file() {
                return Err(format!(
                    "{KERNEL_ARM64} names {}, which is not a file",
                    p.display()
                ));
            }
            crate::assets::verify(store_root, "machine-kernel-arm64", &p, None)
                .map_err(|e| format!("{KERNEL_ARM64}: {e}"))?;
            Ok(Assets {
                kernel: p,
                modules: Vec::new(),
                volume_modules: Vec::new(),
            })
        }
    }
}

/// The emulator line of `arch`, from the line of the x86_64 guest: the
/// machine and the console of the kernel.
pub fn adapt(argv: &mut [String], arch: Arch) {
    if arch == Arch::Amd64 {
        return;
    }
    let mut it = argv.iter_mut().peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-M" => {
                if let Some(v) = it.next() {
                    *v = arch.machine().to_string();
                }
            }
            "-append" => {
                if let Some(v) = it.next() {
                    *v = v.replace("console=ttyS0", &format!("console={}", arch.console()));
                }
            }
            _ => {}
        }
    }
}

/// The second port (stderr) of an arm64 guest: `-M virt` has one PL011, so
/// the port of the chardev `podvmerr` is a virtio port named `podvm.err`.
pub fn err_port(argv: &mut Vec<String>, arch: Arch) {
    if arch == Arch::Amd64 {
        return;
    }
    if let Some(i) = argv
        .windows(2)
        .position(|w| w[0] == "-serial" && w[1] == "chardev:podvmerr")
    {
        argv.splice(
            i..i + 2,
            [
                "-device".to_string(),
                "virtio-serial-pci,id=podvmes".to_string(),
                "-device".to_string(),
                "virtserialport,chardev=podvmerr,name=podvm.err".to_string(),
            ],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The arm64 line takes `-M virt` and the console of the PL011, and the
    /// x86_64 line stays as it was.
    #[test]
    fn machine_arch_adapts_the_emulator_line() {
        let base =
            super::super::qemu_argv(Accel::Tcg, 256 << 20, Path::new("/k"), Path::new("/i"), &[]);
        let mut x = base.clone();
        adapt(&mut x, Arch::Amd64);
        assert_eq!(x, base);
        let mut a = base.clone();
        adapt(&mut a, Arch::Arm64);
        let line = a.join(" ");
        assert!(line.contains("-M virt"), "{line}");
        assert!(line.contains("console=ttyAMA0"), "{line}");
        assert!(
            !line.contains("pc,acpi=off") && !line.contains("ttyS0"),
            "{line}"
        );
        assert!(line.contains("-cpu max"), "TCG takes the CPU max: {line}");
        let mut e = super::super::stderr::argv(Path::new("/s"));
        err_port(&mut e, Arch::Arm64);
        let e = e.join(" ");
        assert!(
            e.contains("virtserialport,chardev=podvmerr") && !e.contains("-serial chardev"),
            "{e}"
        );
    }

    /// Another architecture runs under TCG, and KVM for it is refused by
    /// name; an architecture with no guest has none.
    #[test]
    fn machine_arch_runs_a_foreign_guest_under_tcg() {
        let foreign = if Arch::host() == Arch::Amd64 {
            Arch::Arm64
        } else {
            Arch::Amd64
        };
        assert_eq!(accel(foreign, None, true), Ok(Accel::Tcg));
        assert!(accel(foreign, Some("kvm"), true)
            .unwrap_err()
            .contains("TCG"));
        assert_eq!(Arch::of("arm64"), Some(Arch::Arm64));
        assert_eq!(Arch::of("riscv64"), None);
    }
}
