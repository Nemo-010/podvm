//! TODO/supervise.md T-1848: `podvm container prune` removes each exited
//! record and its tree, and keeps a created or running one.
//!
//! Hermetic: the image is a one-file rootfs that `podvm import` records
//! (this static test binary), and a foreground `run` leaves its record.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A payload that holds while the running half of the test looks at it.
#[test]
fn sleeper() {
    if std::env::var_os("T1848_SLEEP").is_some() {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

fn podvm(store: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_podvm"))
        .env("PODVM_SPARE_TREE", "0")
        .args(args)
        .env("PODVM_STORE", store)
        .output()
        .expect("podvm runs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
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

fn names(store: &Path) -> Vec<String> {
    let (rc, out, err) = podvm(store, &["ps", "-a", "--format", "{{.Names}}"]);
    assert_eq!(rc, 0, "ps: {err}");
    let mut v: Vec<String> = out.lines().map(str::to_string).collect();
    v.sort();
    v
}

#[test]
fn container_prune_removes_exited_records_and_frees_their_trees() {
    let dir = std::env::temp_dir().join(format!("podvm-t1848-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let _gone = Gone(dir.clone());
    let tar = dir.join("r.tar");
    let me = std::fs::read(std::env::current_exe().unwrap()).unwrap();
    write_ustar(&tar, "hello", &me);
    let store = dir.join("store");
    let (rc, _, err) = podvm(&store, &["import", tar.to_str().unwrap(), "prune/one:1"]);
    assert_eq!(rc, 0, "import: {err}");
    for n in ["ex1", "ex2", "ex3"] {
        let (rc, _, err) = podvm(
            &store,
            &["run", "--name", n, "prune/one:1", "/hello", "--list"],
        );
        assert_eq!(rc, 0, "run {n}: {err}");
    }
    let (rc, _, err) = podvm(
        &store,
        &["create", "--name", "made", "prune/one:1", "/hello"],
    );
    assert_eq!(rc, 0, "create: {err}");
    // A running record, where this host has a detached driver for a
    // static program (a chroot, or the native-static rung).
    let (rc, _, _) = podvm(
        &store,
        &[
            "run",
            "-d",
            "--name",
            "up",
            "-e",
            "T1848_SLEEP=1",
            "prune/one:1",
            "/hello",
            "sleeper",
            "--exact",
        ],
    );
    let running = rc == 0;
    let trees = store.join("trees");
    let before = std::fs::read_dir(&trees).unwrap().count();

    let (rc, out, err) = podvm(&store, &["container", "prune", "-f"]);
    assert_eq!(rc, 0, "prune: {err}");
    assert!(out.contains("Deleted Containers:"), "{out}");
    assert!(out.contains("Total reclaimed space:"), "{out}");
    assert!(!out.contains("0 B") && !out.contains(" 0.0 MiB"), "{out}");
    let mut kept = vec!["made".to_string()];
    if running {
        kept.push("up".to_string());
    }
    kept.sort();
    assert_eq!(names(&store), kept);
    // The trees go at the exit of podvm (TODO/image.md T-1781).
    let after = std::fs::read_dir(&trees)
        .unwrap()
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .count();
    assert!(after < before, "trees before {before}, after {after}");

    // A second prune finds nothing and says so.
    let (rc, out, _) = podvm(&store, &["container", "prune"]);
    assert_eq!(rc, 0);
    assert!(!out.contains("Deleted Containers:"), "{out}");
    if running {
        let _ = podvm(&store, &["rm", "-f", "up"]);
    }
}
