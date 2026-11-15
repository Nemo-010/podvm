//! The bus of the root disk of a Windows install (TODO/machine.md T-1867).

/// TODO/machine.md T-1867: `PODVM_WINDOWS_ROOT_BUS=sata` puts the root disk
/// of the install on the SATA (AHCI) bus of q35, not on NVMe, to tell a
/// stall of the emulated NVMe disk from one of the setup. Run 43726557269
/// stalled in the apply of the WIM at 5%, with no error in Panther.
/// ⛔ A diagnosis only: Windows installed on SATA does not boot from NVMe.
pub(super) fn root_bus(argv: Vec<String>) -> Vec<String> {
    if std::env::var("PODVM_WINDOWS_ROOT_BUS").as_deref() != Ok("sata") {
        return argv;
    }
    on_sata(argv)
}

/// The argv with the NVMe device of the root on port 2 of the AHCI bus.
fn on_sata(argv: Vec<String>) -> Vec<String> {
    argv.into_iter()
        .map(|a| match a.strip_prefix("nvme,drive=root,") {
            Some(rest) => {
                let keep: Vec<&str> = rest
                    .split(',')
                    .filter(|o| !o.starts_with("serial="))
                    .collect();
                let mut d = "ide-hd,drive=root,bus=ide.2".to_string();
                for o in keep {
                    d.push(',');
                    d.push_str(o);
                }
                d
            }
            None => a,
        })
        .collect()
}

#[cfg(test)]
#[test]
fn the_root_moves_to_sata_with_its_boot_index() {
    let a = vec![
        "-device".to_string(),
        "nvme,drive=root,serial=wqroot,bootindex=1".to_string(),
    ];
    assert_eq!(on_sata(a)[1], "ide-hd,drive=root,bus=ide.2,bootindex=1");
    let b = vec!["nvme,drive=mailbox,serial=x".to_string()];
    assert_eq!(on_sata(b.clone()), b);
}
