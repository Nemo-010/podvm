//! TODO/machine.md T-1875: the fetch of the store copy of QEMU
//! (`podvm_windows::tools`) for a host with no emulator, no `qemu-img` or
//! no UEFI pair.
//!
//! ⛔ Each file is checked against its size and sha256 before it lands
//! (`podvm_windows::fetch::store`), and the store is written into a
//! directory beside it and renamed in one step: a fetch that stops leaves
//! no half store that a later run trusts.

use std::path::{Path, PathBuf};

use podvm_image::error::EXIT_RUNTIME_ERROR;
use podvm_windows::tools::{self, Dest, Tool};

/// The store copy under its root, fetched where it is not complete.
/// Prints one line before the fetch and one after, on stderr.
pub(crate) fn ensure(verb: &str) -> Result<PathBuf, i32> {
    let root = tools::root();
    if tools::complete(&root) {
        return Ok(root);
    }
    eprintln!(
        "podvm {verb}: this host has no QEMU; podvm fetches the static QEMU {} and its \
         firmware ({} MiB) into {} (TODO/machine.md T-1875)",
        tools::QEMU_VERSION,
        tools::total_bytes() >> 20,
        root.display()
    );
    let part = root.with_extension("part");
    let _ = std::fs::remove_dir_all(&part);
    let done = fetch_all(&part);
    if let Err(why) = done {
        let _ = std::fs::remove_dir_all(&part);
        eprintln!("podvm {verb}: the store copy of QEMU: {why}");
        return Err(EXIT_RUNTIME_ERROR);
    }
    let _ = std::fs::remove_dir_all(&root);
    std::fs::rename(&part, &root).map_err(|why| {
        eprintln!("podvm {verb}: {}: {why}", root.display());
        EXIT_RUNTIME_ERROR
    })?;
    eprintln!(
        "podvm {verb}: the store copy of QEMU {} is in {}",
        tools::QEMU_VERSION,
        root.display()
    );
    Ok(root)
}

/// TODO/machine.md T-1875: the emulator of the Linux guest of the machine
/// tier: the host one on PATH, else the store copy, fetched where it is
/// not there.
pub(crate) fn emulator_for(verb: &str) -> Result<PathBuf, i32> {
    if let Some(p) = crate::tier::which_qemu("qemu-system-x86_64") {
        return Ok(p);
    }
    let root = ensure(verb)?;
    Ok(tools::bin(&root, "qemu-system-x86_64"))
}

/// Each file of the table into `part`.
fn fetch_all(part: &Path) -> Result<(), String> {
    let fsize = super::plan::ceiling().map_err(|_| "RLIMIT_FSIZE could not be read".to_string())?;
    for t in tools::TOOLS {
        let dest = t.path_in(part);
        let dir = dest.parent().unwrap_or(part);
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let download = match t.dest {
            Dest::ShareBz2 => dir.join(t.name),
            _ => dest.clone(),
        };
        let mut origin = super::origin::Http {
            url: t.url.to_string(),
        };
        let ceiling = podvm_windows::Ceiling {
            max_bytes: t.size,
            fsize,
        };
        podvm_windows::fetch::store(
            &mut origin,
            &download,
            Some(t.sha256),
            ceiling,
            super::origin::TRIES,
        )
        .map_err(|e| format!("{}: {e}", t.name))?;
        finish(t, &download, &dest)?;
    }
    Ok(())
}

/// The step after the check: a program gets mode 0755, and a `.bz2` file
/// is decompressed and removed.
fn finish(t: &Tool, download: &Path, dest: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    match t.dest {
        Dest::Bin => std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("{}: {e}", dest.display())),
        Dest::Share => Ok(()),
        Dest::ShareBz2 => {
            let input = std::fs::File::open(download)
                .map_err(|e| format!("{}: {e}", download.display()))?;
            let mut out =
                std::fs::File::create(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
            std::io::copy(&mut bzip2::read::BzDecoder::new(input), &mut out)
                .map_err(|e| format!("{}: not a bzip2 stream: {e}", t.name))?;
            std::fs::remove_file(download).map_err(|e| format!("{}: {e}", download.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `.bz2` download becomes its decompressed file, and the download
    /// goes; a program gets mode 0755.
    #[test]
    fn tools_finish_decompresses_and_marks_programs() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("podvm-tools-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bz = dir.join("v.fd.bz2");
        let mut enc =
            bzip2::write::BzEncoder::new(std::fs::File::create(&bz).unwrap(), Default::default());
        enc.write_all(b"vars of edk2").unwrap();
        enc.finish().unwrap();
        let t = Tool {
            name: "v.fd.bz2",
            url: "https://example.invalid/v.fd.bz2",
            size: 1,
            sha256: "-",
            dest: Dest::ShareBz2,
        };
        finish(&t, &bz, &dir.join("v.fd")).unwrap();
        assert_eq!(std::fs::read(dir.join("v.fd")).unwrap(), b"vars of edk2");
        assert!(!bz.exists());
        let p = dir.join("qemu-img");
        std::fs::write(&p, b"#!").unwrap();
        let t = Tool {
            dest: Dest::Bin,
            name: "qemu-img",
            ..t
        };
        finish(&t, &p, &p).unwrap();
        assert_eq!(
            std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o755
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
