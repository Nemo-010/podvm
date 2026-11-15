//! TODO/machine.md T-1750: `exec` into a running guest of the machine
//! tier. The guest of `run -d --podvm-tier=machine` has one more
//! `virtserialport`, `podvm.exec`, on a chardev socket of the host
//! ([`argv`]). In the guest, `podvm guest-channel exec` serves one command
//! at a time on the port; on the host, `podvm exec NAME CMD` connects to
//! the socket ([`client`]).
//!
//! The request is one line of JSON: `{"argv": [...], "env": [...],
//! "workdir": "..."}`. The answer is frames: a tag byte, a length (u32,
//! big-endian) and the bytes: `1` stdout, `2` stderr, `X` the exit code
//! (i32, big-endian). The command's stdin is `/dev/null`.
//!
//! ⚠ One stream, one command at a time: QEMU's chardev server takes one
//! client, so a second `exec` waits for the first to end.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// The name of the port in the guest.
pub const PORT_NAME: &str = "podvm.exec";

/// The emulator's arguments for the port, on the socket `sock`.
pub fn argv(sock: &Path) -> Vec<String> {
    vec![
        "-device".into(),
        "virtio-serial-pci,id=podvmexb".into(),
        "-chardev".into(),
        format!(
            "socket,id=podvmexec,path={},server=on,wait=off",
            sock.display()
        ),
        "-device".into(),
        format!("virtserialport,chardev=podvmexec,name={PORT_NAME}"),
    ]
}

/// The host files of a detached guest (`run -d`): the exec port, the QMP
/// monitor that `stop` uses, and the file where the boot writes the pid
/// and the start time of the emulator.
#[derive(Debug, Clone)]
pub struct Keeper {
    pub exec: PathBuf,
    pub qmp: PathBuf,
    pub pidfile: PathBuf,
}

/// The emulator's arguments of a detached guest: the exec port and QMP.
pub fn keeper_args(k: &Keeper) -> Vec<String> {
    let mut v = argv(&k.exec);
    v.push("-qmp".into());
    v.push(format!("unix:{},server=on,wait=off", k.qmp.display()));
    v
}

/// The init of a detached guest (`on`) starts the exec server before the
/// payload; each other init stays as it is.
pub fn with_server_if(script: String, on: bool) -> String {
    if !on {
        return script;
    }
    script.replacen(
        "mkfifo /podvm-out",
        "/podvm/podvm guest-channel exec >/dev/null 2>&1 &\nmkfifo /podvm-out",
        1,
    )
}

/// One frame.
pub fn frame(tag: u8, bytes: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(5 + bytes.len());
    f.push(tag);
    f.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    f.extend_from_slice(bytes);
    f
}

/// Read one frame: the tag and the bytes; `None` at the end of the stream.
pub fn read_frame(r: &mut impl Read) -> Option<(u8, Vec<u8>)> {
    let mut h = [0u8; 5];
    r.read_exact(&mut h).ok()?;
    let n = u32::from_be_bytes([h[1], h[2], h[3], h[4]]) as usize;
    // A chunk of the guest is 64 KiB; more is no frame of podvm.
    if n > 1 << 20 {
        return None;
    }
    let mut b = vec![0u8; n];
    r.read_exact(&mut b).ok()?;
    Some((h[0], b))
}

/// The request of one command.
pub fn request(argv: &[String], env: &[String], workdir: &str) -> String {
    let mut s = serde_json::json!({ "argv": argv, "env": env, "workdir": workdir }).to_string();
    s.push('\n');
    s
}

/// The host side: send the command to the guest of the socket `sock`, copy
/// its output, and give its exit code (125 where the guest cannot be
/// reached or the stream ends with no code).
pub fn client(verb: &str, sock: &Path, argv: &[String], env: &[String], workdir: &str) -> i32 {
    let mut s = match std::os::unix::net::UnixStream::connect(sock) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "podvm {verb}: the exec port of the guest ({}): {e} (TODO/machine.md T-1750)",
                sock.display()
            );
            return podvm_image::error::EXIT_RUNTIME_ERROR;
        }
    };
    if s.write_all(request(argv, env, workdir).as_bytes()).is_err() {
        eprintln!("podvm {verb}: the guest took no command (TODO/machine.md T-1750)");
        return podvm_image::error::EXIT_RUNTIME_ERROR;
    }
    let (mut out, mut err) = (std::io::stdout(), std::io::stderr());
    while let Some((tag, b)) = read_frame(&mut s) {
        match tag {
            b'1' => {
                if let Err(e) = out.write_all(&b).and_then(|()| out.flush()) {
                    eprintln!("podvm {verb}: stdout: {e}");
                    return podvm_image::error::EXIT_RUNTIME_ERROR;
                }
            }
            // stderr is the last report of podvm: a failed write there has
            // nowhere to be named.
            b'2' => err.write_all(&b).unwrap_or(()),
            b'X' if b.len() == 4 => return i32::from_be_bytes([b[0], b[1], b[2], b[3]]),
            _ => break,
        }
    }
    eprintln!(
        "podvm {verb}: the guest ended the stream with no exit code (TODO/machine.md T-1750)"
    );
    podvm_image::error::EXIT_RUNTIME_ERROR
}

/// `podvm exec NAME CMD` of a machine: a running Linux guest of `run -d
/// --podvm-tier=machine` takes the command on its exec port; a Windows or
/// DOS machine has no exec yet (T-1868).
pub fn exec_machine(
    r: &podvm_windows::machines::Record,
    argv: &[String],
    env: &[String],
    workdir: Option<&str>,
) -> i32 {
    if r.flavor != "linux" {
        eprintln!(
            "podvm exec: {} is a {} machine, which has no exec yet (TODO/machine.md T-1868)",
            r.name, r.flavor
        );
        return podvm_image::error::EXIT_RUNTIME_ERROR;
    }
    if !r.running() {
        eprintln!(
            "podvm exec: the machine {} does not run: {}",
            r.name,
            r.status()
        );
        return podvm_image::error::EXIT_RUNTIME_ERROR;
    }
    if argv.is_empty() {
        eprintln!("podvm exec: name a command to run in {}", r.name);
        return podvm_image::error::EXIT_FLAG_ERROR;
    }
    let sock = podvm_windows::machines::exec_socket(&podvm_windows::machines::root(), &r.name);
    client("exec", &sock, argv, env, workdir.unwrap_or(""))
}

/// The device of the port named [`PORT_NAME`] in the guest.
fn port() -> Option<PathBuf> {
    std::fs::read_dir("/sys/class/virtio-ports")
        .ok()?
        .flatten()
        .find(|e| {
            std::fs::read_to_string(e.path().join("name")).is_ok_and(|n| n.trim() == PORT_NAME)
        })
        .map(|e| Path::new("/dev").join(e.file_name()))
}

/// Run one request and send its frames to `w`.
fn serve_one(line: &[u8], w: &std::sync::Arc<std::sync::Mutex<std::fs::File>>) {
    // A write that fails means that the host left: the next command waits
    // for a host again.
    let send = |tag: u8, b: &[u8]| {
        if let Ok(mut f) = w.lock() {
            f.write_all(&frame(tag, b)).unwrap_or(());
        }
    };
    let req: serde_json::Value = serde_json::from_slice(line).unwrap_or_default();
    let strings = |k: &str| -> Vec<String> {
        req[k]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };
    let argv = strings("argv");
    let Some(prog) = argv.first() else {
        send(b'2', b"podvm guest-exec: no command\n");
        send(b'X', &125i32.to_be_bytes());
        return;
    };
    let mut cmd = std::process::Command::new(prog);
    cmd.args(&argv[1..]).stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for kv in strings("env") {
        if let Some((k, v)) = kv.split_once('=') {
            cmd.env(k, v);
        }
    }
    if let Some(d) = req["workdir"].as_str().filter(|d| !d.is_empty()) {
        cmd.current_dir(d);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            send(b'2', format!("podvm guest-exec: {prog}: {e}\n").as_bytes());
            let code: i32 = if e.kind() == std::io::ErrorKind::NotFound {
                127
            } else {
                126
            };
            send(b'X', &code.to_be_bytes());
            return;
        }
    };
    let pump = |mut r: Box<dyn Read + Send>, tag: u8| {
        let w = w.clone();
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 64 << 10];
            while let Ok(n) = r.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let sent = w.lock().map(|mut f| f.write_all(&frame(tag, &buf[..n])));
                if !matches!(sent, Ok(Ok(()))) {
                    break;
                }
            }
        })
    };
    let a = pump(Box::new(child.stdout.take().expect("piped")), b'1');
    let b = pump(Box::new(child.stderr.take().expect("piped")), b'2');
    let status = child.wait();
    let _ = (a.join(), b.join());
    let code = match status {
        Ok(s) => s.code().unwrap_or_else(|| {
            use std::os::unix::process::ExitStatusExt;
            128 + s.signal().unwrap_or(0)
        }),
        Err(_) => 125,
    };
    send(b'X', &code.to_be_bytes());
}

/// `podvm guest-channel exec`, in the guest: serve the commands of the port.
pub fn guest_main() -> i32 {
    let Some(dev) = port() else {
        eprintln!("podvm guest-exec: no port {PORT_NAME}");
        return 1;
    };
    // A virtio port opens one time (a second open is EBUSY): one open, and
    // a duplicate of the descriptor for the writes.
    let opened = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&dev)
        .and_then(|f| f.try_clone().map(|w| (f, w)));
    let Ok((mut r, w)) = opened else {
        eprintln!("podvm guest-exec: {} does not open", dev.display());
        return 1;
    };
    let w = std::sync::Arc::new(std::sync::Mutex::new(w));
    let mut line = Vec::new();
    let mut b = [0u8; 1];
    loop {
        match r.read(&mut b) {
            // No host on the socket: the port reads nothing; wait for one.
            Ok(0) | Err(_) => std::thread::sleep(std::time::Duration::from_millis(200)),
            Ok(_) if b[0] == b'\n' => {
                serve_one(&line, &w);
                line.clear();
            }
            Ok(_) => line.push(b[0]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frames round-trip, and the request names argv, env and workdir.
    #[test]
    fn machine_exec_frames_round_trip() {
        let mut s = frame(b'1', b"out");
        s.extend(frame(b'X', &7i32.to_be_bytes()));
        let mut r = &s[..];
        assert_eq!(read_frame(&mut r), Some((b'1', b"out".to_vec())));
        assert_eq!(
            read_frame(&mut r),
            Some((b'X', 7i32.to_be_bytes().to_vec()))
        );
        assert_eq!(read_frame(&mut r), None);
        let q = request(
            &["sh".into(), "-c".into(), "exit 3".into()],
            &["A=1".into()],
            "/w",
        );
        let v: serde_json::Value = serde_json::from_str(q.trim()).unwrap();
        assert_eq!(v["argv"][2], "exit 3");
        assert_eq!(v["env"][0], "A=1");
        assert_eq!(v["workdir"], "/w");
        assert!(q.ends_with('\n') && !q.trim().contains('\n'));
    }

    /// One request of the guest side gives the output and the exit code of
    /// its command, on a pipe in place of the port.
    #[test]
    fn machine_exec_serves_one_command() {
        let path = std::env::temp_dir().join(format!("podvm-exec-{}", std::process::id()));
        let f = std::fs::File::create(&path).unwrap();
        let w = std::sync::Arc::new(std::sync::Mutex::new(f));
        let q = request(
            &[
                "sh".into(),
                "-c".into(),
                "echo hi; echo e >&2; exit 4".into(),
            ],
            &[],
            "/",
        );
        serve_one(q.trim().as_bytes(), &w);
        drop(w);
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let mut r = &bytes[..];
        let mut out = Vec::new();
        let mut code = None;
        while let Some((t, b)) = read_frame(&mut r) {
            match t {
                b'1' => out.extend(b),
                b'X' => code = Some(i32::from_be_bytes([b[0], b[1], b[2], b[3]])),
                _ => {}
            }
        }
        assert_eq!(out, b"hi\n");
        assert_eq!(code, Some(4));
    }
}
