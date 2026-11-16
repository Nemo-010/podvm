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
    let v: Vec<String> = inner(&rest, &[3128], false, &cmd)
        .iter()
        .map(|o| o.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        v,
        [
            "/proc/self/exe",
            "lab",
            "run",
            "--allow-connect",
            "3128",
            "getsockname-denied",
            "--",
            "sh",
            "-c",
            "true"
        ]
    );
    assert_eq!(inner(&[], &[], true, &cmd).len(), 3);
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

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_flags_of_run_come_before_the_list() {
    let a = strings(&[
        "--way-out",
        "--allow-connect",
        "3128,8080",
        "proxy-only",
        "--",
        "true",
    ]);
    let (f, rest) = run_flags(&a).unwrap();
    assert_eq!(
        f,
        RunFlags {
            allow_connect: vec![3128, 8080],
            way_out: true
        }
    );
    assert_eq!(rest, &a[3..]);
    let a = strings(&["--allow-connect=3128", "bind-denied", "--", "--way-out"]);
    let (f, rest) = run_flags(&a).unwrap();
    assert!(!f.way_out, "a --way-out after the list belongs to CMD");
    assert_eq!(rest.len(), 3);
    assert!(run_flags(&strings(&["--allow-connect", "0", "x"])).is_err());
    assert!(run_flags(&strings(&["--allow-connect", "http", "x"])).is_err());
}

#[test]
fn an_inner_lab_with_the_proxy_gets_the_way_out() {
    let rest = parse("proxy-only").unwrap();
    let cmd = vec!["true".to_string()];
    let v: Vec<String> = inner(&rest, &[], true, &cmd)
        .iter()
        .map(|o| o.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        v,
        [
            "/proc/self/exe",
            "lab",
            "run",
            "--way-out",
            "proxy-only",
            "--",
            "true"
        ]
    );
    assert!(pod_lab::has_proxy(rest[0]));
    assert!(!pod_lab::has_proxy(parse("bind-denied").unwrap()[0]));
}

#[test]
fn a_way_out_with_no_stand_in_proxy_is_a_flag_error() {
    let a = strings(&["--way-out", "bind-denied", "--", "true"]);
    assert_eq!(run(&a), EXIT_FLAG_ERROR);
}
