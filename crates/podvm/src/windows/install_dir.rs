//! TODO/machine.md T-1867: the work directory of `podvm pull windows:NAME`
//! in the store. The root disk of an install grows to some 10 GiB, and the
//! runtime directory (`$XDG_RUNTIME_DIR`) is a small tmpfs on many hosts: on
//! a hosted runner it held 1.5 GiB, and each write of the guest failed after
//! it (runs 43689812413 to 44000335653).

use std::path::{Path, PathBuf};

/// The work directory of an install or a foreground run in the store,
/// removed at its end (`PODVM_WINDOWS_KEEP` keeps it): the root disk (or
/// the overlay of a run) and the sticks.
pub(super) struct WorkDir(pub(super) PathBuf);

impl WorkDir {
    /// The work directory of the install of `name`.
    pub(super) fn install(name: &str) -> std::io::Result<Self> {
        Self::make(
            &podvm_windows::catalog::store_dir(),
            &format!("install-{name}"),
        )
    }

    /// The work directory of a foreground run (TODO/machine.md T-1932).
    pub(super) fn run() -> std::io::Result<Self> {
        Self::make(&podvm_windows::catalog::store_dir(), "run")
    }

    fn make(store: &Path, what: &str) -> std::io::Result<Self> {
        use std::os::unix::fs::DirBuilderExt as _;
        let tag = format!(
            "{}{}-{what}",
            podvm_windows::SCRATCH_PREFIX,
            std::process::id()
        );
        let dir = store.join(tag);
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)?;
        Ok(WorkDir(dir))
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        if std::env::var_os("PODVM_WINDOWS_KEEP").is_none() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
