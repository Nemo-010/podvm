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
