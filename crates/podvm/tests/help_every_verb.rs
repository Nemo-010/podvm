//! TODO/cli.md T-1833: each verb of the parity table answers `-h` and
//! `--help` with exit 0 and its usage on stdout. The list comes from the
//! binary itself (`system info --format '{{json .Parity}}'`), so a new verb
//! is checked with no change here. `docker` and `podman` are names of the
//! binary (argv[0]), not verbs.

use std::process::Command;

fn podvm(args: &[&str], store: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_podvm"))
        .args(args)
        .env("PODVM_STORE", store)
        .env("HOME", store)
        .output()
        .unwrap()
}

#[test]
fn help_every_verb_exits_0_with_its_usage() {
    let store = std::env::temp_dir().join(format!("podvm-help-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&store);
    std::fs::create_dir_all(&store).unwrap();
    let out = podvm(&["system", "info", "--format", "{{json .Parity}}"], &store);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let rows: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    let mut verbs: Vec<String> = rows
        .iter()
        .filter(|r| r["status"] != "None")
        .filter_map(|r| r["verb"].as_str().map(str::to_string))
        .filter(|v| v != "docker" && v != "podman")
        .collect();
    verbs.sort();
    verbs.dedup();
    assert!(
        verbs.len() > 30,
        "the parity table lists {} verbs",
        verbs.len()
    );
    let mut bad = Vec::new();
    for v in &verbs {
        for h in ["-h", "--help"] {
            let mut args: Vec<&str> = v.split(' ').collect();
            args.push(h);
            let o = podvm(&args, &store);
            if o.status.code() != Some(0) || o.stdout.is_empty() {
                bad.push(format!("{v} {h}: {:?}", o.status.code()));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&store);
    assert!(bad.is_empty(), "verbs that refuse their help: {bad:?}");
}
