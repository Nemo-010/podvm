//! TODO/machine.md T-1868: `podvm exec`, `podvm cp` and the graceful
//! `stop` of a full Windows machine (T-1870), through the QEMU Guest Agent
//! on the virtio-serial channel of the machine (`podvm_windows::qga`).
//!
//! ⛔ A DOS or ValidationOS machine has no agent: it runs one command for
//! each boot through its mailbox, and these verbs refuse it by name.

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use podvm_image::error::{EXIT_FLAG_ERROR, EXIT_RUNTIME_ERROR};
use podvm_windows::machines::{self, Record};
use podvm_windows::qga;

/// The refusal of a machine with no agent, or `None` for a desktop one.
fn no_agent(verb: &str, r: &Record) -> Option<i32> {
    if r.flavor == "desktop" {
        return None;
    }
    eprintln!(
        "podvm {verb}: {} is a {} machine, which has no guest agent: it runs one command \
         for each boot through its mailbox (`podvm run {} CMD`; TODO/machine.md T-1868)",
        r.name, r.flavor, r.image
    );
    Some(EXIT_RUNTIME_ERROR)
}

/// A client of the agent of a running machine, after the readiness probe
/// (`guest-ping`) within [`qga::READY_WAIT`].
fn connect(verb: &str, r: &Record) -> Result<qga::Client, i32> {
    if !r.running() {
        eprintln!(
            "podvm {verb}: the machine {} does not run: {}",
            r.name,
            r.status()
        );
        return Err(EXIT_RUNTIME_ERROR);
    }
    let sock = machines::qga_socket(&machines::root(), &r.name);
    qga::ready(&sock, qga::READY_WAIT).map_err(|e| {
        eprintln!(
            "podvm {verb}: {}: the guest agent gave no answer within {} s: {e}; the agent \
             comes with the virtio-win guest tools that `podvm pull` installs into the \
             golden image (TODO/machine.md T-1868)",
            r.name,
            qga::READY_WAIT.as_secs()
        );
        EXIT_RUNTIME_ERROR
    })
}

/// `podvm exec NAME CMD [ARG...]` of a full Windows machine: `cmd.exe /d
/// /c CMD ARG...` in the guest. The output goes to stdout and stderr after
/// the end, and the exit code passes through in 0 to 124.
pub(crate) fn exec(r: &Record, argv: &[String], env: &[String], workdir: Option<&str>) -> i32 {
    if let Some(c) = no_agent("exec", r) {
        return c;
    }
    if !env.is_empty() {
        eprintln!(
            "podvm exec: -e is refused for a Windows machine: the agent replaces the whole \
             environment of the command (TODO/machine.md T-1868)"
        );
        return EXIT_FLAG_ERROR;
    }
    let e = match qga::command(argv, workdir) {
        Ok(e) => e,
        Err(why) => {
            eprintln!("podvm exec: {}: {why}", r.name);
            return EXIT_FLAG_ERROR;
        }
    };
    let mut c = match connect("exec", r) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let s = match qga::run(&mut c, &e, qga::EXEC_LIMIT) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("podvm exec: {}: {err}", r.name);
            return EXIT_RUNTIME_ERROR;
        }
    };
    let mut out = std::io::stdout();
    if let Err(err) = out.write_all(&s.stdout).and_then(|()| out.flush()) {
        eprintln!("podvm exec: stdout: {err}");
        return EXIT_RUNTIME_ERROR;
    }
    // stderr is the last report of podvm: a failed write there has nowhere
    // to be named.
    std::io::stderr().write_all(&s.stderr).unwrap_or(());
    if s.stdout_truncated || s.stderr_truncated {
        eprintln!("podvm exec: the agent kept only the first part of the output");
    }
    qga::exit_of(&s).unwrap_or_else(|why| {
        eprintln!(
            "podvm exec: {}: {why}, so podvm reports {EXIT_RUNTIME_ERROR}",
            r.name
        );
        EXIT_RUNTIME_ERROR
    })
}

/// `podvm cp` where one side is `NAME:PATH` of a machine; `None` where no
/// side names a machine.
pub(crate) fn cp_if(a: &str, b: &str, recursive: bool) -> Option<i32> {
    let side = |arg: &str| {
        let (name, path) = qga::split_machine(arg)?;
        Some((super::find(name)?, path.to_string()))
    };
    let (from, to) = (side(a), side(b));
    let (r, guest, host, out_of) = match (from, to) {
        (None, None) => return None,
        (Some(_), Some(_)) => {
            eprintln!("podvm cp: a copy between two machines is not implemented");
            return Some(EXIT_RUNTIME_ERROR);
        }
        (Some((r, p)), None) => (r, p, b, true),
        (None, Some((r, p))) => (r, p, a, false),
    };
    if let Some(c) = no_agent("cp", &r) {
        return Some(c);
    }
    if recursive || host == "-" {
        eprintln!(
            "podvm cp: the guest agent copies one file; -r and a tar stream (-) are refused \
             for a Windows machine (TODO/machine.md T-1868)"
        );
        return Some(EXIT_FLAG_ERROR);
    }
    Some(copy(&r, &guest, Path::new(host), out_of))
}

/// One file between the host path `host` and the guest path `guest`.
fn copy(r: &Record, guest: &str, host: &Path, out_of: bool) -> i32 {
    if !out_of && !host.is_file() {
        eprintln!(
            "podvm cp: {} is not a file; the guest agent copies one file",
            host.display()
        );
        return EXIT_RUNTIME_ERROR;
    }
    let mut c = match connect("cp", r) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let done = if out_of {
        let dest = if host.is_dir() {
            host.join(qga::base_name(guest))
        } else {
            host.to_path_buf()
        };
        let got = std::fs::File::create(&dest)
            .map_err(|e| format!("{}: {e}", dest.display()))
            .and_then(|mut f| qga::copy_out(&mut c, guest, &mut f));
        if got.is_err() {
            let _ = std::fs::remove_file(&dest);
        }
        got
    } else {
        let target = qga::guest_target(guest, host);
        std::fs::File::open(host)
            .map_err(|e| format!("{}: {e}", host.display()))
            .and_then(|mut f| qga::copy_in(&mut c, &mut f, &target))
    };
    match done {
        Ok(_) => 0,
        Err(why) => {
            eprintln!("podvm cp: {}: {why}", r.name);
            EXIT_RUNTIME_ERROR
        }
    }
}

/// The graceful stop of a full Windows machine: `guest-shutdown`, then a
/// wait of at most [`qga::SHUTDOWN_GRACE`] for the emulator to end. False
/// where no agent answered within [`qga::STOP_PROBE`] or the guest still
/// runs: the caller then goes on with the ACPI power-off, QMP `quit` and
/// SIGKILL.
pub(super) fn graceful(root: &Path, r: &Record) -> bool {
    let sock = machines::qga_socket(root, &r.name);
    if qga::shutdown(&sock, qga::STOP_PROBE).is_err() {
        return false;
    }
    let t0 = Instant::now();
    while t0.elapsed() < qga::SHUTDOWN_GRACE && r.running() {
        std::thread::sleep(Duration::from_millis(250));
    }
    !r.running()
}
