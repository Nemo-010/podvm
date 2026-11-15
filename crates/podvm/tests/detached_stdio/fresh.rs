//! The stores of `detached_stdio.rs` (TODO/gate.md T-1859).

use super::NEXT;
use std::path::{Path, PathBuf};

/// A fresh store directory, unique across the parallel tests in this file.
pub(crate) fn fresh_store(tag: &str) -> Fresh {
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("podvm-{}-{}-{n}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)
        .unwrap_or_else(|e| panic!("could not create store dir {}: {e}", dir.display()));
    Fresh(dir)
}

/// TODO/gate.md T-1859: a store that goes at the drop, also after a
/// panic. A failed test left its store of about 1.3 GB in the temp
/// directory.
pub(crate) struct Fresh(PathBuf);

impl std::ops::Deref for Fresh {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Fresh {
    fn drop(&mut self) {
        remove_tree(&self.0);
    }
}

pub(crate) fn cleanup(store: &Path) {
    remove_tree(store);
}

/// TODO/gate.md T-1859: remove a test tree as a user who is not root. The
/// extraction keeps the mode of a directory of the image (0555 and the
/// like, podcrates `v0.44.0`), and such a directory refuses the removal of
/// its entries; each directory gets mode 0700 first. A second try after a
/// pause covers a launcher that writes its record at the end of a payload.
pub(crate) fn remove_tree(root: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    for _ in 0..2 {
        let mut stack = vec![root.to_path_buf()];
        while let Some(p) = stack.pop() {
            let Ok(m) = std::fs::symlink_metadata(&p) else {
                continue;
            };
            if m.is_dir() {
                let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700));
                if let Ok(rd) = std::fs::read_dir(&p) {
                    stack.extend(rd.flatten().map(|e| e.path()));
                }
            }
        }
        let _ = std::fs::remove_dir_all(root);
        if !root.exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}
