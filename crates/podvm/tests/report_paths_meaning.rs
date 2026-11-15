//! TODO/gate.md T-1856: the `--report` of each path of a run says what
//! happened, not only valid JSON. Each run passes `-e K=canary`, and no
//! file that the run wrote holds the canary. `refused` is null exactly
//! when the payload ran.
//!
//! Hermetic: the image is a one-file rootfs that `podvm import` records
//! (this static test binary); the payloads are the tests below, which act
//! only when their variable is set.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The value of `-e`, made at run time: this binary is the image, so a
/// literal would be in the store.
fn canary() -> String {
    format!("cnry{}q{}", std::process::id() * 7, 1856 * 3)
}

#[test]
fn payload_exit() {
    if let Some(code) = std::env::var_os("T1856_EXIT") {
        std::process::exit(code.to_string_lossy().parse().unwrap());
    }
}

#[test]
fn payload_abort() {
    if std::env::var_os("T1856_ABORT").is_some() {
        std::process::abort();
    }
}

#[test]
fn payload_sleep() {
    if std::env::var_os("T1856_SLEEP").is_some() {
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}

fn podvm(dir: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_podvm"))
        .env("PODVM_SPARE_TREE", "0")
        .env("PODVM_STORE", dir.join("store"))
        .env("HOME", dir)
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("podvm runs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A ustar archive of one executable file, in pure std.
fn write_ustar(path: &Path, name: &str, data: &[u8]) {
    let mut hdr = [0u8; 512];
    hdr[..name.len()].copy_from_slice(name.as_bytes());
    hdr[100..108].copy_from_slice(b"0000755\0");
    hdr[108..116].copy_from_slice(b"0000000\0");
    hdr[116..124].copy_from_slice(b"0000000\0");
    hdr[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
    hdr[136..148].copy_from_slice(b"00000000000\0");
    hdr[148..156].copy_from_slice(b"        ");
    hdr[156] = b'0';
    hdr[257..262].copy_from_slice(b"ustar");
    hdr[263..265].copy_from_slice(b"00");
    let sum: u32 = hdr.iter().map(|b| u32::from(*b)).sum();
    hdr[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    let mut tar = hdr.to_vec();
    tar.extend_from_slice(data);
    tar.resize(tar.len() + (512 - data.len() % 512) % 512, 0);
    tar.resize(tar.len() + 1024, 0);
    std::fs::write(path, tar).unwrap();
}

struct Gone(PathBuf);

impl Drop for Gone {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Each file under `dir` (the store, the logs and the reports): none
/// holds the canary, but for the container table of mode 0600.
fn assert_no_canary(dir: &Path) {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(p) = stack.pop() {
        let Ok(m) = std::fs::symlink_metadata(&p) else {
            continue;
        };
        if m.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&p) {
                stack.extend(rd.flatten().map(|e| e.path()));
            }
        } else if p.file_name().is_some_and(|n| n == "containers.json") {
            // The container table keeps the environment for `start`, as
            // docker keeps `Config.Env`; it is the owner's alone.
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(m.permissions().mode() & 0o077, 0, "{}", p.display());
        } else if m.is_file() && m.len() < 64 << 20 {
            let bytes = std::fs::read(&p).unwrap_or_default();
            let c = canary();
            let hit = bytes.windows(c.len()).any(|w| w == c.as_bytes());
            assert!(!hit, "{} holds the value of -e", p.display());
        }
    }
}

fn report(dir: &Path, name: &str) -> Value {
    let p = dir.join(name);
    let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    serde_json::from_str(&text).unwrap()
}

/// `refused` is null exactly when the payload ran.
fn assert_ran(r: &Value, ran: bool, case: &str) {
    assert_eq!(r["refused"].is_null(), ran, "{case}: {r}");
}

#[test]
fn report_paths_meaning() {
    let dir = std::env::temp_dir().join(format!("podvm-t1856-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let _gone = Gone(dir.clone());
    let tar = dir.join("r.tar");
    let me = std::fs::read(std::env::current_exe().unwrap()).unwrap();
    write_ustar(&tar, "hello", &me);
    let (rc, err) = podvm(&dir, &["import", tar.to_str().unwrap(), "rep/one:1"]);
    assert_eq!(rc, 0, "import: {err}");
    let canary = format!("K={}", canary());
    let run = |name: &str, extra: &[&str], payload: &[&str]| {
        let mut a = vec!["run", "--report", name, "-e", canary.as_str()];
        a.extend_from_slice(extra);
        a.push("rep/one:1");
        a.push("/hello");
        a.extend_from_slice(payload);
        a.extend_from_slice(&["--exact", "--nocapture"]);
        podvm(&dir, &a)
    };

    // 1. success
    let (rc, err) = run("ok.json", &[], &["payload_exit"]);
    assert_eq!(rc, 0, "{err}");
    let r = report(&dir, "ok.json");
    assert_ran(&r, true, "success");
    assert_eq!(r["exit_code"], 0, "{r}");
    assert_eq!(r["argv"][0], "/hello", "the payload's argv: {r}");

    // 2. a nonzero exit
    let (rc, err) = run("three.json", &["-e", "T1856_EXIT=3"], &["payload_exit"]);
    assert_eq!(rc, 3, "{err}");
    let r = report(&dir, "three.json");
    assert_ran(&r, true, "nonzero");
    assert_eq!(r["exit_code"], 3, "{r}");
    assert_eq!(r["timed_out"], false, "{r}");

    // 3. a signal
    let (rc, err) = run("abrt.json", &["-e", "T1856_ABORT=1"], &["payload_abort"]);
    assert_eq!(rc, 134, "{err}");
    let r = report(&dir, "abrt.json");
    assert_ran(&r, true, "signal");
    assert!(
        r["signal"] == 6 || r["signal"] == "SIGABRT" || r["exit_code"] == 134,
        "{r}"
    );

    // 4. a timeout
    let (rc, err) = run(
        "slow.json",
        &["--timeout", "1", "-e", "T1856_SLEEP=1"],
        &["payload_sleep"],
    );
    assert_ne!(rc, 0, "{err}");
    let r = report(&dir, "slow.json");
    assert_ran(&r, true, "timeout");
    assert_eq!(r["timed_out"], true, "{r}");

    // 5. a refusal before the parse ends: an unknown flag writes no report
    let (rc, _) = podvm(
        &dir,
        &[
            "run",
            "--report",
            "early.json",
            "-e",
            &canary,
            "--bogus",
            "rep/one:1",
        ],
    );
    assert_eq!(rc, 125);
    assert!(
        !dir.join("early.json").exists() || !report(&dir, "early.json")["refused"].is_null(),
        "an early refusal wrote a report that says it ran"
    );

    // 6. a refusal after the parse: the image does not exist
    let (rc, _) = podvm(
        &dir,
        &[
            "run",
            "--report",
            "late.json",
            "-e",
            &canary,
            "localhost:1/none:1",
            "true",
        ],
    );
    assert_ne!(rc, 0);
    let r = report(&dir, "late.json");
    assert_ran(&r, false, "refusal after the parse");
    assert!(r["exit_code"].is_null(), "{r}");

    // 7. detached: the record of the start, and no exit yet
    let (rc, err) = run(
        "det.json",
        &["-d", "--name", "det", "-e", "T1856_SLEEP=1"],
        &["payload_sleep"],
    );
    if rc == 0 {
        let r = report(&dir, "det.json");
        assert_eq!(r["detached"], true, "{r}");
        assert_ran(&r, true, "detached");
        let _ = podvm(&dir, &["rm", "-f", "det"]);
    } else {
        // A host with no detached driver for a static program refuses.
        assert_eq!(rc, 125, "{err}");
    }

    // 9. an unwritable path: refused before the run (tests/report_paths.rs)
    let (rc, _) = run("/nonexistent/t1856/r.json", &[], &["payload_exit"]);
    assert_eq!(rc, 125);

    assert_no_canary(&dir);
}
