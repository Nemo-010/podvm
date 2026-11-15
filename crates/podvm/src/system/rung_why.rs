//! PodUtils/podvm#397 (TODO/probe.md T-1903): `probe` names the leg that
//! kept each stronger rung from the run, from the census rows it already
//! has. A host where an unprivileged user opens a user namespace is said
//! so: the namespace rung of pod-enter does not enter one yet (COORD-132,
//! PodUtils/podvm#406).

use podvm_probe::select::Rung;
use podvm_probe::verdict::{Outcome, Verdict};
use podvm_probe::Findings;

/// `ok`, the errno name, or `skip`.
fn word(o: Option<&Outcome>) -> String {
    match o {
        Some(o) if o.verdict == Verdict::Ok => "ok".to_string(),
        Some(o) => o.errno_name().unwrap_or_else(|| "skip".to_string()),
        None => "not probed".to_string(),
    }
}

fn ok(o: Option<&Outcome>) -> bool {
    o.is_some_and(|o| o.verdict == Verdict::Ok)
}

/// The lines that say why the run does not enter the namespace or the
/// chroot rung, where `entered` is a weaker one. Empty otherwise.
pub fn why_not(f: &Findings, entered: Rung) -> Vec<String> {
    if matches!(entered, Rung::Namespace | Rung::Chroot) {
        return Vec::new();
    }
    let mut v = Vec::new();
    let newns = f.get("unshare(CLONE_NEWNS)");
    if !ok(newns) {
        let mut l = format!(
            "the namespace rung needs unshare(CLONE_NEWNS) in the namespace of the caller, \
             and it gives {}",
            word(newns)
        );
        let user = f.get("unshare(CLONE_NEWUSER)");
        if ok(user) {
            l.push_str(
                "; a user namespace opens on this host (unshare(CLONE_NEWUSER) ok), and \
                 the namespace rung does not enter one yet (COORD-132, PodUtils/podvm#406)",
            );
        } else {
            l.push_str(&format!("; unshare(CLONE_NEWUSER) gives {}", word(user)));
        }
        v.push(l);
    }
    if !crate::ladder::ns_reachable(f) {
        let row = f.rows.iter().find(|(n, _)| n.starts_with("chroot("));
        v.push(format!(
            "the chroot rung needs chroot(2), and it gives {}",
            word(row.map(|(_, o)| o))
        ));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn findings(rows: Vec<(&'static str, Outcome)>) -> Findings {
        let mut f = Findings::empty();
        f.rows = rows;
        f
    }

    #[test]
    fn each_weaker_rung_names_the_leg_that_stopped_the_stronger_ones() {
        let eperm = || Outcome::denied(podvm_probe::sys::Errno(1));
        let f = findings(vec![
            ("unshare(CLONE_NEWNS)", eperm()),
            ("unshare(CLONE_NEWUSER)", Outcome::ok()),
            ("chroot(.)", eperm()),
        ]);
        let v = why_not(&f, Rung::Userland);
        assert_eq!(v.len(), 2, "{v:?}");
        assert!(v[0].contains("gives EPERM"), "{}", v[0]);
        assert!(v[0].contains("a user namespace opens"), "{}", v[0]);
        assert!(v[1].starts_with("the chroot rung needs chroot(2), and it gives EPERM"));
        assert!(why_not(&f, Rung::Namespace).is_empty());
        let f = findings(vec![
            ("unshare(CLONE_NEWNS)", eperm()),
            ("unshare(CLONE_NEWUSER)", eperm()),
        ]);
        assert!(why_not(&f, Rung::Supervise)[0].ends_with("unshare(CLONE_NEWUSER) gives EPERM"));
    }
}
