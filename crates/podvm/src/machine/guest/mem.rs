//! TODO/machine.md T-1922 (PodUtils/podrunner#199): the memory of a guest
//! for the size of its initrd.
//!
//! The guest holds the initrd and its unpacked copy in RAM at the same
//! time. Measured with experiment 463 (`kvm.yml`, run 43876504275): an
//! initrd of 1416 MiB resets the guest in its early boot at `-m 4G`, with
//! no line on the console, and boots at 5G, 6G and 8G. The rule below is
//! an estimate from that measurement: three times the initrd and 512 MiB.

use std::path::Path;

/// Below this size of initrd the default memory stays as it is.
const LARGE: u64 = 100 << 20;

/// The estimate of the memory that an initrd of `bytes` needs, rounded up
/// to 256 MiB.
pub(super) fn estimate(bytes: u64) -> u64 {
    let need = bytes.saturating_mul(3).saturating_add(512 << 20);
    need.div_ceil(256 << 20) * (256 << 20)
}

/// The memory of the guest: `asked`, or the default raised for a large
/// initrd. `warn` prints one line where `asked` is below the estimate.
pub(super) fn size(verb: &str, asked: Option<u64>, initrd: &Path, warn: bool) -> u64 {
    let bytes = std::fs::metadata(initrd).map(|m| m.len()).unwrap_or(0);
    let need = estimate(bytes);
    match asked {
        None if bytes > LARGE => need.max(super::DEFAULT_MEM),
        None => super::DEFAULT_MEM,
        Some(m) => {
            if warn && bytes > LARGE && m < need {
                eprintln!(
                    "podvm {verb}: machine tier: the initrd of {} MiB likely needs about {} MiB \
                     of memory, and --podvm-mem gives {} MiB: the guest can reset before the \
                     command (an estimate, TODO/machine.md T-1922)",
                    bytes.div_ceil(1 << 20),
                    need >> 20,
                    m >> 20
                );
            }
            m
        }
    }
}

#[cfg(test)]
mod tests {
    use super::estimate;

    /// The measured case: 1416 MiB failed at 4 GiB and booted at 5 GiB.
    #[test]
    fn the_estimate_lies_between_the_measured_failure_and_boot() {
        let e = estimate(1416 << 20);
        assert!(e > 4096 << 20 && e <= 5120 << 20, "{}", e >> 20);
        assert_eq!(estimate(0), 512 << 20);
    }
}
