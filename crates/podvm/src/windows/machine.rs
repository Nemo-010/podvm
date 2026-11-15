//! TODO/machine.md T-1870: a detached Windows or DOS guest. `run -d
//! windows:NAME` makes a machine (`podvm_windows::machines`), and `ps`,
//! `stop`, `rm` and `logs` know machines beside containers.
//!
//! ⛔ The emulator never runs without an owner. `run -d` starts a keeper
//! (`podvm windows keeper NAME ACCEL`, its own process group) and returns
//! when the keeper wrote the pid of the emulator into the record. The
//! keeper waits for the emulator, stops it at the end of its lifetime
//! (`--timeout`, else 24 h), and writes how it ended. `stop` and `rm -f`
//! end the emulator and wait for the keeper to finish.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use podvm_image::error::{EXIT_CLI_ERROR, EXIT_FLAG_ERROR, EXIT_RUNTIME_ERROR};
use podvm_windows::catalog::Kind;
use podvm_windows::machines::{self, Record};
use podvm_windows::{Accel, Plan};

/// The time that `run -d` waits for the keeper to start the emulator.
const START_WAIT: Duration = Duration::from_secs(20);
/// The time that `stop` gives the guest to power off after ACPI, before
/// QMP `quit` (docker gives 10 s).
const STOP_GRACE: Duration = Duration::from_secs(10);

/// The plan of the machine in `dir`: each per-machine file is in it.
fn plan_of(r: &Record, dir: &Path, accel: Accel) -> Plan {
    let (code, _) = super::plan::firmware();
    let options = match r.flavor.as_str() {
        "desktop" => podvm_windows::options::desktop(accel, Vec::new()),
        _ => podvm_windows::options::mailbox(),
    };
    Plan {
        emulator: super::plan::emulator(),
        accel,
        share: super::plan::share(),
        firmware_code: code,
        firmware_vars: dir.join("vars.fd"),
        root: dir.join("disk.qcow2"),
        mailbox: dir.join("mailbox.img"),
        serial: dir.join("serial.log"),
        monitor: machines::socket(&machines::root(), &r.name),
        memory_mib: r.memory_mib,
        cpus: 2,
        emu_args: Vec::new(),
        options,
    }
}

/// The base image of a catalog name, and the flavor of its boot line.
fn base_of(verb: &str, image: &str) -> Result<(PathBuf, &'static str), i32> {
    let Some(name) = podvm_windows::catalog::name_of(image) else {
        eprintln!(
            "podvm {verb}: run -d of a Windows guest takes a name of the catalog \
             (windows:dos, windows:validationos, windows:2025); {image} is none"
        );
        return Err(EXIT_FLAG_ERROR);
    };
    let e = podvm_windows::catalog::resolve(name).map_err(|why| {
        eprintln!("podvm {verb}: {why}");
        EXIT_CLI_ERROR
    })?;
    match e.kind {
        Kind::DosZip => Ok((super::pull::dos_base(verb)?, "dos")),
        Kind::ValidationOs => Ok((super::named::vos_image(verb, e)?, "mailbox")),
        Kind::Iso => Ok((super::named::golden_image(verb, e)?, "desktop")),
    }
}

/// A free name: `win-` and six hex digits of the nonce.
fn fresh_name(root: &Path) -> Result<String, i32> {
    for _ in 0..8 {
        let n = podvm_windows::agent::nonce().map_err(|_| EXIT_RUNTIME_ERROR)?;
        let name = format!("win-{}", &n[..6.min(n.len())]);
        if !root.join(&name).exists() {
            return Ok(name);
        }
    }
    Err(EXIT_RUNTIME_ERROR)
}

/// The mailbox of a machine: empty for DOS; for Windows the marker and
/// `KEEP`, so that the agent of the image polls it for a command.
fn mailbox(flavor: &str) -> podvm_windows::Result<podvm_windows::Fat16> {
    use podvm_windows::agent;
    let s = podvm_windows::settings();
    let mut m = podvm_windows::Fat16::with_oem(&s.oem)?;
    if flavor != "dos" {
        m.put(agent::MARKER, &s.marker)?;
        m.put(agent::KEEP, b"")?;
    }
    Ok(m)
}

/// `run -d windows:NAME`: make the machine, start its keeper, print the
/// name.
pub(crate) fn run_detached(
    verb: &str,
    name: Option<&str>,
    image: &str,
    mem: Option<u64>,
    lifetime: Option<u64>,
) -> i32 {
    let root = machines::root();
    let name = match name {
        Some(n) if !machines::valid_name(n) => {
            eprintln!("podvm {verb}: {n:?} is no valid name ([a-zA-Z0-9][a-zA-Z0-9_.-]*)");
            return EXIT_FLAG_ERROR;
        }
        Some(n) => n.to_string(),
        None => match fresh_name(&root) {
            Ok(n) => n,
            Err(c) => return c,
        },
    };
    let dir = machines::dir_in(&root, &name);
    if dir.exists() {
        eprintln!(
            "podvm {verb}: the name {name:?} is in use; `podvm rm {name}` removes the old one"
        );
        return EXIT_RUNTIME_ERROR;
    }
    let (base, flavor) = match base_of(verb, image) {
        Ok(b) => b,
        Err(c) => return c,
    };
    if let Err(c) = super::plan::provide(verb, flavor != "dos") {
        return c;
    }
    let accel = match super::plan::accelerator() {
        Ok(a) => a,
        Err(c) => return c,
    };
    let r = Record {
        name: name.clone(),
        image: image.to_string(),
        base: base.display().to_string(),
        flavor: flavor.to_string(),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        lifetime: lifetime.unwrap_or(machines::DEFAULT_LIFETIME_SECS),
        memory_mib: mem
            .map(|m| m / (1 << 20))
            .unwrap_or(if flavor == "dos" { 512 } else { 4096 }),
        ..Default::default()
    };
    if let Err(why) = make(&r, &dir, accel) {
        let _ = std::fs::remove_dir_all(&dir);
        eprintln!("podvm {verb}: {name}: {why}");
        return EXIT_RUNTIME_ERROR;
    }
    match start_keeper(&name, accel, &dir) {
        Ok(()) => {
            println!("{name}");
            0
        }
        Err(why) => {
            eprintln!("podvm {verb}: {name}: {why}");
            let _ = stop_record(&root, &name);
            let _ = std::fs::remove_dir_all(&dir);
            EXIT_RUNTIME_ERROR
        }
    }
}

/// The directory of a new machine: the overlay, the mailbox, the UEFI
/// variables and the record.
fn make(r: &Record, dir: &Path, accel: Accel) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let plan = plan_of(r, dir, accel);
    let base = std::fs::canonicalize(&r.base).unwrap_or_else(|_| PathBuf::from(&r.base));
    podvm_windows::emulator::run_steps(&podvm_windows::overlay_argv(
        &super::plan::qemu_img(),
        &base,
        &plan.root,
    ))
    .map_err(|e| e.to_string())?;
    let m = mailbox(&r.flavor).map_err(|e| e.to_string())?;
    std::fs::write(&plan.mailbox, m.image()).map_err(|e| e.to_string())?;
    if r.flavor != "dos" {
        let (_, vars) = super::plan::firmware();
        std::fs::copy(&vars, &plan.firmware_vars)
            .map_err(|e| format!("{}: {e}", vars.display()))?;
    }
    machines::write(&machines::root(), r).map_err(|e| e.to_string())
}

/// Start the keeper in its own process group, and wait until it wrote the
/// pid of the emulator.
fn start_keeper(name: &str, accel: Accel, dir: &Path) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let log = std::fs::File::create(dir.join("keeper.log")).map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    std::process::Command::new(exe)
        .args(["windows", "keeper", name, accel.word()])
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(err)
        .process_group(0)
        .spawn()
        .map_err(|e| e.to_string())?;
    let t0 = Instant::now();
    let root = machines::root();
    while t0.elapsed() < START_WAIT {
        if let Some(r) = machines::read(&root, name) {
            if r.running() {
                return Ok(());
            }
            if !r.ended.is_empty() {
                return Err(format!("the emulator ended at once: {}", r.ended));
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let tail = std::fs::read_to_string(dir.join("keeper.log")).unwrap_or_default();
    Err(format!(
        "the keeper started no emulator within {} s: {}",
        START_WAIT.as_secs(),
        tail.lines().last().unwrap_or("no line")
    ))
}

/// `podvm windows keeper NAME ACCEL`: own the emulator of the machine.
pub(crate) fn keeper(args: &[String]) -> i32 {
    let (Some(name), Some(word)) = (args.first(), args.get(1)) else {
        eprintln!("podvm windows keeper: NAME and ACCEL are needed");
        return EXIT_FLAG_ERROR;
    };
    let accel = if word == "kvm" {
        Accel::Kvm
    } else {
        Accel::Tcg
    };
    let root = machines::root();
    let Some(mut r) = machines::read(&root, name) else {
        eprintln!("podvm windows keeper: no machine {name:?}");
        return EXIT_RUNTIME_ERROR;
    };
    let dir = machines::dir_in(&root, name);
    let plan = plan_of(&r, &dir, accel);
    let argv = if r.flavor == "dos" {
        podvm_windows::dos::argv(
            accel,
            &plan.root,
            &plan.mailbox,
            &plan.serial,
            &plan.monitor,
            plan.memory_mib,
            &[],
        )
    } else {
        podvm_windows::argv(&plan)
    };
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("emulator.log"))
        .map(Stdio::from)
        .unwrap_or_else(|_| Stdio::null());
    let mut child = match podvm_windows::emulator::spawn_emulator(&plan, &argv, log) {
        Ok(c) => c,
        Err(e) => {
            r.ended = format!("the emulator did not start: {e}");
            let _ = machines::write(&root, &r);
            return EXIT_RUNTIME_ERROR;
        }
    };
    r.keeper_pid = std::process::id();
    r.keeper_start = machines::start_of(r.keeper_pid).unwrap_or(0);
    r.emulator_pid = child.id();
    r.emulator_start = machines::start_of(child.id()).unwrap_or(0);
    let _ = machines::write(&root, &r);
    let t0 = Instant::now();
    let lifetime = Duration::from_secs(r.lifetime);
    let ended = loop {
        match child.try_wait() {
            Ok(Some(s)) => break format!("the emulator ended, {s}"),
            Ok(None) => {}
            Err(e) => break format!("the wait for the emulator failed: {e}"),
        }
        if t0.elapsed() >= lifetime {
            let stop = podvm_windows::emulator::stop_emulator(&plan, &mut child);
            break format!("its lifetime of {} s ended; {stop}", r.lifetime);
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    // A `stop` may have written its own end first: that one stays.
    let mut now = machines::read(&root, name).unwrap_or(r);
    if now.ended.is_empty() {
        now.ended = ended;
    }
    let _ = machines::write(&root, &now);
    let _ = std::fs::remove_file(&plan.monitor);
    0
}

/// The record of `name`, when a machine has that name.
pub(crate) fn find(name: &str) -> Option<Record> {
    machines::valid_name(name)
        .then(|| machines::read(&machines::root(), name))
        .flatten()
}

/// End the emulator of `name`: ACPI power-off, then QMP `quit`, then
/// SIGKILL to its process group; then wait for the keeper. Gives the end
/// that it wrote, or `None` where the machine did not run.
fn stop_record(root: &Path, name: &str) -> Option<String> {
    let r = machines::read(root, name)?;
    if !r.running() {
        return None;
    }
    let sock = machines::socket(root, name);
    let mut how = "SIGKILL";
    if let Ok(mut m) = podvm_windows::qmp::Monitor::connect(&sock, Duration::from_secs(2)) {
        let _ = m.call("system_powerdown");
        let t0 = Instant::now();
        while t0.elapsed() < STOP_GRACE && r.running() {
            std::thread::sleep(Duration::from_millis(250));
        }
        how = "ACPI power-off";
        if r.running() {
            m.quit();
            std::thread::sleep(Duration::from_millis(500));
            how = "QMP quit";
        }
    }
    if r.running() {
        let _ = podvm_probe::sys::kill(-i64::from(r.emulator_pid), 9);
        how = "SIGKILL";
    }
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(10)
        && (r.running() || machines::alive(r.keeper_pid, r.keeper_start))
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    let end = format!("stopped by podvm stop ({how})");
    if let Some(mut now) = machines::read(root, name) {
        now.ended = end.clone();
        let _ = machines::write(root, &now);
    }
    Some(end)
}

/// `podvm stop NAME` of a machine.
pub(crate) fn stop(name: &str) -> i32 {
    let _ = stop_record(&machines::root(), name);
    println!("{name}");
    0
}

/// `rm` of `want` where a machine has that name; `None` for no machine.
pub(crate) fn rm_if(want: &str, force: bool) -> Option<i32> {
    find(want).map(|_| rm(want, force))
}

/// `run -d windows:NAME`, where the run is detached and names the catalog.
pub(crate) fn detach_if(verb: &str, o: &crate::run::Opts) -> Option<i32> {
    (verb == "run" && o.detach && super::named(o.image.as_deref())).then(|| {
        run_detached(
            verb,
            o.name.as_deref(),
            o.image.as_deref().unwrap_or_default(),
            o.mem,
            o.timeout,
        )
    })
}

/// `start` of `want` where a machine has that name: a running one changes
/// nothing; a stopped one boots again from its own disk. `None` for no
/// machine.
pub(crate) fn start_if(verb: &str, want: &str) -> Option<Result<String, i32>> {
    let mut r = find(want)?;
    if r.running() {
        return Some(Ok(r.name));
    }
    if machines::alive(r.keeper_pid, r.keeper_start) {
        eprintln!("podvm {verb}: the keeper of {want} still runs; wait for it to end");
        return Some(Err(EXIT_RUNTIME_ERROR));
    }
    let started = (|| {
        super::plan::provide(verb, r.flavor != "dos")?;
        let accel = super::plan::accelerator()?;
        r.ended.clear();
        r.keeper_pid = 0;
        r.keeper_start = 0;
        r.emulator_pid = 0;
        r.emulator_start = 0;
        let root = machines::root();
        machines::write(&root, &r).map_err(|e| {
            eprintln!("podvm {verb}: {want}: {e}");
            EXIT_RUNTIME_ERROR
        })?;
        start_keeper(&r.name, accel, &machines::dir_in(&root, &r.name)).map_err(|why| {
            eprintln!("podvm {verb}: {want}: {why}");
            EXIT_RUNTIME_ERROR
        })
    })();
    Some(started.map(|()| r.name.clone()))
}

/// `podvm rm [-f] NAME` of a machine.
pub(crate) fn rm(name: &str, force: bool) -> i32 {
    let root = machines::root();
    let Some(r) = machines::read(&root, name) else {
        return EXIT_RUNTIME_ERROR;
    };
    if r.running() {
        if !force {
            eprintln!(
                "podvm rm: the machine {name} runs; `podvm stop {name}` first, or `podvm rm -f {name}`"
            );
            return EXIT_RUNTIME_ERROR;
        }
        let _ = stop_record(&root, name);
    }
    if r.running() || machines::alive(r.keeper_pid, r.keeper_start) {
        eprintln!("podvm rm: the emulator of {name} did not end; nothing was removed");
        return EXIT_RUNTIME_ERROR;
    }
    match std::fs::remove_dir_all(machines::dir_in(&root, name)) {
        Ok(()) => {
            println!("{name}");
            0
        }
        Err(e) => {
            eprintln!("podvm rm: {name}: {e}");
            EXIT_RUNTIME_ERROR
        }
    }
}

/// `podvm logs [-f] NAME` of a machine: the serial line of the guest; with
/// `follow`, the new bytes while the machine runs.
pub(crate) fn logs(name: &str, follow: bool) -> i32 {
    use std::io::{Read, Seek, Write};
    let root = machines::root();
    let path = machines::dir_in(&root, name).join("serial.log");
    let mut out = std::io::stdout().lock();
    let mut at = 0u64;
    loop {
        if let Ok(mut f) = std::fs::File::open(&path)
            && f.seek(std::io::SeekFrom::Start(at)).is_ok()
        {
            let mut buf = Vec::new();
            if f.read_to_end(&mut buf).is_ok() && !buf.is_empty() {
                at += buf.len() as u64;
                if out.write_all(&buf).and_then(|()| out.flush()).is_err() {
                    return 0;
                }
            }
        }
        if !follow || !machines::read(&root, name).is_some_and(|r| r.running()) {
            return 0;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// The rows of `podvm ps` for the machines: each one with `all`, else the
/// running ones. The columns are those of the containers.
pub(crate) fn ps_rows(all: bool) -> Vec<Vec<String>> {
    machines::list(&machines::root())
        .into_iter()
        .filter(|r| all || r.running())
        .map(|r| {
            vec![
                r.name.clone(),
                r.image.clone(),
                format!("({} guest)", r.flavor),
                podvm_image::clock::iso8601(std::time::UNIX_EPOCH + Duration::from_secs(r.created)),
                r.status(),
                r.name.clone(),
            ]
        })
        .collect()
}
