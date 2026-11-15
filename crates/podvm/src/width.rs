//! TODO/supervise.md T-1858: the display width of text on a terminal, so
//! that a table pads and cuts by columns, not by characters. An East Asian
//! Wide or Fullwidth character takes two columns, a combining mark and a
//! zero-width character none, and each other printable character one. A
//! small table is enough for the tables of podvm; it is not a full UAX #11.

/// The ranges of two columns (East Asian Wide and Fullwidth, and the
/// emoji blocks), as inclusive pairs.
const WIDE: &[(u32, u32)] = &[
    (0x1100, 0x115F),
    (0x2E80, 0x303E),
    (0x3041, 0x33FF),
    (0x3400, 0x4DBF),
    (0x4E00, 0x9FFF),
    (0xA000, 0xA4CF),
    (0xAC00, 0xD7A3),
    (0xF900, 0xFAFF),
    (0xFE30, 0xFE4F),
    (0xFF00, 0xFF60),
    (0xFFE0, 0xFFE6),
    (0x1F300, 0x1F64F),
    (0x1F900, 0x1F9FF),
    (0x20000, 0x2FFFD),
    (0x30000, 0x3FFFD),
];

/// The ranges of no column: combining marks and zero-width characters.
const ZERO: &[(u32, u32)] = &[
    (0x0300, 0x036F),
    (0x1AB0, 0x1AFF),
    (0x1DC0, 0x1DFF),
    (0x200B, 0x200F),
    (0x20D0, 0x20FF),
    (0xFE00, 0xFE0F),
    (0xFE20, 0xFE2F),
];

fn within(c: u32, table: &[(u32, u32)]) -> bool {
    table.iter().any(|&(a, b)| (a..=b).contains(&c))
}

/// The columns of one character.
pub fn char_width(c: char) -> usize {
    let u = c as u32;
    if u < 0x20 || (0x7F..0xA0).contains(&u) || within(u, ZERO) {
        0
    } else if within(u, WIDE) {
        2
    } else {
        1
    }
}

/// The columns of a string.
pub fn str_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

/// `s` cut to at most `max` columns: where it is wider, the first
/// characters that fit in `max - 3` columns and `...`. A cut never falls
/// inside a character.
pub fn cut(s: &str, max: usize) -> String {
    if str_width(s) <= max {
        return s.to_string();
    }
    let room = max.saturating_sub(3);
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = char_width(c);
        if used + w > room {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_characters_take_two_columns_and_marks_none() {
        assert_eq!(str_width("echo"), 4);
        assert_eq!(str_width("\u{65e5}\u{65e5}"), 4);
        assert_eq!(str_width("e\u{301}"), 1);
        assert_eq!(str_width("\u{1F600}"), 2);
    }

    #[test]
    fn a_cut_counts_columns() {
        assert_eq!(
            cut("echo \u{65e5}\u{65e5} \u{65e5}\u{65e5}\u{65e5}", 20),
            "echo \u{65e5}\u{65e5} \u{65e5}\u{65e5}\u{65e5}"
        );
        let c = cut(
            "echo \u{65e5}\u{65e5}\u{65e5}\u{65e5}\u{65e5}\u{65e5}\u{65e5}\u{65e5}\u{65e5}\u{65e5}",
            20,
        );
        assert!(str_width(&c) <= 20, "{c}");
        assert!(c.ends_with("..."), "{c}");
        assert_eq!(
            cut("abcdefghijklmnopqrstuvwxyz", 20),
            "abcdefghijklmnopq..."
        );
    }

    /// A `ps` row with U+65E5 and a row with ASCII put the CREATED column
    /// at the same display column.
    #[test]
    fn display_width_aligns_the_ps_columns() {
        let rows = vec![
            vec!["ID".into(), "COMMAND".into(), "CREATED".into()],
            vec![
                "a".into(),
                "echo \u{65e5}\u{65e5} \u{65e5}\u{65e5}\u{65e5}".into(),
                "T1".into(),
            ],
            vec!["b".into(), "echo plain".into(), "T2".into()],
        ];
        let out = crate::images::table(&rows);
        let at = |line: &str, cell: &str| str_width(&line[..line.find(cell).unwrap()]);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(at(lines[1], "T1"), at(lines[2], "T2"), "{out}");
        assert_eq!(at(lines[0], "CREATED"), at(lines[2], "T2"), "{out}");
    }
}
