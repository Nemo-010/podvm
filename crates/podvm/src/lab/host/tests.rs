use super::*;

#[test]
fn the_list_keeps_its_order_and_refuses_an_unknown_name() {
    let p = parse("userns-off,bind-denied").unwrap();
    assert_eq!(
        p.iter().map(|p| p.name).collect::<Vec<_>>(),
        ["userns-off", "bind-denied"]
    );
    assert!(parse("bind-denied,nope")
        .unwrap_err()
        .contains("nope is not a profile"));
    assert!(is_profile("small-tmp,noexec-tmp"));
    assert!(!is_profile("dualstack"));
}

#[test]
fn a_list_runs_as_nested_labs() {
    let rest = parse("getsockname-denied").unwrap();
    let cmd = vec!["sh".to_string(), "-c".to_string(), "true".to_string()];
    let v: Vec<String> = inner(&rest, &cmd)
        .iter()
        .map(|o| o.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        v,
        [
            "/proc/self/exe",
            "lab",
            "run",
            "getsockname-denied",
            "--",
            "sh",
            "-c",
            "true"
        ]
    );
    assert_eq!(inner(&[], &cmd).len(), 3);
}

#[test]
fn each_profile_of_the_table_is_listed() {
    let t = profiles_text();
    for p in pod_lab::PROFILES {
        assert!(t.contains(p.name), "{}", p.name);
    }
}

#[test]
fn a_profile_of_a_tmpfs_is_outermost() {
    let p = nest_order(parse("userns-off,bind-denied,small-tmp").unwrap());
    assert_eq!(
        p.iter().map(|p| p.name).collect::<Vec<_>>(),
        ["small-tmp", "userns-off", "bind-denied"]
    );
}
