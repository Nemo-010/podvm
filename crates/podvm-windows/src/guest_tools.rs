//! TODO/machine.md T-1868: the virtio-win guest tools in the install of a
//! setup ISO (T-1867). The install attaches the virtio-win CD of the
//! catalog ([`crate::catalog::VIRTIO_WIN`]) as one more CD-ROM, and puts
//! [`SCRIPT`] on the mailbox volume. The first logon runs [`SETUP`] before
//! the `READY` marker. The script finds the CD by its MSI file, trusts the
//! certificate of Red Hat, and installs the drivers (with the virtio-serial
//! driver) and the QEMU Guest Agent, each with `msiexec /qn`. It writes the
//! exit codes of `msiexec` and the state of the service `QEMU-GA` into
//! [`REPORT`] on the mailbox, and the driver reads it back ([`report`]).
//!
//! The agent then serves the virtio-serial channel of a machine
//! ([`crate::qga`]). ValidationOS has no Windows Installer service: it
//! keeps the mailbox.

use std::path::Path;

use crate::plan::{Medium, MediumKind};

/// The name of the script on the mailbox volume of an install (8.3).
pub const SCRIPT: &str = "VT.CMD";

/// The name of the report that the script writes on the mailbox volume.
pub const REPORT: &str = "TOOLS.TXT";

/// The line of the first logon: find the mailbox by its marker file and
/// run the script on it. `cmd /c` waits for the script, and the script
/// waits for each `msiexec`.
pub const SETUP: &str = r#"cmd /c "for %d in (D E F G H I J K L M N O P Q R S T U V W X Y Z) do @if exist %d:\WQMARK.TXT call %d:\VT.CMD""#;

/// The script: CRLF lines for `cmd`. `%~d0` is the drive of the mailbox.
/// `start /wait` makes `cmd` wait for `msiexec` and gives its exit code;
/// 0 and 3010 (a restart is needed) are a success.
pub const SCRIPT_TEXT: &str = "@echo off\r\n\
rem podvm, TODO/machine.md T-1868: install the virtio-win guest tools\r\n\
rem from the virtio-win CD: the drivers and the QEMU Guest Agent.\r\n\
set R=%~d0\\TOOLS.TXT\r\n\
set V=\r\n\
for %%d in (D E F G H I J K L M N O P Q R S T U V W X Y Z) do if exist %%d:\\virtio-win-gt-x64.msi set V=%%d:\r\n\
if \"%V%\"==\"\" goto nocd\r\n\
for %%c in (%V%\\cert\\*.cer) do certutil -addstore -f TrustedPublisher \"%%c\" >nul 2>&1\r\n\
for %%c in (%V%\\cert\\*.cer) do certutil -addstore -f Root \"%%c\" >nul 2>&1\r\n\
start \"\" /wait msiexec /i %V%\\virtio-win-gt-x64.msi /qn /norestart ADDLOCAL=ALL\r\n\
set D=%ERRORLEVEL%\r\n\
start \"\" /wait msiexec /i %V%\\guest-agent\\qemu-ga-x86_64.msi /qn /norestart\r\n\
set G=%ERRORLEVEL%\r\n\
echo drivers=%D% agent=%G%> \"%R%\"\r\n\
sc query QEMU-GA >> \"%R%\" 2>&1\r\n\
exit /b 0\r\n\
:nocd\r\n\
echo drivers=nocd agent=nocd> \"%R%\"\r\n\
exit /b 1\r\n";

/// The medium of the virtio-win CD at `iso`: a read-only CD-ROM that the
/// firmware does not boot.
pub fn medium(iso: &Path) -> Medium {
    Medium {
        kind: MediumKind::Cdrom,
        path: iso.to_path_buf(),
        readonly: true,
        boot: false,
    }
}

/// The verdict of the report of the script: `Ok` when both `msiexec`
/// runs ended with 0 or 3010, else the refusal with the first line of the
/// report. `None` (no report) means that the first logon did not run the
/// script.
pub fn report(text: Option<&str>) -> Result<(), String> {
    let Some(text) = text else {
        return Err("the first logon wrote no report of the guest tools".to_string());
    };
    let first = text.lines().next().unwrap_or("").trim();
    let code = |key: &str| {
        first
            .split_whitespace()
            .find_map(|w| w.strip_prefix(key))
            .map(str::to_string)
    };
    let good = |c: Option<String>| matches!(c.as_deref(), Some("0") | Some("3010"));
    if good(code("drivers=")) && good(code("agent=")) {
        Ok(())
    } else {
        Err(format!("the install of the guest tools reported {first:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The script names both packages of the CD, writes its report, and
    /// has CRLF lines; the line of the first logon calls it by its name.
    #[test]
    fn qga_guest_tools_script_installs_the_drivers_and_the_agent() {
        assert!(SCRIPT_TEXT.contains(r"virtio-win-gt-x64.msi /qn"));
        assert!(SCRIPT_TEXT.contains(r"guest-agent\qemu-ga-x86_64.msi /qn"));
        assert!(SCRIPT_TEXT.contains(REPORT));
        assert!(SCRIPT_TEXT.contains("start \"\" /wait msiexec"));
        assert!(SCRIPT_TEXT
            .split("\r\n")
            .all(|l| !l.contains('\n') && !l.contains('\r')));
        assert!(SETUP.contains(SCRIPT) && SETUP.contains(crate::agent::MARKER));
        let m = medium(Path::new("/s/virtio-win-0.1.302.iso"));
        assert_eq!(m.kind, MediumKind::Cdrom);
        assert!(m.readonly && !m.boot);
    }

    /// 0 and 3010 pass; another code, a missing CD or no report fail.
    #[test]
    fn qga_guest_tools_report_is_read() {
        assert!(report(Some("drivers=0 agent=0\r\nSERVICE_NAME: QEMU-GA\r\n")).is_ok());
        assert!(report(Some("drivers=3010 agent=0\r\n")).is_ok());
        let e = report(Some("drivers=0 agent=1603\r\n")).unwrap_err();
        assert!(e.contains("agent=1603"), "{e}");
        assert!(report(Some("drivers=nocd agent=nocd")).is_err());
        assert!(report(None).unwrap_err().contains("no report"));
    }
}
