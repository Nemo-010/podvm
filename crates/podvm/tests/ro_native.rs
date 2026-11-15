//! TODO/enter.md T-1901 (PodUtils/podrunner#111): a `:ro` volume holds on
//! the native-static rung, where chroot is denied (CI): a write through it
//! fails with EROFS, a read works, and the host file stays. The payload is
//! this static test binary, whose helper test `ro_child` does the write
//! when `PODVM_T1901` names a path. Where the run takes another rung (the
//! chroot rung of a root user), the test says SKIP.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

struct Gone(PathBuf);

impl Drop for Gone {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn podvm(base: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_podvm"))
        .args(args)
        .current_dir(base)
        .env("HOME", base)
        .env("PODVM_STORE", base.join("store"))
        .env("PODVM_SPARE_TREE", "0")
        .stdin(Stdio::null())
        .output()
        .expect("podvm runs")
}

/// The helper of the payload: write to the path of `PODVM_T1901`, and print
/// the errno of the open, or `wrote`.
#[test]
fn ro_child() {
    let Ok(p) = std::env::var("PODVM_T1901") else {
        return;
    };
    match std::fs::OpenOptions::new().write(true).open(&p) {
        Ok(_) => println!("T1901 wrote"),
        Err(e) => println!("T1901 errno {}", e.raw_os_error().unwrap_or(-1)),
    }
    match std::fs::read_to_string(&p) {
        Ok(t) => println!("T1901 read {}", t.trim()),
        Err(e) => println!("T1901 read-errno {}", e.raw_os_error().unwrap_or(-1)),
    }
}

#[test]
fn a_ro_volume_refuses_a_write_on_the_native_static_rung() {
    let base = std::env::temp_dir().join(format!("podvm-t1901-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("r/bin")).unwrap();
    std::fs::create_dir_all(base.join("r/etc")).unwrap();
    std::fs::create_dir_all(base.join("vol")).unwrap();
    let _gone = Gone(base.clone());
    std::fs::write(base.join("r/etc/os-release"), "x\n").unwrap();
    std::fs::write(base.join("vol/f"), "orig\n").unwrap();
    std::fs::copy(std::env::current_exe().unwrap(), base.join("r/bin/t")).unwrap();
    let mut b = tar::Builder::new(Vec::new());
    b.append_dir_all("etc", base.join("r/etc")).unwrap();
    b.append_dir_all("bin", base.join("r/bin")).unwrap();
    std::fs::write(base.join("a.tar"), b.into_inner().unwrap()).unwrap();
    let imp = podvm(&base, &["import", "a.tar", "t1901:1"]);
    assert!(
        imp.status.success(),
        "{}",
        String::from_utf8_lossy(&imp.stderr)
    );
    let vol = format!("{}:/e:ro", base.join("vol").display());
    let o = podvm(
        &base,
        &[
            "run",
            "--rm",
            "--verbose",
            "-e",
            "PODVM_T1901=/e/f",
            "-v",
            &vol,
            "t1901:1",
            "/bin/t",
            "--exact",
            "ro_child",
            "--nocapture",
            "--test-threads=1",
        ],
    );
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    let err = String::from_utf8_lossy(&o.stderr).to_string();
    if !err.contains("native-static rung") {
        eprintln!("SKIP: the run took another rung than native-static: {err}");
        return;
    }
    assert!(o.status.success(), "{err}");
    assert!(out.contains("T1901 errno 30"), "no EROFS: {out}\n{err}");
    assert!(out.contains("T1901 read orig"), "the read failed: {out}");
    assert_eq!(
        std::fs::read_to_string(base.join("vol/f")).unwrap(),
        "orig\n"
    );
}
