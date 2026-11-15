//! TODO/machine.md T-1890: the pinned kernel of an arm64 guest. The netboot
//! archive of Alpine 3.22.6 for aarch64 (`alpine-netboot-aarch64` of
//! `pod-services`, COORD-128, podcrates `v0.51.0`) is fetched once, its
//! kernel and its modloop are checked against the facts of the service,
//! and the kernel and the 9p modules stay in the store. The archive is
//! about 340 MB; it is read as a stream and not kept.
//!
//! The guest still has no podvm binary of its own architecture (T-1890):
//! the 9p modules wait in the store for it, and the NIC modules of the
//! aarch64 kernel have no pin yet.

use std::path::Path;

use podvm_image::net::Net;

use super::kernel::{self, Assets, LIMITS, MAX_ARCHIVE, MEMBER, MODLOOP};

fn service() -> &'static pod_services::Service {
    pod_services::get("alpine-netboot-aarch64")
        .expect("pod-services holds alpine-netboot-aarch64 (podcrates v0.51.0)")
}

fn fact(name: &str) -> &'static str {
    service()
        .text(name)
        .unwrap_or_else(|| panic!("alpine-netboot-aarch64 has the fact {name}"))
}

/// The URL of the aarch64 netboot archive.
pub fn url() -> String {
    service()
        .endpoint("archive")
        .expect("alpine-netboot-aarch64 has the endpoint archive")
        .url()
}

/// The first 16 hex digits of a SHA-256, which name a file of the store.
fn short(sha: &str) -> &str {
    sha.get(..16).unwrap_or(sha)
}

fn cached(dir: &Path) -> Option<Assets> {
    let k = fact("vmlinuz-virt-sha256");
    let m = fact("modloop-virt-sha256");
    let kernel = dir.join(format!("vmlinuz-{}", short(k)));
    let volume_modules: Vec<_> = super::vol::MODULES
        .iter()
        .map(|n| (*n, dir.join(format!("{n}-{}.ko", short(m)))))
        .collect();
    (kernel.is_file() && volume_modules.iter().all(|(_, p)| p.is_file())).then_some(Assets {
        kernel,
        modules: Vec::new(),
        volume_modules,
    })
}

fn check(bytes: &[u8], want: &str, what: &str) -> Result<(), String> {
    let got = kernel::sha256_hex(bytes);
    if got == want {
        Ok(())
    } else {
        Err(format!(
            "{}: {what} has SHA-256 {got}, not the pin {want}",
            url()
        ))
    }
}

/// The kernel and the 9p modules of an arm64 guest, from the store, or
/// fetched where they are not there yet.
pub fn ensure(store_root: &Path) -> Result<Assets, String> {
    let dir = store_root.join("machine").join("arm64");
    if let Some(a) = cached(&dir) {
        return Ok(a);
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    crate::assets::space_floor(&dir, "the kernel of an arm64 guest", 64 << 20)?;
    let (config, _roots) = podvm_image::tls::client_config();
    let mut net = Net::from_environment(config)?;
    let url = url();
    let body = net
        .send("GET", &url, &[], true, LIMITS)
        .map_err(|e| format!("{url}: {e}"))?;
    let gz = flate2::read::GzDecoder::new(std::io::Read::take(body, MAX_ARCHIVE));
    let mut got = kernel::tar_members(gz, &[MEMBER, MODLOOP]).map_err(|e| format!("{url}: {e}"))?;
    let modloop = got.pop().flatten().ok_or(format!("{url}: no {MODLOOP}"))?;
    let kern = got.pop().flatten().ok_or(format!("{url}: no {MEMBER}"))?;
    let (k, m) = (fact("vmlinuz-virt-sha256"), fact("modloop-virt-sha256"));
    check(&kern, k, MEMBER)?;
    check(&modloop, m, MODLOOP)?;
    let put = |name: String, bytes: &[u8]| {
        let path = dir.join(name);
        let tmp = path.with_extension("partial");
        std::fs::write(&tmp, bytes)
            .and_then(|()| std::fs::rename(&tmp, &path))
            .map_err(|e| format!("{}: {e}", path.display()))
    };
    for (n, bytes) in kernel::volume_modules(&modloop)? {
        put(format!("{n}-{}.ko", short(m)), &bytes)?;
    }
    put(format!("vmlinuz-{}", short(k)), &kern)?;
    cached(&dir).ok_or_else(|| format!("{}: the assets did not stay", dir.display()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_arm64_pins_come_from_pod_services() {
        assert!(super::url().ends_with("alpine-netboot-3.22.6-aarch64.tar.gz"));
        assert_eq!(super::fact("vmlinuz-virt-sha256").len(), 64);
        assert_eq!(super::fact("modloop-virt-sha256").len(), 64);
    }
}
