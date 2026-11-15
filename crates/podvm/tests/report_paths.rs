//! TODO/cli.md T-1825: a `--report` path whose directory takes no file is
//! refused with 125 before the payload runs, and before any image is
//! fetched. It printed a notice after the run and exited 0.

use std::process::Command;

#[test]
fn report_paths_refuse_an_unwritable_directory_before_the_run() {
    let store = std::env::temp_dir().join(format!("podvm-report-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&store);
    let out = Command::new(env!("CARGO_BIN_EXE_podvm"))
        .args([
            "run",
            "--report",
            "/nonexistent/podvm/r.json",
            "no.such.registry.test/none:1",
            "true",
        ])
        .env("PODVM_STORE", &store)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(125), "{err}");
    assert!(
        err.contains("T-1825") && err.contains("/nonexistent/podvm"),
        "{err}"
    );
    assert!(
        !err.contains("no.such.registry.test"),
        "it reached the image: {err}"
    );
    let _ = std::fs::remove_dir_all(&store);
}

/// A payload that holds while the detached half of the test bounds it.
#[test]
fn payload_sleep() {
    if std::env::var_os("T1825_SLEEP").is_some() {
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}

/// A ustar archive of one executable file, in pure std.
fn write_ustar(path: &std::path::Path, name: &str, data: &[u8]) {
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

/// TODO/cli.md T-1825: the launcher of a detached run writes the end of
/// the payload into its report: `--timeout 1` gives `timed_out: true` and
/// `signal: 9` after `wait`.
#[test]
fn report_paths_a_detached_run_gets_its_end() {
    let dir = std::env::temp_dir().join(format!("podvm-report-d-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let podvm = |args: &[&str]| {
        let o = Command::new(env!("CARGO_BIN_EXE_podvm"))
            .args(args)
            .current_dir(&dir)
            .env("PODVM_STORE", dir.join("store"))
            .env("PODVM_SPARE_TREE", "0")
            .output()
            .unwrap();
        (
            o.status.code(),
            String::from_utf8_lossy(&o.stderr).into_owned(),
        )
    };
    let tar = dir.join("r.tar");
    write_ustar(
        &tar,
        "hello",
        &std::fs::read(std::env::current_exe().unwrap()).unwrap(),
    );
    let (rc, err) = podvm(&["import", tar.to_str().unwrap(), "rep/d:1"]);
    assert_eq!(rc, Some(0), "{err}");
    let (rc, err) = podvm(&[
        "run",
        "-d",
        "--name",
        "d1",
        "--timeout",
        "1",
        "--report",
        "d.json",
        "-e",
        "T1825_SLEEP=1",
        "rep/d:1",
        "/hello",
        "payload_sleep",
        "--exact",
    ]);
    if rc != Some(0) {
        // A host with no detached driver for a static program refuses.
        assert_eq!(rc, Some(125), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    let (rc, err) = podvm(&["wait", "d1"]);
    assert_eq!(rc, Some(0), "{err}");
    // The launcher writes the report after the record; wait for it.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let r: serde_json::Value = loop {
        let text = std::fs::read_to_string(dir.join("d.json")).unwrap();
        let r: serde_json::Value = serde_json::from_str(&text).unwrap();
        if !r["exit_code"].is_null() || std::time::Instant::now() > deadline {
            break r;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    assert_eq!(r["detached"], true, "{r}");
    assert_eq!(r["timed_out"], true, "{r}");
    assert_eq!(r["signal"], 9, "{r}");
    assert_eq!(r["exit_code"], 137, "{r}");
    assert!(r["refused"].is_null(), "{r}");
    let _ = podvm(&["rm", "-f", "d1"]);
    let _ = std::fs::remove_dir_all(&dir);
}
