//! TODO/enter.md T-1927 (PodUtils/podrunner#196): the user of a detached
//! run on the namespace rung.
//!
//! pod-enter maps the caller to uid 0 of a one-uid namespace unless the
//! plan names other ids (`Plan::userns_ids`, podcrates `v0.62.0`). docker
//! runs the image's `USER`; so `run -d` and `create` on the namespace rung
//! record the ids of `--user`, else of the image's `USER`, resolved against
//! the tree, and the launcher hands them to the plan. Uid 0 records nothing.

use podvm_supervise::table::USERNS_IDS_VAR;

/// Record the ids for the launcher in `env`, where they apply.
pub(super) fn push(
    verb: &str,
    o: &super::Opts,
    image_user: &str,
    rootfs: &str,
    userland: Option<&crate::lifecycle::Userland>,
    findings: &podvm_probe::Findings,
    env: &mut Vec<String>,
) -> Result<(), i32> {
    env.retain(|e| !e.starts_with(&format!("{USERNS_IDS_VAR}=")));
    let rung = podvm_enter::entered_rung(podvm_probe::select::Selection::choose(findings).rung);
    if !(o.detach || verb == "create")
        || userland.is_some()
        || rung != podvm_probe::select::Rung::Namespace
    {
        return Ok(());
    }
    let image = (!image_user.is_empty()).then(|| image_user.to_string());
    let Some(spec) = o.user.as_ref().or(image.as_ref()) else {
        return Ok(());
    };
    let (uid, gid) = crate::lifecycle::resolve_user(verb, rootfs, spec)?;
    if uid != 0 {
        env.push(format!("{USERNS_IDS_VAR}={uid}:{gid}"));
    }
    Ok(())
}

/// Whether `env` records ids: the banner then does not say that the run
/// keeps uid 0.
pub(super) fn applied(env: &[String]) -> bool {
    podvm_supervise::table::userns_ids_of(env).is_some()
}
