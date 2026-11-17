//! TODO/machine.md T-1933: one boot of the installed disk inside `podvm
//! pull windows:NAME`, before the disk becomes the golden image.
//!
//! The first logon of the install puts in the virtio-win guest tools
//! (T-1868), and `msiexec` asks for a restart (3010): the next boot ends the
//! setup of the drivers before the logon. On a host with 2 CPUs that boot
//! took more than the 600 s of a `run`, so the first `run windows:NAME ver`
//! of a fresh golden image failed (runs 44050312708 and 44077136976), and
//! the next boot passed. Here that boot happens once, with a longer bound,
//! and the golden image keeps its result.

use std::time::{Duration, Instant};

use podvm_windows::Plan;

/// The bound of the boot: the end of the setup of the drivers, then the
/// logon and one `ver` (an estimate: 600 s was too short on 2 CPUs).
pub(super) const BOUND: Duration = Duration::from_secs(20 * 60);

/// Boot the installed disk of `plan` once, run `ver` through the mailbox
/// agent, and give the time that it took. The guest powers off at the end.
pub(super) fn settle(plan: &Plan) -> Result<Duration, String> {
    let token = podvm_windows::agent::nonce().map_err(|e| e.to_string())?;
    let request = podvm_windows::Request {
        command: "ver".to_string(),
        token,
        timeout: BOUND,
        mode: Default::default(),
    };
    let mailbox = podvm_windows::mailbox(podvm_windows::agent::AGENT_CMD, &request)
        .map_err(|e| e.to_string())?;
    std::fs::write(&plan.mailbox, mailbox.image()).map_err(|e| e.to_string())?;
    let start = Instant::now();
    let out = podvm_windows::run(plan, &request).map_err(|e| e.to_string())?;
    if out.code != 0 {
        return Err(format!("`ver` in the guest gave {}", out.code));
    }
    Ok(start.elapsed())
}

/// After the power-off of the install: the reports of the first logon on
/// the mailbox (the guest tools, T-1868; the agent, T-1867), then one boot
/// of the installed disk where the agent is there. The reports are read
/// first: the boot writes a new mailbox.
pub(super) fn after_install(
    verb: &str,
    name: &str,
    plan: &Plan,
    quiet: bool,
) -> Result<(), String> {
    let tools = mailbox_file(plan, podvm_windows::guest_tools::REPORT);
    let tools = tools.map(|b| String::from_utf8_lossy(&b).into_owned());
    if let Err(why) = podvm_windows::guest_tools::report(tools.as_deref()) {
        eprintln!(
            "podvm {verb}: windows:{name}: {why}, so `podvm exec` and `podvm cp` cannot \
             reach a machine of this image (TODO/machine.md T-1868)"
        );
    }
    if !agent_installed(plan) {
        eprintln!(
            "podvm {verb}: windows:{name}: the first logon did not install the mailbox agent, \
             so `podvm run windows:{name}` cannot run commands in this image (TODO/machine.md T-1867)"
        );
        return Ok(());
    }
    let took =
        settle(plan).map_err(|why| format!("the first boot of the installed disk: {why}"))?;
    if !quiet {
        eprintln!(
            "podvm {verb}: windows:{name}: first boot of the installed disk in {}",
            podvm_windows::install::elapsed(took)
        );
    }
    Ok(())
}

/// The file `name` of the mailbox of the install, after the power-off.
fn mailbox_file(plan: &Plan, name: &str) -> Option<Vec<u8>> {
    std::fs::read(&plan.mailbox)
        .ok()
        .and_then(podvm_windows::Fat16::from_image)?
        .get(name)
}

/// True when the installer of the agent wrote its report to the mailbox.
fn agent_installed(plan: &Plan) -> bool {
    mailbox_file(plan, podvm_windows::agent::SETUP).is_some()
}
