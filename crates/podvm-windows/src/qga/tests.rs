//! TODO/machine.md T-1868: the tests of `qga` against a stand-in agent on
//! a Unix socket in the temporary directory, with no QEMU. The stand-in
//! answers as the agent of the virtio-win tools does: the delimited sync
//! after a byte 0xFF, `guest-ping`, `guest-exec` and its status (the end
//! flag only after two asks), the file calls (the end flag of a read only
//! on a read of no byte, as on Windows) and `guest-shutdown` with no
//! answer. pod-qga keeps its own stand-in in its tests, and pod-testkit
//! has none, so this file has a small one.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};

use super::*;

/// The guest of the stand-in.
#[derive(Default)]
struct Guest {
    calls: Vec<String>,
    execs: Vec<Value>,
    files: BTreeMap<String, Vec<u8>>,
    open: BTreeMap<i64, (String, usize)>,
    asks: BTreeMap<i64, usize>,
    shutdown: Option<String>,
    /// The syncs that get no answer: an agent that does not run yet.
    mute: usize,
    next: i64,
}

/// One stand-in agent; the socket goes when it drops.
struct Agent {
    path: PathBuf,
    guest: Arc<Mutex<Guest>>,
}

impl Drop for Agent {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn start(guest: Guest) -> Agent {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("pvqga-{}-{n}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).expect("the socket of the stand-in");
    let guest = Arc::new(Mutex::new(guest));
    let g = guest.clone();
    std::thread::spawn(move || {
        for s in listener.incoming().flatten() {
            serve(s, &g);
        }
    });
    Agent { path, guest }
}

fn serve(s: UnixStream, g: &Arc<Mutex<Guest>>) {
    let mut out = s.try_clone().expect("a clone of the stream");
    let mut r = BufReader::new(s);
    let mut line = Vec::new();
    loop {
        line.clear();
        match r.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let text: Vec<u8> = line.iter().copied().filter(|b| *b != 0xFF).collect();
        let Ok(req) = serde_json::from_slice::<Value>(&text) else {
            continue;
        };
        let reply = g.lock().unwrap().answer(&req);
        match reply {
            Some(bytes) => {
                if out.write_all(&bytes).is_err() {
                    return;
                }
            }
            None if req["execute"] == "guest-shutdown" => return,
            None => {}
        }
    }
}

fn ret(v: Value) -> Option<Vec<u8>> {
    Some(format!("{}\n", json!({ "return": v })).into_bytes())
}

fn refuse(desc: String) -> Option<Vec<u8>> {
    Some(
        format!(
            "{}\n",
            json!({"error": {"class": "GenericError", "desc": desc}})
        )
        .into_bytes(),
    )
}

impl Guest {
    fn answer(&mut self, req: &Value) -> Option<Vec<u8>> {
        let a = &req["arguments"];
        let call = req["execute"].as_str().unwrap_or("").to_string();
        self.calls.push(call.clone());
        self.next += 1;
        match call.as_str() {
            "guest-sync-delimited" if self.mute > 0 => {
                self.mute -= 1;
                None
            }
            "guest-sync-delimited" => {
                let mut b = vec![0xFF];
                b.extend(ret(a["id"].clone())?);
                Some(b)
            }
            "guest-ping" => ret(json!({})),
            "guest-exec" => {
                self.execs.push(a.clone());
                ret(json!({ "pid": 4000 + self.execs.len() as i64 }))
            }
            "guest-exec-status" => {
                let pid = a["pid"].as_i64().unwrap_or(0);
                let asked = self.asks.entry(pid).or_default();
                *asked += 1;
                if *asked < 2 {
                    return ret(json!({ "exited": false }));
                }
                let e = &self.execs[(pid - 4001) as usize];
                let words: Vec<&str> = e["arg"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(Value::as_str)
                    .collect();
                // `exit N` ends with N; each other command echoes its words.
                let code = words
                    .iter()
                    .position(|w| *w == "exit")
                    .and_then(|i| words.get(i + 1)?.parse::<i32>().ok())
                    .unwrap_or(0);
                let out = format!("{}\r\n", words.join(" "));
                ret(json!({
                    "exited": true, "exitcode": code,
                    "out-data": STANDARD.encode(out), "err-data": STANDARD.encode("warn\r\n"),
                }))
            }
            "guest-file-open" => {
                let path = a["path"].as_str().unwrap_or("").to_string();
                if a["mode"] == "w" {
                    self.files.insert(path.clone(), Vec::new());
                } else if !self.files.contains_key(&path) {
                    return refuse(format!("failed to open file '{path}'"));
                }
                self.open.insert(self.next, (path, 0));
                ret(json!(self.next))
            }
            "guest-file-read" => {
                let h = a["handle"].as_i64().unwrap_or(-1);
                let count = a["count"].as_u64().unwrap_or(0) as usize;
                let (path, at) = self.open.get_mut(&h)?;
                let data = &self.files[path.as_str()];
                let end = (*at + count).min(data.len());
                let part = data[*at..end].to_vec();
                *at = end;
                // As on Windows: the end flag only on a read of no byte.
                ret(
                    json!({ "count": part.len(), "buf-b64": STANDARD.encode(&part), "eof": part.is_empty() }),
                )
            }
            "guest-file-write" => {
                let h = a["handle"].as_i64().unwrap_or(-1);
                let path = self.open.get(&h)?.0.clone();
                let data = STANDARD
                    .decode(a["buf-b64"].as_str().unwrap_or(""))
                    .unwrap();
                self.files.get_mut(&path)?.extend_from_slice(&data);
                ret(json!({ "count": data.len(), "eof": false }))
            }
            "guest-file-close" => {
                self.open.remove(&a["handle"].as_i64().unwrap_or(-1))?;
                ret(json!({}))
            }
            "guest-shutdown" => {
                self.shutdown = Some(a["mode"].as_str().unwrap_or("").to_string());
                None
            }
            other => refuse(format!("The command {other} has not been found")),
        }
    }
}

fn client(a: &Agent) -> Client {
    ready_with(
        &a.path,
        Duration::from_secs(5),
        Duration::from_millis(300),
        Duration::from_secs(5),
    )
    .expect("the stand-in answers")
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// The channel of a plan is the virtio-serial port of the agent on the
/// socket, and the boot line names it.
#[test]
fn qga_channel_is_on_the_plan() {
    let sock = Path::new("/run/test/podvm-m-1-win.qga");
    let c = channel(sock);
    assert_eq!(c.name, "org.qemu.guest_agent.0");
    let mut o = crate::options::desktop(crate::Accel::Kvm, Vec::new());
    o.channels.push(c);
    o.check().expect("a valid channel");
    let d = PathBuf::from("/run/test/podvm-plan");
    let line = crate::argv(&crate::Plan {
        emulator: "qemu-system-x86_64".into(),
        accel: crate::Accel::Kvm,
        share: "/usr/share/qemu".into(),
        firmware_code: "/code.fd".into(),
        firmware_vars: d.join("vars.fd"),
        root: d.join("run.qcow2"),
        mailbox: d.join("mailbox.img"),
        serial: d.join("serial.log"),
        monitor: d.join("qmp.sock"),
        memory_mib: 4096,
        cpus: 2,
        emu_args: Vec::new(),
        options: o,
    })
    .join(" ");
    assert!(line.contains("virtserialport"), "{line}");
    assert!(line.contains("name=org.qemu.guest_agent.0"), "{line}");
    assert!(
        line.contains("path=/run/test/podvm-m-1-win.qga,server=on,wait=off"),
        "{line}"
    );
}

/// The probe waits for an agent that does not answer its first syncs, and
/// gives up at its bound where no agent answers.
#[test]
fn qga_ready_waits_for_the_agent_and_gives_up_at_the_bound() {
    let a = start(Guest {
        mute: 1,
        ..Guest::default()
    });
    let mut c = client(&a);
    c.ping().unwrap();
    assert!(a
        .guest
        .lock()
        .unwrap()
        .calls
        .contains(&"guest-ping".to_string()));
    let gone = std::env::temp_dir().join(format!("pvqga-none-{}.sock", std::process::id()));
    let e = match ready_with(
        &gone,
        Duration::from_millis(200),
        Duration::from_millis(100),
        CALL,
    ) {
        Ok(_) => panic!("no agent answered"),
        Err(e) => e,
    };
    assert!(matches!(e.kind(), ErrorKind::NoAgent { .. }), "{e}");
}

/// `exec` runs `cmd.exe /d /c ARGV` (with `cd /d DIR &&` for a working
/// directory), waits for the end, keeps the output, and the exit code
/// passes through in 0 to 124.
#[test]
fn qga_exec_passes_the_exit_code_through() {
    let a = start(Guest::default());
    let mut c = client(&a);
    let e = command(&strings(&["ver"]), None).unwrap();
    let s = run(&mut c, &e, Duration::from_secs(5)).unwrap();
    assert_eq!(exit_of(&s), Ok(0));
    assert_eq!(s.stdout, b"/d /c ver\r\n");
    assert_eq!(s.stderr, b"warn\r\n");
    let e = command(&strings(&["exit", "7"]), Some(r"C:\Users\podvm")).unwrap();
    let s = run(&mut c, &e, Duration::from_secs(5)).unwrap();
    assert_eq!(exit_of(&s), Ok(7));
    let g = a.guest.lock().unwrap();
    assert_eq!(g.execs[1]["path"], "cmd.exe");
    assert_eq!(
        g.execs[1]["arg"],
        json!(["/d", "/c", "cd", "/d", r"C:\Users\podvm", "&&", "exit", "7"])
    );
    assert_eq!(g.execs[1]["capture-output"], true);
    drop(g);
    let big = ExecStatus {
        exited: true,
        exit_code: Some(125),
        ..ExecStatus::default()
    };
    assert!(exit_of(&big).unwrap_err().contains("0-124"));
    assert!(exit_of(&ExecStatus::default()).is_err());
}

/// A value that `cmd.exe` cannot take is refused before any request.
#[test]
fn qga_exec_refuses_a_quote_and_no_command() {
    assert!(command(&[], None).is_err());
    let e = command(&strings(&["echo", "a\"b"]), None).unwrap_err();
    assert!(e.contains("double quote"), "{e}");
    assert!(command(&strings(&["ver"]), Some("C:\\a\nb")).is_err());
}

/// A file goes into the guest and back out in chunks, with each handle
/// closed and the bytes kept; a missing file is the refusal of the agent.
#[test]
fn qga_copy_round_trip_keeps_the_bytes() {
    let a = start(Guest::default());
    let mut c = client(&a);
    let data: Vec<u8> = (0..(2 * CHUNK + 12_345))
        .map(|i| (i * 31 % 251) as u8)
        .collect();
    let n = copy_in(&mut c, &mut data.as_slice(), r"C:\podvm\in.bin").unwrap();
    assert_eq!(n, data.len() as u64);
    let mut back = Vec::new();
    let m = copy_out(&mut c, r"C:\podvm\in.bin", &mut back).unwrap();
    assert_eq!(m, n);
    assert!(back == data, "the bytes changed");
    let mut none = Vec::new();
    let e = copy_out(&mut c, r"C:\no\such.txt", &mut none).unwrap_err();
    assert!(e.contains("failed to open"), "{e}");
    let g = a.guest.lock().unwrap();
    assert!(g.open.is_empty(), "a handle stays open");
    assert!(g.calls.iter().filter(|c| *c == "guest-file-write").count() >= 3);
}

/// The names of `cp`: the first `:` ends the name of the machine, and a
/// directory as the target gets the name of the source.
#[test]
fn qga_cp_names_split_and_join() {
    assert_eq!(
        split_machine(r"win1:C:\x\y.txt"),
        Some(("win1", r"C:\x\y.txt"))
    );
    assert_eq!(split_machine("win1:"), None);
    assert_eq!(split_machine("/tmp/a"), None);
    assert_eq!(split_machine("../x:C:\\a"), None);
    assert_eq!(base_name(r"C:\Windows\win.ini"), "win.ini");
    assert_eq!(base_name("C:/a/b/"), "b");
    assert_eq!(
        guest_target(r"C:\podvm\", Path::new("/tmp/f.txt")),
        r"C:\podvm\f.txt"
    );
    assert_eq!(
        guest_target(r"C:\podvm\g.txt", Path::new("/tmp/f.txt")),
        r"C:\podvm\g.txt"
    );
}

/// The graceful stop sends `guest-shutdown` with the mode `powerdown`;
/// the agent gives no answer and the call is a success.
#[test]
fn qga_shutdown_asks_for_a_power_down() {
    let a = start(Guest::default());
    shutdown(&a.path, Duration::from_secs(2)).unwrap();
    // The stand-in reads the request in its own thread.
    for _ in 0..50 {
        if a.guest.lock().unwrap().shutdown.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        a.guest.lock().unwrap().shutdown.as_deref(),
        Some("powerdown")
    );
}
