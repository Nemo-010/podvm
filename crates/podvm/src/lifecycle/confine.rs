//! TODO/enter.md T-1658: the confine layer of `run --podvm-confine=SPEC`.
//!
//! The layer runs on a rung with a chroot (the namespace rung and the
//! chroot rung), where the paths of the spec are guest paths and
//! `pod-enter` opens each under the root before the fork, and on each
//! family of the userland rung (the loader family, the native-static rung,
//! the syscall and the emulate tier), where each guest path becomes its
//! host path through the table of the interposer (the rootfs and each
//! `-v`). The record keeps the SPEC, so the launcher of `run -d` and each
//! `exec` apply it too. Each other case refuses by name, before anything
//! runs: a payload that asked for the layer never runs without it.

use podvm_enter::confine::{self, Ask, Spec};
use podvm_probe::select::Rung;

fn refuse(verb: &str, why: String) -> i32 {
    eprintln!("podvm {verb}: --podvm-confine: {why} (TODO/enter.md T-1658)");
    podvm_image::error::EXIT_RUNTIME_ERROR
}

/// The spec of the run, or None without `--podvm-confine`.
pub fn decide(
    verb: &str,
    ask: Option<&Ask>,
    entered: Rung,
    rootfs: &str,
    userland: Option<&super::Userland>,
    env: &mut Vec<String>,
) -> Result<Option<Spec>, i32> {
    // TODO/enter.md T-1819, T-1892: a caller's own exec table is dropped;
    // `pod-enter` writes the table of the layer (host paths) at the spawn.
    env.retain(|e| !e.starts_with("PODVM_EXEC=") && !e.starts_with("PODVM_EXEC_HOST="));
    let Some(ask) = ask else {
        return Ok(None);
    };
    // Each family of the userland rung runs with no chroot, so the layer
    // takes host paths. Beside the image, the payload's process runs
    // podvm's own dispatcher (the native-static rung, the syscall tier) or
    // a host emulator (the emulate tier): each gets read and execute.
    if let Err(why) = check(ask, entered, userland.is_some(), confine::abi()) {
        return Err(refuse(verb, why));
    }
    let Some(u) = userland else {
        return Ok(Some(confine::spec_in(ask, std::path::Path::new(rootfs))));
    };
    // TODO/enter.md T-1819, T-1892: on the userland rung the kernel
    // executes the loader (or podvm's dispatcher, or the emulator), so
    // Landlock never checks EXECUTE on the program itself. The interposer
    // checks each program against the table of host paths that `pod-enter`
    // writes from this spec (`PODVM_EXEC_HOST`), the one table it reads.
    let rx = confine::Access::READ | confine::Access::EXECUTE;
    let mut spec = match confine::host_spec(ask, &u.maps) {
        Ok(s) => s,
        // TODO/enter.md T-1888: `create` keeps the SPEC in the record, and
        // `start` resolves the paths on the tree that it starts, which can
        // hold a path by then. `start` refuses where it still does not.
        Err(_) if verb == "create" => return Ok(Some(Spec::new())),
        Err(why) => return Err(refuse(verb, why)),
    };
    // TODO/enter.md T-1819: the loader family too: a static child enters
    // the native-static rung through podvm itself (T-1724), so podvm's own
    // binary gets read and execute on each family. A static program that
    // an `exec=` rule grants was denied (126) without it.
    if let Ok(me) = std::env::current_exe() {
        spec = spec.allow(me.to_string_lossy().to_string(), rx);
    }
    if let Some((_, host)) = u.emulator.as_deref().and_then(|e| e.split_once(':')) {
        spec = spec.allow(host.to_string(), rx);
    }
    for host in raw_reads(ask) {
        spec = spec.allow(host, confine::Access::READ);
    }
    Ok(Some(spec))
}

/// PodUtils/podvm#396 (TODO/enter.md T-1902): the files that musl opens
/// with its own `syscall` instruction, past each symbol of the interposer,
/// so the kernel resolves them on the host: `/etc/resolv.conf`,
/// `/etc/hosts` and `/etc/services` (`__fopen_rb_ca`). podvm writes the
/// same `resolv.conf` and `hosts` into the tree. Where the spec lets the
/// payload read the guest path, the host file (its link resolved) gets
/// read too; without it the resolver of musl found no name server, asked
/// 127.0.0.1, and each name failed with EAI_AGAIN.
fn raw_reads(ask: &Ask) -> Vec<String> {
    const RAW: [&str; 3] = ["/etc/resolv.conf", "/etc/hosts", "/etc/services"];
    RAW.iter()
        .filter(|g| guest_readable(ask, g))
        .filter_map(|g| std::fs::canonicalize(g).ok())
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

/// True where a rule of the spec covers the guest path `g` (each kind of
/// rule gives read).
fn guest_readable(ask: &Ask, g: &str) -> bool {
    ask.read.iter().chain(&ask.write).chain(&ask.exec).any(|p| {
        let p = p.trim_end_matches('/');
        p.is_empty() || g == p || g.strip_prefix(p).is_some_and(|r| r.starts_with('/'))
    })
}

/// The refusals of the layer, as a pure function of its inputs.
pub fn check(ask: &Ask, entered: Rung, userland: bool, abi: Option<u32>) -> Result<(), String> {
    let carried = confine::rung_carries(entered) || (entered == Rung::Userland && userland);
    if !carried {
        let which = if entered == Rung::Userland {
            "the userland rung with no table of host paths".to_string()
        } else {
            format!("the {} rung", entered.word())
        };
        return Err(format!(
            "the layer runs on a rung with a chroot (namespace or chroot) and on the \
             userland rung, and this run enters {which}, so podvm refuses: a payload \
             that asks for the layer never runs without it"
        ));
    }
    match abi {
        None => Err(
            "this kernel has no Landlock, so podvm refuses: a payload that asks for \
                     the layer never runs without it"
                .to_string(),
        ),
        Some(a) if a < confine::MIN_ABI => Err(format!(
            "this kernel has Landlock ABI {a}, and the layer needs ABI {} or later",
            confine::MIN_ABI
        )),
        Some(a) if ask.connect.is_some() && a < confine::NET_ABI => Err(format!(
            "`connect` needs Landlock ABI {} (TCP rules), and this kernel has ABI {a}",
            confine::NET_ABI
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_case_with_no_mechanism_refuses_by_name() {
        let ask = confine::parse("read=/workspace;connect=443").unwrap();
        assert!(check(&ask, Rung::Namespace, false, Some(7)).is_ok());
        assert!(check(&ask, Rung::Userland, true, Some(7)).is_ok());
        let e = check(&ask, Rung::Userland, false, Some(7)).unwrap_err();
        assert!(e.contains("no table of host paths"), "{e}");
        let e = check(&ask, Rung::Supervise, false, Some(7)).unwrap_err();
        assert!(e.contains("supervise rung"), "{e}");
        let e = check(&ask, Rung::Namespace, false, None).unwrap_err();
        assert!(e.contains("no Landlock"), "{e}");
        let e = check(&ask, Rung::Chroot, false, Some(3)).unwrap_err();
        assert!(e.contains("ABI 4"), "{e}");
        let no_tcp = confine::parse("read=/workspace").unwrap();
        assert!(check(&no_tcp, Rung::Chroot, false, Some(3)).is_ok());
        assert!(check(&no_tcp, Rung::Chroot, false, Some(2)).is_err());
    }

    /// TODO/enter.md T-1819, T-1892: `decide` drops a caller's own exec
    /// table, of either name, and writes none of its own: `pod-enter`
    /// writes the one table at the spawn.
    #[test]
    fn a_rule_over_etc_reads_the_raw_files_of_musl() {
        let ask = confine::parse("read=/;write=/tmp").unwrap();
        assert!(guest_readable(&ask, "/etc/resolv.conf"));
        let ask = confine::parse("read=/usr,/etcetera;write=/tmp").unwrap();
        assert!(!guest_readable(&ask, "/etc/resolv.conf"));
        let ask = confine::parse("read=/etc/;write=/tmp").unwrap();
        assert!(guest_readable(&ask, "/etc/hosts"));
    }

    #[test]
    fn exec_table_of_the_caller_is_dropped() {
        let mut env = vec![
            "PODVM_EXEC=/".to_string(),
            "PODVM_EXEC_HOST=/".to_string(),
            "A=1".to_string(),
        ];
        let none = decide("run", None, Rung::Namespace, "/r", None, &mut env).unwrap();
        assert!(none.is_none());
        assert_eq!(env, ["A=1"], "a caller's own table goes");
    }
}
