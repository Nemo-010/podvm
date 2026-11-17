//! `run windows:NAME` (TODO/machine.md T-1861, T-1867): `dos` fetches
//! its pinned base and boots; `validationos` takes its VHDX from the ISO,
//! provisions it one time, and runs the command through the mailbox; a
//! setup ISO runs the command in its golden image (`podvm pull` installs
//! it, T-1867).
//!
//! ⛔ stdout is the output of the command of the guest only: each line of
//! podvm goes to stderr, also the lines of the one provisioning boot.

use std::path::{Path, PathBuf};
use std::time::Duration;

use podvm_image::error::{EXIT_CLI_ERROR, EXIT_RUNTIME_ERROR};
use podvm_windows::catalog::{Entry, Kind};

use super::args::Args;

/// The bound of the provisioning boot of Validation OS: the default of
/// `podvm windows setup`.
const PROVISION_WAIT: Duration = Duration::from_secs(240);

pub(super) fn run_named(
    verb: &str,
    mem: Option<u64>,
    name: &str,
    argv: &[String],
    qemu_extra: &[String],
) -> i32 {
    let e = match podvm_windows::catalog::resolve(name) {
        Ok(e) => e,
        Err(why) => {
            eprintln!("podvm {verb}: {why}");
            return EXIT_CLI_ERROR;
        }
    };
    let image = match e.kind {
        Kind::DosZip => {
            return super::dos::dos_run(verb, mem, None, qemu_extra, None, &argv.join(" "));
        }
        Kind::ValidationOs => vos_image(verb, e),
        Kind::Iso => golden_image(verb, e).and_then(|golden| {
            // TODO/machine.md T-1933: the command goes through the guest
            // agent of a machine; the mailbox stays for a run with extra
            // arguments of the emulator and for an image with no agent.
            let on_agent =
                qemu_extra.is_empty() && std::env::var_os("PODVM_WINDOWS_MAILBOX").is_none();
            match on_agent
                .then(|| {
                    super::machine::once::run_once(verb, &format!("windows:{}", e.name), mem, argv)
                })
                .flatten()
            {
                Some(code) => Err(code),
                None => Ok(golden),
            }
        }),
        Kind::Drivers => Err(no_guest(verb, e)),
    };
    let image = match image {
        Ok(p) => p,
        Err(code) => return code,
    };
    let a = Args {
        sub: "run".to_string(),
        image: Some(image),
        mem,
        emu_args: qemu_extra.to_vec(),
        command: podvm_windows::agent::command_from_argv(argv),
        ..Default::default()
    };
    super::run::run(verb, &a)
}

/// The provisioned image of Validation OS: the VHDX out of the ISO
/// (T-1863), then one provisioning boot where none was made.
pub(super) fn vos_image(verb: &str, e: &Entry) -> Result<PathBuf, i32> {
    let vhdx = super::pull::vos_disk(verb, e)?;
    let out = super::setup::provisioned_path(&vhdx);
    if out.is_file() {
        return Ok(out);
    }
    eprintln!(
        "podvm {verb}: windows:{}: the first run provisions {} (one boot)",
        e.name,
        out.display()
    );
    provision(verb, e, &vhdx, &out)?;
    Ok(out)
}

/// One provisioning boot of `vhdx` into `out`, as `podvm windows setup`,
/// with each line on stderr.
fn provision(verb: &str, e: &Entry, vhdx: &Path, out: &Path) -> Result<(), i32> {
    let a = Args {
        sub: "setup".to_string(),
        image: Some(vhdx.to_path_buf()),
        ..Default::default()
    };
    let plan = super::plan::build(&a)?;
    let _guard = podvm_windows::RunGuard::arm(&plan);
    match podvm_windows::provision(&super::plan::qemu_img(), vhdx, &plan, out, PROVISION_WAIT) {
        Ok(_) => Ok(()),
        Err(why) => {
            let _ = std::fs::remove_file(out);
            eprintln!(
                "podvm {verb}: windows:{}: the provisioning boot: {why}",
                e.name
            );
            Err(EXIT_RUNTIME_ERROR)
        }
    }
}

/// The golden image of a setup ISO, or the refusal that names `podvm pull`.
pub(super) fn golden_image(verb: &str, e: &Entry) -> Result<PathBuf, i32> {
    let golden = super::install::golden_path(e);
    if golden.is_file() {
        return Ok(golden);
    }
    eprintln!(
        "podvm {verb}: windows:{}: {} is not installed; `podvm pull windows:{}` installs it \
         (KVM, about an hour: estimate)",
        e.name, e.what, e.name
    );
    Err(EXIT_RUNTIME_ERROR)
}

/// TODO/machine.md T-1868: the CD of drivers boots no guest. The catalog
/// does not resolve its name, so this is a guard only.
pub(super) fn no_guest(verb: &str, e: &Entry) -> i32 {
    eprintln!(
        "podvm {verb}: windows:{} is a CD of drivers that an install attaches; it boots no guest",
        e.name
    );
    EXIT_CLI_ERROR
}
