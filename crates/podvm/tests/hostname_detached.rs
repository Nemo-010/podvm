//! TODO/enter.md T-1926 (PodUtils/podrunner#196): `run -d --hostname NAME`
//! on the namespace rung. pod-enter opens a UTS namespace for the plan and
//! sets the name before the exec, so the payload reads NAME from the
//! kernel, and podvm does not refuse the flag.
//!
//! The image is the host's `cat` with the loader and libc of the host,
//! imported from a tar (as `tmpfs_exec.rs`), with a `/proc` for the rung to
//! mount. A host with no glibc at the paths of x86_64, or a run that does
//! not enter the namespace rung, skips the test with a line on stderr.

use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

const FILES: &[(&str, &[&str])] = &[
    (
        "lib64/ld-linux-x86-64.so.2",
        &["/lib64/ld-linux-x86-64.so.2"],
    ),
    (
        "lib/x86_64-linux-gnu/libc.so.6",
        &[
            "/lib/x86_64-linux-gnu/libc.so.6",
            "/usr/lib/x86_64-linux-gnu/libc.so.6",
        ],
    ),
    ("usr/bin/cat", &["/usr/bin/cat", "/bin/cat"]),
];

/// The tar of the image, or `None` where the host lacks a file.
fn image_tar() -> Option<Vec<u8>> {
    let mut b = tar::Builder::new(Vec::new());
    for dir in ["tmp", "etc", "proc", "dev", "sys"] {
        let mut h = tar::Header::new_gnu();
        h.set_entry_type(tar::EntryType::Directory);
        h.set_mode(if dir == "tmp" { 0o1777 } else { 0o755 });
        h.set_size(0);
        h.set_cksum();
        b.append_data(&mut h, format!("{dir}/"), &[][..]).ok()?;
    }
    for (inside, candidates) in FILES {
        let host = candidates.iter().map(Path::new).find(|p| p.exists())?;
        let bytes = std::fs::read(std::fs::canonicalize(host).ok()?).ok()?;
        let mut h = tar::Header::new_gnu();
        h.set_size(bytes.len() as u64);
        h.set_mode(0o755);
        h.set_cksum();
        b.append_data(&mut h, inside, &bytes[..]).ok()?;
    }
    b.into_inner().ok()
}

fn podvm(store: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_podvm"))
        // TODO/gate.md T-1859: no detached spare-tree fill writes into the
        // store after the test removed it.
        .env("PODVM_SPARE_TREE", "0")
        .env("PODVM_STORE", store)
        .args(args)
        .output()
        .unwrap()
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

#[test]
fn run_detached_hostname_names_the_uts_namespace_of_the_run() {
    if !cfg!(target_arch = "x86_64") {
        return;
    }
    let Some(tar) = image_tar() else {
        eprintln!("T-1926: this host has no glibc at the x86_64 paths; skipped");
        return;
    };
    let dir = std::env::temp_dir().join(format!("podvm-t1926-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("img.tar"), tar).unwrap();
    let store = dir.join("store");
    let tar = dir.join("img.tar");
    let i = podvm(&store, &["import", tar.to_str().unwrap(), "t1926:1"]);
    assert!(i.status.success(), "import: {}", text(&i.stderr));

    // The rung that a detached run enters here, read from a run with no name.
    let probe = podvm(
        &store,
        &[
            "run",
            "-d",
            "--name",
            "t1926-probe",
            "t1926:1",
            "cat",
            "/etc/hostname",
        ],
    );
    let rung = text(&podvm(&store, &["inspect", "t1926-probe"]).stdout);
    let _ = podvm(&store, &["rm", "-f", "t1926-probe"]);
    if !probe.status.success() || !rung.contains("\"Rung\":\"namespace\"") {
        eprintln!("T-1926: run -d does not enter the namespace rung here; skipped");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }

    let r = podvm(
        &store,
        &[
            "run",
            "-d",
            "--name",
            "t1926",
            "--hostname",
            "t1926-name",
            "t1926:1",
            "cat",
            "/proc/sys/kernel/hostname",
        ],
    );
    assert!(r.status.success(), "run -d --hostname: {}", text(&r.stderr));
    // The payload ends at once; its log holds the name.
    let end = Instant::now() + Duration::from_secs(30);
    let mut log = String::new();
    while Instant::now() < end {
        log = text(&podvm(&store, &["logs", "t1926"]).stdout);
        if !log.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert_eq!(log.trim(), "t1926-name", "the payload's host name");
    let _ = podvm(&store, &["rm", "-f", "t1926"]);
    let _ = std::fs::remove_dir_all(&dir);
}
