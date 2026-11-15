use super::*;

#[test]
fn the_list_takes_the_order_of_the_table_and_refuses_an_unknown_name() {
    assert_eq!(
        parse("userns-off,bind-denied").unwrap(),
        vec!["bind-denied", "userns-off"]
    );
    assert!(parse("bind-denied,nope")
        .unwrap_err()
        .contains("nope is not a profile"));
    assert!(is_profile("small-tmp,noexec-tmp"));
    assert!(!is_profile("dualstack"));
}

#[test]
fn each_profile_of_the_filter_has_its_rules_and_the_program_compiles() {
    assert_eq!(denials(&["bind-denied"]).len(), 4);
    assert_eq!(denials(&["getsockname-denied"]).len(), 2);
    assert_eq!(denials(&["userns-off"]).len(), 3);
    assert!(denials(&["small-tmp", "inherited-fds"]).is_empty());
    let all = denials(&["bind-denied", "getsockname-denied", "userns-off"]);
    assert!(pod_confine::deny::program(&all).is_ok());
}

#[test]
fn a_size_of_mountinfo_is_bytes() {
    assert_eq!(probe::kib("1048576k"), Some(TMP_BYTES));
    assert_eq!(probe::kib("1g"), Some(TMP_BYTES));
    assert_eq!(probe::kib("12x"), None);
}

#[test]
fn each_profile_is_listed() {
    let t = profiles_text();
    for p in PROFILES {
        assert!(t.contains(p.name), "{}", p.name);
    }
}
