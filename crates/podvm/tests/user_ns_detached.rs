//! TODO/enter.md T-1927 (PodUtils/podrunner#196): on the namespace rung of
//! a user with no root, `run -d` runs the payload as the image's `USER`,
//! or as `--user`, inside the user namespace; `--user 0` keeps uid 0.
//!
//! The image is an OCI layout with the host's `cat`, its loader and libc,
//! an `/etc/passwd` with `runner` as 1001, and `USER runner` in its config.
//! A host with no glibc at the x86_64 paths, a root caller (pod-enter maps
//! nothing for root), or a run that does not enter the namespace rung skips
//! the test with a line on stderr.

use sha2::{Digest, Sha256};
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

fn add(b: &mut tar::Builder<Vec<u8>>, name: &str, mode: u32, bytes: &[u8]) -> Option<()> {
    let mut h = tar::Header::new_gnu();
    h.set_size(bytes.len() as u64);
    h.set_mode(mode);
    h.set_cksum();
    b.append_data(&mut h, name, bytes).ok()
}

/// The layer: a rootfs tar, or `None` where the host lacks a file.
fn layer() -> Option<Vec<u8>> {
    let mut b = tar::Builder::new(Vec::new());
    for dir in ["tmp", "etc", "proc", "dev", "sys"] {
        let mut h = tar::Header::new_gnu();
        h.set_entry_type(tar::EntryType::Directory);
        h.set_mode(if dir == "tmp" { 0o1777 } else { 0o755 });
        h.set_size(0);
        h.set_cksum();
        b.append_data(&mut h, format!("{dir}/"), &[][..]).ok()?;
    }
    let passwd = b"root:x:0:0:root:/root:/bin/sh\nrunner:x:1001:1001::/home/runner:/bin/sh\n";
    add(&mut b, "etc/passwd", 0o644, passwd)?;
    for (inside, candidates) in FILES {
        let host = candidates.iter().map(Path::new).find(|p| p.exists())?;
        let bytes = std::fs::read(std::fs::canonicalize(host).ok()?).ok()?;
        add(&mut b, inside, 0o755, &bytes)?;
    }
    b.into_inner().ok()
}

fn sha(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

/// An OCI layout of one image with `USER runner`, as a tar.
fn layout(layer: &[u8]) -> Option<Vec<u8>> {
    let lh = sha(layer);
    let config = serde_json::json!({
        "architecture": "amd64", "os": "linux",
        "config": {"User": "runner", "Env": ["PATH=/usr/bin:/bin"]},
        "rootfs": {"type": "layers", "diff_ids": [format!("sha256:{lh}")]},
    })
    .to_string();
    let ch = sha(config.as_bytes());
    let manifest = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {"mediaType": "application/vnd.oci.image.config.v1+json",
                   "digest": format!("sha256:{ch}"), "size": config.len()},
        "layers": [{"mediaType": "application/vnd.oci.image.layer.v1.tar",
                    "digest": format!("sha256:{lh}"), "size": layer.len()}],
    })
    .to_string();
    let mh = sha(manifest.as_bytes());
    let index = serde_json::json!({
        "schemaVersion": 2,
        "manifests": [{"mediaType": "application/vnd.oci.image.manifest.v1+json",
                       "digest": format!("sha256:{mh}"), "size": manifest.len(),
                       "annotations": {"org.opencontainers.image.ref.name": "localhost/t1927:1"}}],
    })
    .to_string();
    let mut b = tar::Builder::new(Vec::new());
    add(
        &mut b,
        "oci-layout",
        0o644,
        br#"{"imageLayoutVersion":"1.0.0"}"#,
    )?;
    add(&mut b, "index.json", 0o644, index.as_bytes())?;
    add(
        &mut b,
        &format!("blobs/sha256/{ch}"),
        0o644,
        config.as_bytes(),
    )?;
    add(&mut b, &format!("blobs/sha256/{lh}"), 0o644, layer)?;
    add(
        &mut b,
        &format!("blobs/sha256/{mh}"),
        0o644,
        manifest.as_bytes(),
    )?;
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

/// The first field of the payload's `/proc/self/uid_map` in a detached run.
fn inside_uid(store: &Path, name: &str, user: &[&str]) -> String {
    let mut args = vec!["run", "-d", "--name", name];
    args.extend_from_slice(user);
    args.extend_from_slice(&["localhost/t1927:1", "cat", "/proc/self/uid_map"]);
    let r = podvm(store, &args);
    assert!(r.status.success(), "run -d {user:?}: {}", text(&r.stderr));
    let end = Instant::now() + Duration::from_secs(30);
    let mut log = String::new();
    while Instant::now() < end && log.is_empty() {
        std::thread::sleep(Duration::from_millis(200));
        log = text(&podvm(store, &["logs", name]).stdout);
    }
    let _ = podvm(store, &["rm", "-f", name]);
    log.split_whitespace().next().unwrap_or("").to_string()
}

#[test]
fn run_detached_runs_as_the_user_of_the_image_in_the_user_namespace() {
    if !cfg!(target_arch = "x86_64") {
        return;
    }
    if std::fs::metadata("/proc/self").map(|_| ()).is_ok()
        && std::fs::read_to_string("/proc/self/status")
            .unwrap_or_default()
            .lines()
            .any(|l| l.starts_with("Uid:") && l.split_whitespace().nth(1) == Some("0"))
    {
        eprintln!("T-1927: a root caller keeps its ids (podcrates@T-787); skipped");
        return;
    }
    let Some(tar) = layer().and_then(|l| layout(&l)) else {
        eprintln!("T-1927: this host has no glibc at the x86_64 paths; skipped");
        return;
    };
    let dir = std::env::temp_dir().join(format!("podvm-t1927-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("img.tar"), tar).unwrap();
    let store = dir.join("store");
    let tar = dir.join("img.tar");
    let l = podvm(&store, &["load", "-i", tar.to_str().unwrap()]);
    assert!(l.status.success(), "load: {}", text(&l.stderr));

    let probe = podvm(
        &store,
        &[
            "run",
            "-d",
            "--name",
            "t1927-probe",
            "--user",
            "0",
            "localhost/t1927:1",
            "cat",
            "/etc/passwd",
        ],
    );
    let rung = text(&podvm(&store, &["inspect", "t1927-probe"]).stdout);
    let _ = podvm(&store, &["rm", "-f", "t1927-probe"]);
    if !probe.status.success() || !rung.contains("\"Rung\":\"namespace\"") {
        eprintln!("T-1927: run -d does not enter the namespace rung here; skipped");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    assert_eq!(
        inside_uid(&store, "t1927-image", &[]),
        "1001",
        "the image's USER"
    );
    assert_eq!(
        inside_uid(&store, "t1927-root", &["--user", "0"]),
        "0",
        "--user 0"
    );
    assert_eq!(
        inside_uid(&store, "t1927-flag", &["--user", "1001"]),
        "1001",
        "--user 1001"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
