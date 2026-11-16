//! TODO/machine.md T-1867: the log stick of an install. Setup of Windows
//! Server 2025 fails in the apply of `install.wim`, and its error stays in
//! the RAM disk of WinPE (`X:\Windows\Panther`, `X:\$Windows.~BT`), which
//! the power-off loses.
//!
//! The install attaches a second USB stick, writable, with one file:
//! [`SCRIPT`]. The windowsPE pass of the answer file runs [`START`], which
//! finds the stick by that file and starts the script in its own window
//! (`start`), so Setup does not wait for it. The script copies the logs of
//! WinPE and of Setup to `PELOGS` on the stick each 30 s, until the
//! power-off. Windows writes through to a removable drive by default
//! ("Quick removal"), so the copies are in the image file when the
//! emulator stops. `PODVM_WINDOWS_KEEP_FAILED` keeps the image file.
//!
//! The answer stick stays read-only: it holds the password of the guest.

use std::path::{Path, PathBuf};

use crate::plan::{Medium, MediumKind};

/// The name of the script on the log stick. It is also the mark that
/// [`START`] looks for.
pub const SCRIPT: &str = "PVLOGS.CMD";

/// The name of the image file of the log stick, beside the root disk of
/// the install and in the directory of `PODVM_WINDOWS_KEEP_FAILED`.
pub const IMAGE: &str = "pelogs.img";

/// The directory on the log stick that the script writes.
pub const DIR: &str = "PELOGS";

/// The command of the windowsPE pass: find the drive with [`SCRIPT`] and
/// start the script in a new, minimized window. `cmd` ends at once, so
/// the `RunSynchronous` of Setup ends at once.
pub const START: &str = r#"cmd /c "for %d in (C D E F G H I J K L M N O P Q R S T U V W Y Z) do @if exist %d:\PVLOGS.CMD start "podvm-logs" /min cmd /c %d:\PVLOGS.CMD""#;

/// The script: CRLF lines for `cmd`. `%~d0` is the drive of the script.
/// `ping` to the loopback waits 30 s; `timeout` and `waitfor` are the
/// waits of a WinPE where `ping` fails (each one fails at once where it
/// is missing). `xcopy /c` continues after an error, for example a full
/// stick.
pub const SCRIPT_TEXT: &str = "@echo off\r\n\
rem podvm, TODO/machine.md T-1867: copy the logs of WinPE and of Setup\r\n\
rem to this stick each 30 s, until the power-off.\r\n\
set T=%~d0\\PELOGS\r\n\
if not exist %T% mkdir %T%\r\n\
set /a N=0\r\n\
:again\r\n\
set /a N+=1\r\n\
xcopy \"X:\\Windows\\Panther\" \"%T%\\XPANTHER\\\" /e /c /h /i /y /q >nul 2>&1\r\n\
xcopy \"X:\\$Windows.~BT\\Sources\\Panther\" \"%T%\\XBTPANTH\\\" /e /c /h /i /y /q >nul 2>&1\r\n\
xcopy \"X:\\Sources\\Panther\" \"%T%\\XSRCPANT\\\" /e /c /h /i /y /q >nul 2>&1\r\n\
xcopy \"X:\\Windows\\INF\\setupapi*.log\" \"%T%\\XINF\\\" /c /h /i /y /q >nul 2>&1\r\n\
xcopy \"X:\\Windows\\Logs\" \"%T%\\XLOGS\\\" /e /c /h /i /y /q >nul 2>&1\r\n\
dir /s /b /a-d X:\\*.log > \"%T%\\XLOGLIST.TXT\" 2>&1\r\n\
echo pass %N% at %DATE% %TIME%> \"%T%\\STAMP.TXT\"\r\n\
ping -n 31 127.0.0.1 >nul 2>&1 || timeout /t 30 /nobreak >nul 2>&1 || waitfor /t 30 PodvmLogsNever >nul 2>&1\r\n\
goto again\r\n";

/// The FAT image of the log stick: the script only.
pub fn image() -> crate::Result<Vec<u8>> {
    let mut fat = crate::Fat16::with_oem("PODVM")?;
    fat.put(SCRIPT, SCRIPT_TEXT.as_bytes())?;
    Ok(fat.into_image())
}

/// The image file of the log stick of an install whose root disk is
/// `root`.
pub fn path_beside(root: &Path) -> PathBuf {
    root.parent().unwrap_or(Path::new(".")).join(IMAGE)
}

/// Write the log stick to `path`, and give its medium: a writable USB
/// stick on the xHCI bus, not the boot medium.
pub fn write(path: &Path) -> std::io::Result<Medium> {
    let bytes = image().map_err(|e| std::io::Error::other(e.to_string()))?;
    let _ = std::fs::remove_file(path);
    std::fs::write(path, bytes)?;
    Ok(Medium {
        kind: MediumKind::Usb,
        path: path.to_path_buf(),
        readonly: false,
        boot: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The image holds the script byte for byte, and the command of the
    /// windowsPE pass looks for the same name.
    #[test]
    fn pe_logs_stick_holds_the_script_that_the_answer_file_starts() {
        let fat = crate::Fat16::from_image(image().unwrap()).expect("a FAT image");
        assert_eq!(fat.get(SCRIPT).unwrap(), SCRIPT_TEXT.as_bytes());
        assert!(START.contains(&format!(r"%d:\{SCRIPT}")), "{START}");
        // `start` lets Setup go on; X: is the RAM disk, not the stick.
        assert!(
            START.contains("start ") && !START.contains(" X "),
            "{START}"
        );
        assert!(
            START.len() < 260,
            "a Path of Setup has at most 259 characters"
        );
        for dir in [r"X:\Windows\Panther", r"X:\$Windows.~BT\Sources\Panther"] {
            assert!(SCRIPT_TEXT.contains(dir), "{dir}");
        }
        assert!(SCRIPT_TEXT.contains(&format!(r"%~d0\{DIR}")));
        assert!(SCRIPT_TEXT.contains("goto again"));
        // Each line ends in CRLF (`str::lines` drops the CR, so split).
        assert!(SCRIPT_TEXT.ends_with("\r\n"));
        assert!(SCRIPT_TEXT
            .split('\n')
            .all(|l| l.is_empty() || l.ends_with('\r')));
    }

    /// The emulator line takes the log stick writable, and the answer
    /// stick read-only.
    #[test]
    fn pe_logs_stick_is_a_writable_usb_drive() {
        let dir = std::env::temp_dir().join(format!("podvm-pelogs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = path_beside(&dir.join("run.qcow2"));
        assert_eq!(path, dir.join(IMAGE));
        let m = write(&path).unwrap();
        assert!(!m.readonly && m.kind == MediumKind::Usb && !m.boot);
        let mut options = crate::plan::Options::default();
        options
            .media
            .push(crate::media::medium(dir.join("answer.img")));
        options.media.push(m);
        let plan = crate::Plan {
            emulator: "qemu-system-x86_64".into(),
            accel: crate::Accel::Tcg,
            share: "/usr/share/qemu".into(),
            firmware_code: "/code.fd".into(),
            firmware_vars: dir.join("vars.fd"),
            root: dir.join("run.qcow2"),
            mailbox: dir.join("mailbox.img"),
            serial: dir.join("serial.log"),
            monitor: dir.join("qmp.sock"),
            memory_mib: 4096,
            cpus: 2,
            emu_args: Vec::new(),
            options,
        };
        let argv = crate::argv(&plan);
        let logs: Vec<&String> = argv.iter().filter(|a| a.contains(IMAGE)).collect();
        assert_eq!(logs.len(), 1, "{argv:?}");
        assert!(!logs[0].contains("readonly"), "{logs:?}");
        assert!(
            argv.iter()
                .any(|a| a.contains("answer.img") && a.contains("readonly=on")),
            "{argv:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
