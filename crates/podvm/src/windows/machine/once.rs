//! TODO/machine.md T-1933: a foreground `podvm run windows:NAME CMD` of a
//! golden image runs its command through the QEMU Guest Agent (T-1868),
//! as `podvm exec` of a detached machine does, not through the mailbox.
//!
//! On a host with 2 CPUs the run through the mailbox timed out at 600 s in
//! 3 of 4 runs of `kvm.yml`, and a detached machine of the same image
//! answered `exec` each time (runs 44050312708, 44077136976, 44088895746).
//! So the run makes a machine with a free name, waits for the agent, runs
//! the command, and removes the machine. Where the agent gives no answer
//! (an image whose guest tools did not install), the caller runs the
//! command through the mailbox, as before.
//!
//! ⛔ stdout is the output of the command only: the name of the machine and
//! each report of podvm go to stderr.

use std::time::Duration;

use podvm_windows::{machines, qga};

/// The lifetime of the machine of a foreground run: the probe of the agent,
/// the longest command and the stop. The keeper stops the emulator at its
/// end also when podvm itself died (a signal), so no emulator stays.
const LIFETIME: Duration =
    Duration::from_secs(qga::READY_WAIT.as_secs() + qga::EXEC_LIMIT.as_secs() + 300);

/// Run `argv` (`ver` when empty) in a new machine of `image`
/// (`windows:NAME`, a setup ISO of the catalog with its golden image), and
/// give the exit code of the command. `None` when the agent of the machine
/// gave no answer within [`qga::READY_WAIT`]: the machine is gone, and the
/// caller runs the command through the mailbox.
pub(crate) fn run_once(verb: &str, image: &str, mem: Option<u64>, argv: &[String]) -> Option<i32> {
    let name = match super::create(verb, None, image, mem, Some(LIFETIME.as_secs()), &[]) {
        Ok(n) => n,
        Err(code) => return Some(code),
    };
    let sock = machines::qga_socket(&machines::root(), &name);
    let code = match (super::find(&name), qga::ready(&sock, qga::READY_WAIT)) {
        (Some(r), Ok(_)) => Some(super::exec(&r, &command(argv), &[], None)),
        (_, Err(e)) => {
            eprintln!(
                "podvm {verb}: {image}: the guest agent gave no answer within {} s ({e}); \
                 podvm runs the command through the mailbox agent (TODO/machine.md T-1933)",
                qga::READY_WAIT.as_secs()
            );
            None
        }
        (None, Ok(_)) => {
            eprintln!("podvm {verb}: {image}: the record of the machine {name} is gone");
            Some(podvm_image::error::EXIT_RUNTIME_ERROR)
        }
    };
    if super::life::remove(verb, &name, true) != 0 {
        eprintln!("podvm {verb}: the machine {name} stays; `podvm rm -f {name}` removes it");
    }
    code
}

/// The command of the run: `ver` where the caller gave none, as the run
/// through the mailbox does.
fn command(argv: &[String]) -> Vec<String> {
    if argv.iter().all(|a| a.trim().is_empty()) {
        return vec!["ver".to_string()];
    }
    argv.to_vec()
}

#[cfg(test)]
#[test]
fn an_empty_command_runs_ver() {
    assert_eq!(command(&[]), ["ver"]);
    assert_eq!(command(&[" ".to_string()]), ["ver"]);
    let a = ["echo".to_string(), "hi".to_string()];
    assert_eq!(command(&a), a);
}

#[cfg(test)]
#[test]
fn the_lifetime_covers_the_probe_and_the_longest_command() {
    assert!(LIFETIME > qga::READY_WAIT + qga::EXEC_LIMIT);
}
