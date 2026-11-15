//! TODO/machine.md T-1867: the install of a setup ISO into a disk, the
//! parts with no emulator: the admission of the accelerator, the phase of
//! the guest from what the host sees, the lines that report it, and the
//! sizes and bounds of the install.
//!
//! The host watches the guest from outside. Two things tell the phase: the
//! bytes that the guest wrote into the blank root disk (Setup copies the
//! image of the ISO in its first phase), and the markers of the answer
//! file on COM1 ([`answer::SPECIALIZE`], [`answer::LOGON`],
//! [`answer::READY`]).

use std::time::Duration;

use crate::answer;
use crate::Accel;

/// The size of the blank root disk: sparse, so the host gives only the
/// bytes that the guest writes.
pub const DISK_BYTES: u64 = 64 << 30;

/// The free disk that the install needs before its first write: the
/// written root disk and its converted copy (estimate, until the first
/// install under KVM gives the size).
pub const ROOM_BYTES: u64 = 48 << 30;

/// The firmware of the stock ISO asks "Press any key to boot from CD or
/// DVD": the driver sends Enter each [`KEY_EVERY`] for [`KEYS_FOR`].
pub const KEYS_FOR: Duration = Duration::from_secs(20);
/// See [`KEYS_FOR`].
pub const KEY_EVERY: Duration = Duration::from_millis(500);

/// The root disk holds this many bytes when Setup copies files: WinPE
/// writes nothing to the blank disk before it.
pub const COPYING_AT: u64 = 256 << 20;

/// The limit of a whole install: 2 h under KVM, 16 h under TCG
/// (estimates: the review gives 15 to 40 min under KVM and hours under
/// TCG).
pub fn deadline(accel: Accel) -> Duration {
    match accel {
        Accel::Kvm => Duration::from_secs(2 * 3600),
        _ => Duration::from_secs(16 * 3600),
    }
}

const _: () = assert!(ROOM_BYTES < DISK_BYTES && COPYING_AT < ROOM_BYTES);
const _: () = assert!(KEY_EVERY.as_millis() < KEYS_FOR.as_millis());

/// The time between two progress lines in the same phase.
pub const PROGRESS_EVERY: Duration = Duration::from_secs(300);

/// The time that the guest gets to power off after [`answer::READY`]: the
/// answer file shuts down 5 s after the marker.
pub const POWER_OFF: Duration = Duration::from_secs(120);

/// One phase of an install, in the order of a good install.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    /// The firmware and WinPE boot from the ISO.
    Booting,
    /// Setup writes the image to the root disk.
    CopyingFiles,
    /// Setup rebooted into the specialize pass.
    Installing,
    /// The first logon of the new user runs its commands.
    FirstLogon,
    /// The first logon wrote [`answer::READY`].
    Ready,
}

impl Phase {
    /// The words of the phase in a progress line.
    pub fn words(self) -> &'static str {
        match self {
            Phase::Booting => "booting the setup ISO",
            Phase::CopyingFiles => "copying files",
            Phase::Installing => "installing",
            Phase::FirstLogon => "first logon",
            Phase::Ready => "ready",
        }
    }
}

/// True when `hay` holds `needle`.
fn holds(hay: &[u8], needle: &str) -> bool {
    let n = needle.as_bytes();
    !n.is_empty() && hay.windows(n.len()).any(|w| w == n)
}

/// The phase of a guest whose serial log holds `serial` and whose root
/// disk holds `root_bytes` written bytes. A later marker wins over an
/// earlier one.
pub fn phase_of(serial: &[u8], root_bytes: u64) -> Phase {
    if holds(serial, answer::READY) {
        Phase::Ready
    } else if holds(serial, answer::LOGON) {
        Phase::FirstLogon
    } else if holds(serial, answer::SPECIALIZE) {
        Phase::Installing
    } else if root_bytes >= COPYING_AT {
        Phase::CopyingFiles
    } else {
        Phase::Booting
    }
}

/// `1h02m05s`, `3m07s` or `12s`.
pub fn elapsed(d: Duration) -> String {
    let s = d.as_secs();
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}h{m:02}m{s:02}s")
    } else if m > 0 {
        format!("{m}m{s:02}s")
    } else {
        format!("{s}s")
    }
}

/// The progress line of `name` in `phase` after `spent`, with the bytes
/// that the guest wrote to the root disk (TODO/machine.md T-1867: a stall
/// shows as the same count on two lines).
pub fn phase_line(verb: &str, name: &str, phase: Phase, spent: Duration, written: u64) -> String {
    let words = phase.words();
    let t = elapsed(spent);
    if written == 0 {
        return format!("podvm {verb}: windows:{name}: {words} ({t})");
    }
    let gib = written as f64 / f64::from(1u32 << 30);
    format!("podvm {verb}: windows:{name}: {words} ({t}, {gib:.2} GiB on the disk)")
}

/// The variable that takes TCG for an install.
pub const ACCEL_VAR: &str = "PODVM_MACHINE_ACCEL";

/// An install under `accel`: KVM always; TCG only when `asked_tcg` (the
/// user set [`ACCEL_VAR`] to `tcg`), as it takes hours.
pub fn admit(name: &str, accel: Accel, asked_tcg: bool) -> Result<(), String> {
    match accel {
        Accel::Kvm => Ok(()),
        _ if asked_tcg => Ok(()),
        _ => Err(format!(
            "windows:{name}: the install of a setup ISO runs under KVM, and this host gives \
             TCG only. Under TCG the install takes hours (estimate; the bound is {}). \
             Set {ACCEL_VAR}=tcg (the --accel tcg of TODO/machine.md T-1867) to install \
             under TCG",
            elapsed(deadline(Accel::Tcg))
        )),
    }
}

/// TODO/machine.md T-1871: the days of the evaluation of a target, from
/// its first boot: 90 for the Enterprise evaluation of Windows 10 and 11,
/// 180 for Windows Server 2025 (the terms that Microsoft publishes for
/// each evaluation ISO; podvm does not extend them).
pub fn eval_days(target: answer::Target) -> u64 {
    match target {
        answer::Target::Server2025 => 180,
        answer::Target::Win11 | answer::Target::Win10 => 90,
    }
}

/// The line of the doctor for a golden image installed at `installed`
/// (seconds since the epoch) with `days` of evaluation, at `now`.
pub fn eval_line(name: &str, installed: u64, days: u64, now: u64) -> String {
    let end = installed + days * 86_400;
    if now >= end {
        format!(
            "windows:{name}: the evaluation of {days} days ended {} days ago; \
             `podvm rmi windows:{name}` and `podvm pull windows:{name}` install it again",
            (now - end) / 86_400
        )
    } else {
        format!(
            "windows:{name}: {} of {days} days of the evaluation are left",
            (end - now).div_ceil(86_400)
        )
    }
}

/// The text of the meta file of a golden image: `NAME INSTALLED DAYS`.
pub fn meta_text(name: &str, installed: u64, days: u64) -> String {
    format!("{name} {installed} {days}\n")
}

/// The meta file of a golden image, read back: `(installed, days)`.
pub fn meta_of(text: &str) -> Option<(u64, u64)> {
    let mut it = text.split_whitespace();
    let _name = it.next()?;
    Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each phase follows from the disk and the markers, and a later
    /// marker wins over an earlier one.
    #[test]
    fn install_plan_phase_follows_the_disk_and_the_markers() {
        assert_eq!(phase_of(b"", 0), Phase::Booting);
        assert_eq!(phase_of(b"BdsDxe: loading", COPYING_AT - 1), Phase::Booting);
        assert_eq!(phase_of(b"", COPYING_AT), Phase::CopyingFiles);
        let spec = format!("noise\r\n{}\r\n", answer::SPECIALIZE);
        assert_eq!(phase_of(spec.as_bytes(), 9 << 30), Phase::Installing);
        let logon = format!("{spec}{}\r\n", answer::LOGON);
        assert_eq!(phase_of(logon.as_bytes(), 9 << 30), Phase::FirstLogon);
        let ready = format!("{logon}{}\r\n", answer::READY);
        assert_eq!(phase_of(ready.as_bytes(), 0), Phase::Ready);
        assert!(Phase::Booting < Phase::CopyingFiles && Phase::FirstLogon < Phase::Ready);
    }

    /// The progress lines name the image, the phase and the elapsed time.
    #[test]
    fn install_plan_phase_lines_name_the_phase_and_the_time() {
        let l = phase_line(
            "pull",
            "2025",
            Phase::CopyingFiles,
            Duration::from_secs(192),
            3 << 29,
        );
        assert_eq!(
            l,
            "podvm pull: windows:2025: copying files (3m12s, 1.50 GiB on the disk)"
        );
        let l = phase_line(
            "pull",
            "11",
            Phase::FirstLogon,
            Duration::from_secs(3725),
            0,
        );
        assert_eq!(l, "podvm pull: windows:11: first logon (1h02m05s)");
        assert_eq!(elapsed(Duration::from_secs(7)), "7s");
        for p in [
            Phase::Booting,
            Phase::CopyingFiles,
            Phase::Installing,
            Phase::FirstLogon,
            Phase::Ready,
        ] {
            assert!(!p.words().is_empty());
        }
    }

    /// TCG is refused unless asked, and the refusal names the switch and
    /// `--accel tcg`; KVM is always admitted.
    #[test]
    fn install_plan_refuses_tcg_unless_asked() {
        assert_eq!(admit("2025", Accel::Kvm, false), Ok(()));
        assert_eq!(admit("2025", Accel::Tcg, true), Ok(()));
        let e = admit("2025", Accel::Tcg, false).unwrap_err();
        assert!(e.contains("PODVM_MACHINE_ACCEL=tcg"), "{e}");
        assert!(e.contains("--accel tcg"), "{e}");
        assert!(e.contains("windows:2025") && e.contains("hours"), "{e}");
    }

    /// The bounds are finite, and TCG gets the longer one.
    #[test]
    fn install_plan_bounds_are_finite() {
        assert!(deadline(Accel::Kvm) < deadline(Accel::Tcg));
    }

    /// The answer file writes each marker that the phase reads.
    #[test]
    fn install_plan_markers_are_in_the_answer_file() {
        for t in [answer::Target::Win11, answer::Target::Server2025] {
            let x = answer::xml(&answer::Answer {
                target: t,
                locale: "en-US".into(),
                user: "podvm".into(),
                password: "Pv-test-1!".into(),
                computer: "podvm-test".into(),
            });
            for m in [answer::SPECIALIZE, answer::LOGON, answer::READY] {
                assert!(x.contains(m), "{m} is not in the answer file of {t:?}");
            }
            assert!(
                !x.contains("<RunSynchronous>\n      </RunSynchronous>"),
                "{x}"
            );
        }
    }

    /// The evaluation: the days of each target, the days left, and the
    /// line after the end.
    #[test]
    fn windows_doctor_eval_period_counts_down() {
        assert_eq!(eval_days(answer::Target::Server2025), 180);
        assert_eq!(eval_days(answer::Target::Win11), 90);
        let day = 86_400;
        let l = eval_line("2025", 1_000 * day, 180, 1_010 * day);
        assert!(l.contains("170 of 180 days"), "{l}");
        let l = eval_line("11", 1_000 * day, 90, 1_100 * day);
        assert!(
            l.contains("ended 10 days ago") && l.contains("podvm pull windows:11"),
            "{l}"
        );
        assert_eq!(meta_of(&meta_text("2025", 42, 180)), Some((42, 180)));
        assert_eq!(meta_of("2025"), None);
    }
}
