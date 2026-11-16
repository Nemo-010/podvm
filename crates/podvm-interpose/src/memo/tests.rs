use super::*;

#[test]
fn a_decimal_fd_parses_and_anything_else_does_not() {
    assert_eq!(parse_fd(b"17"), Some(17));
    assert_eq!(parse_fd(b"0"), Some(0));
    assert_eq!(parse_fd(b""), None);
    assert_eq!(parse_fd(b"nobody"), None);
    assert_eq!(parse_fd(b"17a"), None);
    assert_eq!(parse_fd(b"-1"), None);
    assert_eq!(parse_fd(b"4294967296"), None);
}

#[test]
fn the_record_is_32_bytes_with_dev_ino_uid_gid_set() {
    assert_eq!(RECORD, 32);
    assert_eq!(SET_UID, 1);
    assert_eq!(SET_GID, 2);
}

/// T-1524 (`105` check G). Past the scan ceiling a stale prefix record
/// must refuse, never answer: the file below holds an old owner for
/// (7, 9) before the ceiling, and the lookup must return
/// `BeyondCeiling` rather than that stale hit. The control runs in the
/// SAME test, because the descriptor travels in the process environ and
/// two tests sharing it race: inside the ceiling the last match in the
/// prefix wins, so a re-chown is what a stat reports.
#[test]
fn a_stale_record_before_the_ceiling_is_a_refusal_not_an_answer() {
    use std::io::Write;
    fn record(dev: u64, ino: u64, uid: u32, gid: u32) -> [u8; RECORD] {
        let mut r = [0u8; RECORD];
        r[0..8].copy_from_slice(&dev.to_le_bytes());
        r[8..16].copy_from_slice(&ino.to_le_bytes());
        r[16..20].copy_from_slice(&uid.to_le_bytes());
        r[20..24].copy_from_slice(&gid.to_le_bytes());
        r[24..28].copy_from_slice(&SET_UID.to_le_bytes());
        r
    }
    let path = std::env::temp_dir().join(format!("podvm-memo-g-{}", std::process::id()));
    // ⛔ Read-write, like the handed descriptor production reads: a
    // `File::create` handle is write-only and every lookup read fails
    // `EBADF`, which reads as an empty memo rather than as an error.
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .unwrap();
    f.write_all(&record(7, 9, 0, 42)).unwrap();
    f.set_len((131208 * RECORD) as u64).unwrap();
    hand(&f);
    let got = lookup(7, 9);
    take_back();
    drop(f);
    assert_eq!(got, Lookup::BeyondCeiling);
    let _ = std::fs::remove_file(&path);

    let small = std::env::temp_dir().join(format!("podvm-memo-hit-{}", std::process::id()));
    let mut g = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&small)
        .unwrap();
    g.write_all(&record(7, 9, 0, 42)).unwrap();
    g.write_all(&record(7, 9, 0, 43)).unwrap();
    hand(&g);
    let got = lookup(7, 9);
    take_back();
    drop(g);
    assert_eq!(
        got,
        Lookup::Hit(Owner {
            uid: 0,
            gid: 43,
            set: SET_UID
        })
    );
    let _ = std::fs::remove_file(&small);
}

/// Hand `f` as the memo, as the host does: its number and its identity.
fn hand(f: &std::fs::File) {
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::io::AsRawFd;
    let m = f.metadata().unwrap();
    // SAFETY: only a test changes these variables; edition 2024 marks the change unsafe because another thread can read the environment at the same time.
    unsafe {
        std::env::set_var("PODVM_MEMO_FD", f.as_raw_fd().to_string());
        std::env::set_var("PODVM_MEMO_ID", format!("{}:{}", m.dev(), m.ino()));
    }
}

fn take_back() {
    // SAFETY: as in `hand`.
    unsafe {
        std::env::remove_var("PODVM_MEMO_FD");
        std::env::remove_var("PODVM_MEMO_ID");
    }
}

/// T-1928 (PodUtils/podrunner#197): a payload closed the memo's
/// descriptor and got the number again for a file of its own. The memo
/// is gone for it: no lookup moves that file's offset, and no record
/// writes into it. The same file handed with its identity is the memo.
#[test]
fn a_descriptor_that_is_no_longer_the_memo_is_left_alone() {
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::os::unix::io::AsRawFd;
    let dir = std::env::temp_dir().join(format!("podvm-memo-id-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let open = |name: &str| {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(dir.join(name))
            .unwrap()
    };
    let memo = open("memo");
    let mut other = open("payload");
    other.write_all(b"the payload's own bytes").unwrap();
    other.seek(SeekFrom::Start(0)).unwrap();
    hand(&memo);
    // The number now names the payload's file; the identity is the memo's.
    // SAFETY: as in `hand`.
    unsafe { std::env::set_var("PODVM_MEMO_FD", other.as_raw_fd().to_string()) };
    let looked = lookup(1, 2);
    let wrote = record(
        1,
        2,
        Owner {
            uid: 0,
            gid: 42,
            set: SET_GID,
        },
    );
    let fd = memo_fd();
    take_back();
    assert_eq!((looked, wrote, fd), (Lookup::Miss, false, None));
    let mut got = String::new();
    other.read_to_string(&mut got).unwrap();
    assert_eq!(got, "the payload's own bytes", "offset and bytes untouched");
    // A number with no identity is no memo either.
    // SAFETY: as in `hand`.
    unsafe { std::env::set_var("PODVM_MEMO_FD", memo.as_raw_fd().to_string()) };
    let bare = memo_fd();
    take_back();
    assert_eq!(bare, None);
    // The memo with its identity records and reads back.
    hand(&memo);
    let wrote = record(
        1,
        2,
        Owner {
            uid: 0,
            gid: 42,
            set: SET_GID,
        },
    );
    let looked = lookup(1, 2);
    take_back();
    assert!(wrote);
    assert_eq!(
        looked,
        Lookup::Hit(Owner {
            uid: 0,
            gid: 42,
            set: SET_GID
        })
    );
    let _ = std::fs::remove_dir_all(&dir);
}
