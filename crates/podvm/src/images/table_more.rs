//! TODO/image.md T-1840: the parts of `images` and `inspect` that tell
//! records apart: the filter of a positional with no tag, the PLATFORM
//! column, and `.Config.Labels`.

use podvm_image::{Record, Reference, Store};

/// `podvm images REPOSITORY` with no tag and no digest lists each record
/// of that repository, as docker reads the argument; None for a
/// positional with a tag or a digest, which names records itself.
pub(super) fn repository_filter(
    store: &Store,
    want: &str,
) -> Option<podvm_image::Result<Vec<Record>>> {
    let last = want.rsplit('/').next().unwrap_or(want);
    if last.contains(':') || want.contains('@') {
        return None;
    }
    let repo = match Reference::parse(want) {
        Ok(r) => r.canonical_repository(),
        Err(e) => return Some(Err(e.into())),
    };
    Some(
        store
            .list()
            .map(|all| all.into_iter().filter(|r| r.repository == repo).collect()),
    )
}

/// True where one reference has more than one record (one for each
/// platform): the table then needs a PLATFORM column to tell the rows
/// apart.
pub(super) fn needs_platform(records: &[Record]) -> bool {
    records.iter().enumerate().any(|(i, r)| {
        records[..i]
            .iter()
            .any(|o| o.repository == r.repository && o.tag == r.tag)
    })
}

/// `.Config.Labels` of the image's config: an object, or null where the
/// config has none.
pub(super) fn labels_json(store: &Store, r: &Record) -> serde_json::Value {
    let labels = super::list::labels_of(store, r);
    if labels.is_empty() {
        return serde_json::Value::Null;
    }
    let map: serde_json::Map<String, serde_json::Value> = labels
        .iter()
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (k.to_string(), serde_json::Value::from(v))
        })
        .collect();
    serde_json::Value::Object(map)
}
