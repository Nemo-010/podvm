//! TODO/machine.md T-1875: the emulator, `qemu-img` and the firmware that
//! podvm fetches into its store when the host has none.
//!
//! The binaries are the static builds of QEMU 9.2.0 that pkgforge
//! publishes (`ppkg.qemu`, musl, no `PT_INTERP`). The firmware and the
//! option ROMs are the prebuilt files of `pc-bios/` in the QEMU tree at the
//! tag `v9.2.0`: SeaBIOS, the VGA BIOS, the iPXE EFI ROMs and edk2 (code
//! and variables, compressed with bzip2 in the tree). Each file has a size
//! and a sha256 here; podvm checks both before it uses a byte.
//!
//! The store keeps the layout of a relocatable install: `bin/` for the two
//! programs, `share/qemu/` for the data, so the emulator finds its firmware
//! with no `-L` (`bin/../share/qemu`).
//!
//! ⛔ The host QEMU wins where it is present: the store copy is the
//! fallback of a host with none (the Decision of T-1875).

use std::path::{Path, PathBuf};

/// The version of QEMU in the store.
pub const QEMU_VERSION: &str = "9.2.0";

/// Where one file of the table goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dest {
    /// `bin/NAME`, mode 0755.
    Bin,
    /// `share/qemu/NAME`.
    Share,
    /// `share/qemu/NAME` without `.bz2`, decompressed after the check.
    ShareBz2,
}

/// One file of the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tool {
    /// The name of the download.
    pub name: &'static str,
    pub url: &'static str,
    /// The size of the download in bytes.
    pub size: u64,
    /// The sha256 of the download, lower-case hex.
    pub sha256: &'static str,
    pub dest: Dest,
}

#[cfg(test)]
const PKGFORGE: &str = "https://api.ghcr.pkgforge.dev/pkgforge/bincache/qemu/ppkg";
#[cfg(test)]
const PC_BIOS: &str = "https://raw.githubusercontent.com/qemu/qemu/v9.2.0/pc-bios";

macro_rules! bin {
    ($name:literal, $size:literal, $sha:literal) => {
        Tool {
            name: $name,
            url: concat!(
                "https://api.ghcr.pkgforge.dev/pkgforge/bincache/qemu/ppkg/",
                $name,
                "?tag=9.2.0-x86_64-linux&download=",
                $name
            ),
            size: $size,
            sha256: $sha,
            dest: Dest::Bin,
        }
    };
}

macro_rules! bios {
    ($name:literal, $size:literal, $sha:literal, $dest:expr) => {
        Tool {
            name: $name,
            url: concat!(
                "https://raw.githubusercontent.com/qemu/qemu/v9.2.0/pc-bios/",
                $name
            ),
            size: $size,
            sha256: $sha,
            dest: $dest,
        }
    };
}

/// The files of the store, pinned 2026-11-15.
pub const TOOLS: &[Tool] = &[
    bin!(
        "qemu-system-x86_64",
        42_899_120,
        "b0597850cc3aa3d473410d8da27102eddfcf3f23abbe58bd1efc7a06f197b0a1"
    ),
    bin!(
        "qemu-img",
        11_009_488,
        "7311a2598164b3055097f8f0484c1bbb0cd92463889fd8108c2f40fcc9f597c6"
    ),
    bios!(
        "bios-256k.bin",
        262_144,
        "f1d4f396011197eb989029659cde250751cc711c336b8fbbe6f77cfe0dc5dcd8",
        Dest::Share
    ),
    bios!(
        "bios-microvm.bin",
        131_072,
        "42780e1ab9161752848475529a25c39d7c76baee82a1b2ddff666c2f7de7bbee",
        Dest::Share
    ),
    bios!(
        "qboot.rom",
        65_536,
        "9b9dfc6c25740d6225625570d71cab6805cc9216e68c8932e343266daaeb8c4b",
        Dest::Share
    ),
    bios!(
        "kvmvapic.bin",
        9_216,
        "cdf057a71b07e3b52b19cbe210bdefa59250d01a9810b960f7fe1f98eed95a27",
        Dest::Share
    ),
    bios!(
        "linuxboot_dma.bin",
        1_536,
        "9c49e255340c78fc12e54ed043462bca02fb7fca29b7cfab62ff88a5344b6950",
        Dest::Share
    ),
    bios!(
        "multiboot_dma.bin",
        1_024,
        "e01667420ee78a318ea44627d2dd68058d3377d024e6eb7237700afe2e86507d",
        Dest::Share
    ),
    bios!(
        "pvh.bin",
        1_536,
        "918ff03a118615c2f4611a5df2d9496aabe099b5cfaaf69e544ebf973305d59d",
        Dest::Share
    ),
    bios!(
        "vgabios-stdvga.bin",
        39_424,
        "651513519f9e0d5b99d3b051a8f5c68db69e987339b59a441d371068c34c146b",
        Dest::Share
    ),
    bios!(
        "vgabios-virtio.bin",
        39_424,
        "746f38c2023dbbc071503113f970bcf69d49c6e7c55fa8c0e3748dbccba42772",
        Dest::Share
    ),
    bios!(
        "vgabios-cirrus.bin",
        38_912,
        "305d3b15f67e65f1899a1664621f6b471e4345d33fd9eb4f6320b4559f6a04f2",
        Dest::Share
    ),
    bios!(
        "vgabios-bochs-display.bin",
        28_672,
        "d5fd6992dfdf0c980996b79c5ea13b9c8b36342c1dc94fcd682a6c3e8df850ff",
        Dest::Share
    ),
    bios!(
        "vgabios-ramfb.bin",
        28_672,
        "31b26b2288ed5594b65a234e797ee7105d225d48086170b27ab518c3fb69b7de",
        Dest::Share
    ),
    bios!(
        "efi-e1000e.rom",
        159_232,
        "102c20348be2d0ebe45a77b076b31848182c2f7b4364775b6bc2ece6e97b4024",
        Dest::Share
    ),
    bios!(
        "efi-e1000.rom",
        159_232,
        "f37222fcdf0481bfbaf7a16d9aaf740aeb41d3d24c187801eb6e9a628cec0fc8",
        Dest::Share
    ),
    bios!(
        "efi-virtio.rom",
        160_768,
        "26be36901db7f8181c306cc62bd74891d8646528965a78e40cceadba5dd7c8e7",
        Dest::Share
    ),
    bios!(
        "edk2-x86_64-code.fd.bz2",
        1_907_255,
        "f4d7c29fccb5e1d4d851eaddc41a5c4feec7de96ca7ad9bbf96f4d09838ef973",
        Dest::ShareBz2
    ),
    bios!(
        "edk2-i386-vars.fd.bz2",
        190,
        "83470f393b9a200838b2292a65ca5708a109005d65a6422f3fa23cb21c8179b9",
        Dest::ShareBz2
    ),
];

/// The directory of the store copy: `PODVM_TOOLS_DIR` where it is set,
/// else `tools/qemu-VERSION` beside the Windows catalog.
pub fn root() -> PathBuf {
    if let Some(p) = std::env::var_os("PODVM_TOOLS_DIR")
        && !p.is_empty()
    {
        return PathBuf::from(p);
    }
    let base = crate::base_cache();
    let windows = base.parent().unwrap_or(Path::new("."));
    let data = windows.parent().unwrap_or(Path::new("."));
    data.join("tools").join(format!("qemu-{QEMU_VERSION}"))
}

impl Tool {
    /// The file that the download becomes under `root`.
    pub fn path_in(&self, root: &Path) -> PathBuf {
        match self.dest {
            Dest::Bin => root.join("bin").join(self.name),
            Dest::Share => root.join("share/qemu").join(self.name),
            Dest::ShareBz2 => root
                .join("share/qemu")
                .join(self.name.strip_suffix(".bz2").unwrap_or(self.name)),
        }
    }
}

/// The program `name` of the store under `root`.
pub fn bin(root: &Path, name: &str) -> PathBuf {
    root.join("bin").join(name)
}

/// The data directory of the store under `root`.
pub fn share(root: &Path) -> PathBuf {
    root.join("share/qemu")
}

/// The UEFI pair of the store under `root`: `(code, vars)`.
pub fn firmware(root: &Path) -> (PathBuf, PathBuf) {
    let s = share(root);
    (s.join("edk2-x86_64-code.fd"), s.join("edk2-i386-vars.fd"))
}

/// True when each file of the table is in the store under `root`.
pub fn complete(root: &Path) -> bool {
    TOOLS.iter().all(|t| t.path_in(root).is_file())
}

/// The bytes of all downloads.
pub fn total_bytes() -> u64 {
    TOOLS.iter().map(|t| t.size).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each pin is a size and 64 hex digits, each name is unique, each
    /// URL is https on one of the two sources, and the store layout gives
    /// the emulator, `qemu-img`, the data and the UEFI pair.
    #[test]
    fn tools_pins_are_complete_and_the_layout_is_relocatable() {
        let mut names = std::collections::BTreeSet::new();
        for t in TOOLS {
            assert!(names.insert(t.name), "{} twice", t.name);
            assert!(t.size > 0, "{}", t.name);
            assert_eq!(t.sha256.len(), 64, "{}", t.name);
            assert!(t
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
            assert!(
                t.url.starts_with(PKGFORGE) || t.url.starts_with(PC_BIOS),
                "{}",
                t.url
            );
            assert!(t.url.contains(t.name), "{}", t.url);
            assert_eq!(
                t.dest == Dest::ShareBz2,
                t.name.ends_with(".bz2"),
                "{}",
                t.name
            );
        }
        let root = Path::new("/store/tools/qemu-9.2.0");
        let paths: Vec<PathBuf> = TOOLS.iter().map(|t| t.path_in(root)).collect();
        for want in [
            bin(root, "qemu-system-x86_64"),
            bin(root, "qemu-img"),
            share(root).join("bios-256k.bin"),
            firmware(root).0,
            firmware(root).1,
        ] {
            assert!(
                paths.contains(&want),
                "{} is not in the store",
                want.display()
            );
        }
        assert_eq!(share(root).parent().unwrap(), root.join("share"));
        assert!(total_bytes() < 64 << 20, "{}", total_bytes());
        assert!(!complete(Path::new("/nonexistent-podvm-tools")));
    }
}
