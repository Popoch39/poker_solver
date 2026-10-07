//! Parsing a whole folder of Winamax files in one call.

use std::fs;
use std::path::{Path, PathBuf};

use nitro_hh::ohh::write_hand;
use nitro_hh::{ErrorKind, Hand, parse_hands, parse_path, to_ohh};

const HANDS: &str = include_str!("fixtures/nitro-618031930.txt");
const SUMMARY: &str = include_str!("fixtures/nitro-618031930_summary.txt");

/// A fresh, empty directory under Cargo's per-test scratch directory.
fn scratch_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_folder_parses_into_tournaments_with_their_summaries() {
    let dir = scratch_dir("folder_with_one_tournament");
    fs::write(dir.join("hands.txt"), HANDS).unwrap();
    fs::write(dir.join("hands_summary.txt"), SUMMARY).unwrap();

    let batch = parse_path(&dir);

    assert!(batch.errors.is_empty(), "{:?}", batch.errors);
    assert_eq!(batch.files, 2);
    assert_eq!(batch.tournaments.len(), 1);
    let tournament = &batch.tournaments[0];
    assert_eq!(tournament.id, "618031930");
    assert_eq!(tournament.hands.len(), 3);
    assert_eq!(tournament.summary.as_ref().unwrap().place, 2);
}

#[test]
fn ohh_files_are_read_back_as_hands() {
    let dir = scratch_dir("folder_with_ohh");
    let hands: Vec<Hand> = parse_hands(HANDS).into_iter().map(Result::unwrap).collect();
    let mut file = Vec::new();
    for hand in &hands {
        write_hand(&mut file, to_ohh(hand, None)).unwrap();
    }
    fs::write(dir.join("simulated.ohh"), file).unwrap();
    fs::write(dir.join("broken.ohh"), "{\"ohh\": {}}\n\n").unwrap();

    let batch = parse_path(&dir);

    assert_eq!(batch.files, 2);
    assert_eq!(batch.tournaments.len(), 1);
    assert_eq!(batch.tournaments[0].id, "618031930");
    assert_eq!(batch.hands().count(), 3);
    assert_eq!(batch.errors.len(), 1, "{:?}", batch.errors);
    assert!(batch.errors[0].path.ends_with("broken.ohh"));
}

#[test]
fn bad_hands_and_other_formats_are_reported_without_stopping_the_batch() {
    let dir = scratch_dir("folder_with_errors");
    fs::create_dir(dir.join("nested")).unwrap();
    // A second Nitro whose second hand has a damaged line.
    let damaged = HANDS
        .replace("618031930", "618031931")
        .replace("villain 1 raises 80 to 160", "villain 1 raises 80 to");
    fs::write(dir.join("nested/damaged.txt"), &damaged).unwrap();
    fs::write(dir.join("hands.txt"), HANDS).unwrap();
    // Another tournament format, and a file that is not a hand history.
    let monster_stack = HANDS.replace("\"Expresso Nitro\"", "\"Monster Stack\"");
    fs::write(dir.join("monster.txt"), &monster_stack).unwrap();
    let monster_summary = SUMMARY.replace(
        "Expresso Nitro(618031930)",
        "Monster Stack(623541204) - Late Registration",
    );
    fs::write(dir.join("monster_summary.txt"), &monster_summary).unwrap();
    fs::write(dir.join("LICENSE.txt"), "ISC License\n").unwrap();
    // Not a `.txt` file: not read at all.
    fs::write(dir.join("notes.csv"), "not,a,hand\n").unwrap();

    let batch = parse_path(&dir);

    assert_eq!(batch.files, 5);
    let hands: Vec<(&str, usize)> = batch
        .tournaments
        .iter()
        .map(|t| (t.id.as_str(), t.hands.len()))
        .collect();
    assert_eq!(hands, vec![("618031930", 3), ("618031931", 2)]);
    assert!(batch.tournaments.iter().all(|t| t.summary.is_none()));

    let malformed: Vec<_> = batch
        .errors
        .iter()
        .filter(|e| e.error.kind == ErrorKind::Malformed)
        .collect();
    assert_eq!(malformed.len(), 1, "{:?}", batch.errors);
    assert!(malformed[0].path.ends_with("nested/damaged.txt"));
    assert_eq!(malformed[0].error.line, 43);

    let unsupported = batch
        .errors
        .iter()
        .filter(|e| e.error.kind == ErrorKind::Unsupported)
        .count();
    assert_eq!(
        unsupported, 5,
        "one per Monster Stack hand, its summary, the licence"
    );
}
