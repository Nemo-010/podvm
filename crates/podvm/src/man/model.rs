//! TODO/cli.md T-1917: the manual of podvm as the model of `pod-manual`
//! (podcrates@T-700, `v0.54.0`), filled from podvm's own sources and from
//! no second text: the parity table gives the commands, their flags and
//! their status against docker; each verb's `--help` gives its usage;
//! `names::ALIASES` gives the names of the program by argv0.

use pod_manual::{Command, Flag, FlagKind, Manual, Parity, Status};

use crate::parity::{self, Row};

/// The status of a row as `pod-manual` says it.
pub fn status(s: parity::Status) -> Status {
    match s {
        parity::Status::Native => Status::Implemented,
        parity::Status::Degraded => Status::Degraded,
        parity::Status::Stub => Status::Stub,
        parity::Status::None => Status::Refused,
    }
}

fn parity_of(row: &Row) -> Parity {
    Parity {
        tool: "docker".to_string(),
        status: status(row.status),
        text: row.note.to_string(),
    }
}

/// The flags of one row: `-a, --all` is one flag with two spellings; a row
/// that names several long flags (`--stack, --nat`) gives one flag each.
pub fn flags_of(row: &Row) -> Vec<Flag> {
    let Some(spec) = row.flag else {
        return Vec::new();
    };
    let words: Vec<&str> = spec.split(", ").map(str::trim).collect();
    let short = words
        .iter()
        .find_map(|w| {
            w.strip_prefix('-')
                .filter(|s| !s.starts_with('-') && s.chars().count() == 1)
        })
        .and_then(|s| s.chars().next());
    let longs: Vec<&str> = words.iter().filter_map(|w| w.strip_prefix("--")).collect();
    let kind = if row.status == parity::Status::None {
        FlagKind::Refused {
            instead: row.note.to_string(),
        }
    } else {
        FlagKind::Wired
    };
    let one = |long: Option<&str>, short: Option<char>| Flag {
        short,
        long: long.map(str::to_string),
        help: super::summary(row.note).to_string(),
        kind: kind.clone(),
        parity: Some(parity_of(row)),
        ..Flag::default()
    };
    match longs.as_slice() {
        [] => short.map(|c| vec![one(None, Some(c))]).unwrap_or_default(),
        [l] => vec![one(Some(l), short)],
        many => many.iter().map(|l| one(Some(l), None)).collect(),
    }
}

/// The note after its first sentence (the summary), where it has more.
fn rest_of(note: &str) -> Option<String> {
    note.find(". ")
        .map(|i| note[i + 2..].trim().to_string())
        .filter(|r| !r.is_empty())
}

/// The model: each verb that the table implements, with its flags, its
/// status and the bytes of its `--help` as its usage.
pub fn manual(help: &dyn Fn(&str) -> Option<String>) -> Manual {
    let mut commands = Vec::new();
    for row in super::implemented() {
        let path: Vec<String> = super::help_argv(row.verb)
            .iter()
            .flat_map(|s| s.split_whitespace())
            .map(str::to_string)
            .collect();
        let flags = parity::TABLE
            .iter()
            .filter(|r| r.verb == row.verb && r.flag.is_some())
            .flat_map(flags_of)
            .collect();
        commands.push(Command {
            path,
            about: super::summary(row.note).to_string(),
            notes: rest_of(row.note).into_iter().collect(),
            usage: help(row.verb).unwrap_or_default(),
            flags,
            parity: Some(parity_of(row)),
            ..Command::default()
        });
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Manual {
        name: "podvm".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        date: pod_manual::model::date_of(now),
        about: "docker and podman commands on hosts that deny namespaces, mounts or chroot"
            .to_string(),
        names: crate::names::ALIASES
            .iter()
            .map(|s| s.to_string())
            .collect(),
        commands,
        ..Manual::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_of_two_spellings_is_one_flag_and_a_row_of_longs_is_one_each() {
        let r = Row {
            verb: "x",
            flag: Some("-a, --all"),
            status: parity::Status::Stub,
            note: "Stub. More.",
        };
        let f = flags_of(&r);
        assert_eq!(f.len(), 1);
        assert_eq!((f[0].short, f[0].long.as_deref()), (Some('a'), Some("all")));
        assert_eq!(f[0].parity.as_ref().map(|p| p.status), Some(Status::Stub));
        let r = Row {
            verb: "x",
            flag: Some("--stack, --nat"),
            status: parity::Status::Native,
            note: "n",
        };
        assert_eq!(flags_of(&r).len(), 2);
        let r = Row {
            verb: "x",
            flag: Some("--no"),
            status: parity::Status::None,
            note: "use --yes",
        };
        assert!(matches!(flags_of(&r)[0].kind, FlagKind::Refused { .. }));
    }

    #[test]
    fn the_model_passes_the_structural_check_of_pod_manual() {
        let m = manual(&|v: &str| Some(format!("usage: podvm {v}\n")));
        pod_manual::check::assert_none(pod_manual::check::model(&m));
    }

    #[test]
    fn each_implemented_verb_is_a_command_of_the_model() {
        let m = manual(&|v: &str| Some(format!("usage: podvm {v}\n")));
        assert_eq!(m.commands.len(), super::super::implemented().len());
        assert!(m
            .commands
            .iter()
            .all(|c| c.usage.starts_with("usage: podvm ")));
        assert!(m.names.contains(&"docker".to_string()));
    }
}
