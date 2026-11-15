use super::*;

/// Sentences that the owner's rulings retired. Each stays out of the
/// product source: a message that says it misleads the user.
///
/// TODO/machine.md T-1879: the ruling "Windows media" (`docs/decisions.md`,
/// 2026-11-14) makes podvm fetch the FreeDOS base and the Windows media,
/// so no message may say that podvm fetches no base.
const RETIRED: [(&str, &str); 1] = [(
    "No base is fetched",
    "podvm fetches the base of each catalog name (`docs/decisions.md`, \
     \"Windows media\"; TODO/machine.md T-1879)",
)];

/// TODO/machine.md T-1879: no Rust source of `crates/` holds a retired
/// sentence.
pub(super) fn check_retired_sentences(gate: &mut Gate, root: &Path, files: &BTreeSet<String>) {
    for rel in files {
        if !rel.starts_with("crates/")
            || !rel.ends_with(".rs")
            || rel.starts_with("crates/podvm-gate/")
        {
            continue;
        }
        gate.bump("retired_sentences");
        let Ok(text) = read_text(&root.join(rel)) else {
            continue;
        };
        for (sentence, why) in RETIRED {
            if text.contains(sentence) {
                gate.err(
                    rel,
                    format!("says \"{sentence}\", a retired sentence: {why}"),
                );
            }
        }
    }
}

/// TODO/gate.md T-1857: a sentence that ties a text to a future entry
/// ("until T-NNNN", "arrives with T-NNNN", "waits for T-NNNN") fails once
/// that entry is done. Rust sources of `crates/` and documents of `docs/`;
/// `docs/history/` and `docs/decisions.md` keep dated text, and a test
/// file may name the sentence it forbids.
pub(super) fn check_future_promises(
    gate: &mut Gate,
    root: &Path,
    files: &BTreeSet<String>,
    rows: &BTreeMap<String, IndexRow>,
) {
    const KEYS: [&str; 3] = ["until ", "arrives with ", "waits for "];
    for rel in files {
        let source = rel.starts_with("crates/")
            && rel.ends_with(".rs")
            && !rel.starts_with("crates/podvm-gate/")
            && !rel.ends_with("tests.rs")
            && !rel.contains("/tests/");
        let doc = rel.starts_with("docs/")
            && rel.ends_with(".md")
            && !rel.starts_with("docs/history/")
            && rel != "docs/decisions.md";
        if !source && !doc {
            continue;
        }
        gate.bump("future_promises");
        let Ok(text) = read_text(&root.join(rel)) else {
            continue;
        };
        for (n, line) in text.lines().enumerate() {
            let low = line.to_ascii_lowercase();
            for key in KEYS {
                let mut from = 0;
                while let Some(at) = low[from..].find(key) {
                    let rest = &line[from + at + key.len()..];
                    from += at + key.len();
                    let Some(id) = promised_entry(rest) else {
                        continue;
                    };
                    if rows.get(&id).is_some_and(|r| r.status == "done") {
                        gate.err(
                            &format!("{rel}:{}", n + 1),
                            format!(
                                "says \"{}{id}\", and {id} is done: the text must say what \
                                 the binary does now (TODO/gate.md T-1857)",
                                key
                            ),
                        );
                    }
                }
            }
        }
    }
}

/// The entry right after a key: `T-NNNN`, or a file of the record and
/// then `T-NNNN` (`TODO/cli.md T-1761`, `` `TODO/machine.md` T-1656 ``).
fn promised_entry(rest: &str) -> Option<String> {
    let mut words = rest.split_whitespace();
    let mut w = words.next()?.trim_matches('`');
    if w.starts_with("TODO/") {
        w = words.next()?;
    }
    let id: String = w.chars().take(6).collect();
    (id.len() == 6 && id.starts_with("T-") && id[2..].bytes().all(|b| b.is_ascii_digit()))
        .then_some(id)
}
