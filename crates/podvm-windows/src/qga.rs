//! TODO/machine.md T-1868: the QEMU Guest Agent of a running full Windows
//! guest (a machine of T-1870), on `pod-qga` of podcrates `v0.63.0`
//! (COORD-127, issue 393).
//!
//! A desktop machine has the virtio-serial channel [`QGA_CHANNEL`] on a
//! Unix socket in the runtime directory ([`channel`],
//! `machines::qga_socket`). The virtio-win guest tools of the install
//! ([`crate::guest_tools`]) put the agent into the golden image. This
//! module gives podvm's choices on the client: the readiness probe
//! ([`ready`]: connect, sync and `guest-ping` until a bound), the command
//! line of `podvm exec` ([`command`]: `cmd.exe /d /c ARGV`), the exit code
//! in docker's range ([`exit_of`]), the copy of one file in each direction
//! ([`copy_out`], [`copy_in`]) in chunks, and the bounds.
//!
//! ⛔ ValidationOS has no Windows Installer service and cannot run the
//! agent. It keeps the mailbox.

use std::io::{Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

pub use pod_qga::{Client, Error, ErrorKind, Exec, ExecStatus, OpenMode, Settings, Shutdown};

pub use crate::plan::options::QGA_CHANNEL;
use crate::plan::Channel;

/// The bound of the readiness probe of `exec` and `cp`: a cold boot of
/// Windows under KVM and the start of the agent service (estimate).
pub const READY_WAIT: Duration = Duration::from_secs(300);
/// The bound of the probe of a graceful `stop`: an agent that runs
/// answers at once; a guest that still boots gets the ACPI power-off.
pub const STOP_PROBE: Duration = Duration::from_secs(5);
/// The time that a graceful `stop` gives the guest to power off after
/// `guest-shutdown` (estimate: the shutdown of Windows with its updates
/// off).
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(120);
/// The bound of one call of the probe. A guest whose agent does not run
/// yet gives no answer to the sync.
pub const PROBE_CALL: Duration = Duration::from_secs(3);
/// The bound of one call after the probe: a chunk of 1 MiB in base64.
pub const CALL: Duration = Duration::from_secs(30);
/// The poll of `guest-exec-status`.
pub const EXEC_POLL: Duration = Duration::from_millis(200);
/// The longest `podvm exec` of a Windows machine (estimate: a long
/// build step). The command still runs in the guest after it.
pub const EXEC_LIMIT: Duration = Duration::from_secs(3600);
/// The bytes of one read or write of a copy.
pub const CHUNK: usize = 1 << 20;

/// The virtio-serial channel of the agent on the Unix socket `socket`.
pub fn channel(socket: &Path) -> Channel {
    Channel {
        name: QGA_CHANNEL.to_string(),
        socket: socket.to_path_buf(),
    }
}

/// podvm's settings of a client of the agent at `socket`.
pub fn settings(socket: &Path, call: Duration) -> Settings {
    let mut s = Settings::new(socket);
    s.call_timeout = call;
    s.chunk = CHUNK;
    s
}

/// The readiness probe: connect, sync and `guest-ping`, again each second
/// until `wait`. Gives a client with the bound [`CALL`], or the last
/// error.
pub fn ready(socket: &Path, wait: Duration) -> pod_qga::Result<Client> {
    ready_with(socket, wait, PROBE_CALL, CALL)
}

/// [`ready`] with the bound of a call of the probe and of the client.
pub fn ready_with(
    socket: &Path,
    wait: Duration,
    probe: Duration,
    call: Duration,
) -> pod_qga::Result<Client> {
    let t0 = Instant::now();
    loop {
        let left = wait.saturating_sub(t0.elapsed());
        let s = settings(socket, probe.min(left).max(Duration::from_millis(100)));
        match Client::connect(s).and_then(|mut c| c.ping()) {
            // The probe passed: a new connection with the bound of a call.
            Ok(()) => return Client::connect(settings(socket, call)),
            Err(e) if t0.elapsed() + Duration::from_secs(1) >= wait => return Err(e),
            Err(_) => std::thread::sleep(Duration::from_secs(1)),
        }
    }
}

/// A value that `cmd.exe` cannot take through the agent: the agent quotes
/// each argument with `\"`, which `cmd` does not read, and a line break
/// ends the command line.
fn bad_text(s: &str) -> bool {
    s.contains(['"', '\r', '\n', '\0'])
}

/// The request of `podvm exec NAME -- ARGV`: `cmd.exe /d /c ARGV`, so that
/// a built-in command of `cmd` (`ver`, `dir`) runs, and the exit code of
/// the command is the exit code of `cmd`. `workdir` adds `cd /d DIR &&`
/// first. The output is kept.
pub fn command(argv: &[String], workdir: Option<&str>) -> Result<Exec, String> {
    if argv.is_empty() {
        return Err("name a command to run".to_string());
    }
    if let Some(bad) = argv
        .iter()
        .map(String::as_str)
        .chain(workdir)
        .find(|a| bad_text(a))
    {
        return Err(format!(
            "{bad:?} holds a double quote or a line break, which cmd.exe cannot take \
             through the guest agent"
        ));
    }
    let mut e = Exec::new("cmd.exe");
    e.args = vec!["/d".into(), "/c".into()];
    if let Some(dir) = workdir.filter(|d| !d.is_empty()) {
        e.args
            .extend(["cd".into(), "/d".into(), dir.to_string(), "&&".into()]);
    }
    e.args.extend(argv.iter().cloned());
    Ok(e)
}

/// Run `e` and wait for its end, for at most `limit`.
pub fn run(c: &mut Client, e: &Exec, limit: Duration) -> pod_qga::Result<ExecStatus> {
    let pid = c.exec(e)?;
    c.wait_exec(pid, EXEC_POLL, limit)
}

/// docker's exit code of an ended command: its own code in 0 to 124, else
/// the refusal (125 and above are podvm's own codes).
pub fn exit_of(s: &ExecStatus) -> Result<i32, String> {
    match (s.exit_code, s.signal) {
        (Some(c), _) => crate::cap_status(c).ok_or_else(|| {
            format!("the command exited {c}, which is outside docker's 0-124 range")
        }),
        (None, Some(sig)) => Err(format!("the command ended by the signal {sig}")),
        (None, None) => Err("the agent gave no exit code".to_string()),
    }
}

/// `NAME:PATH` of `podvm cp`, where NAME is a valid name of a machine and
/// PATH is not empty. A Windows path holds a `:` too (`win:C:\x`): the
/// first `:` ends the name.
pub fn split_machine(arg: &str) -> Option<(&str, &str)> {
    let (name, path) = arg.split_once(':')?;
    (crate::machines::valid_name(name) && !path.is_empty()).then_some((name, path))
}

/// The last part of a Windows or a POSIX path.
pub fn base_name(path: &str) -> &str {
    path.trim_end_matches(['\\', '/'])
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
}

/// The path in the guest of a copy of the host file `src` to `dest`: a
/// `dest` that ends with `\` or `/` is a directory and gets the name of
/// `src`. The agent cannot tell a directory, so the caller marks it.
pub fn guest_target(dest: &str, src: &Path) -> String {
    match src.file_name() {
        Some(n) if dest.ends_with(['\\', '/']) => format!("{dest}{}", n.to_string_lossy()),
        _ => dest.to_string(),
    }
}

/// Copy the guest file `guest` to `out`. The handle is closed also when a
/// read or a write fails. Gives the count of the bytes.
pub fn copy_out(c: &mut Client, guest: &str, out: &mut dyn Write) -> Result<u64, String> {
    let h = c
        .file_open(guest, OpenMode::Read)
        .map_err(|e| e.to_string())?;
    let copied = read_to(c, h, out);
    let closed = c.file_close(h);
    let n = copied?;
    closed.map_err(|e| e.to_string())?;
    Ok(n)
}

fn read_to(c: &mut Client, h: i64, out: &mut dyn Write) -> Result<u64, String> {
    let mut n = 0u64;
    loop {
        let chunk = c.file_read(h, CHUNK).map_err(|e| e.to_string())?;
        out.write_all(&chunk.data)
            .map_err(|e| format!("the write on the host: {e}"))?;
        n += chunk.data.len() as u64;
        // The agent of Windows sets the end flag only on a read of no
        // byte, so an empty read is an end too.
        if chunk.eof || chunk.data.is_empty() {
            out.flush()
                .map_err(|e| format!("the write on the host: {e}"))?;
            return Ok(n);
        }
    }
}

/// Copy `input` to the guest file `guest`: made, or cut to zero first.
/// The handle is closed also when a read or a write fails. Gives the count
/// of the bytes.
pub fn copy_in(c: &mut Client, input: &mut dyn Read, guest: &str) -> Result<u64, String> {
    let h = c
        .file_open(guest, OpenMode::Write)
        .map_err(|e| e.to_string())?;
    let copied = write_from(c, h, input);
    let closed = c.file_close(h);
    let n = copied?;
    closed.map_err(|e| e.to_string())?;
    Ok(n)
}

fn write_from(c: &mut Client, h: i64, input: &mut dyn Read) -> Result<u64, String> {
    let mut buf = vec![0u8; CHUNK];
    let mut n = 0u64;
    loop {
        let got = match input.read(&mut buf) {
            Ok(0) => return Ok(n),
            Ok(k) => k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("the read on the host: {e}")),
        };
        let mut at = 0;
        while at < got {
            let w = c.file_write(h, &buf[at..got]).map_err(|e| e.to_string())?;
            if w == 0 {
                return Err("the agent wrote no byte".to_string());
            }
            at += w as usize;
        }
        n += got as u64;
    }
}

/// The graceful stop: the probe within `probe`, then `guest-shutdown`
/// (power-down). `Ok` means that the agent took the request; the caller
/// waits for the emulator to end.
pub fn shutdown(socket: &Path, probe: Duration) -> pod_qga::Result<()> {
    let mut c = ready_with(socket, probe, probe.min(PROBE_CALL), CALL)?;
    c.shutdown(Shutdown::Powerdown)
}

#[cfg(test)]
mod tests;
