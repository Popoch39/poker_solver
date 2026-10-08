//! Smoke tests of the `nitro leaks` command.
//!
//! The fixture holds three hands of the account `hero`: a BTN min-raise,
//! then two heads-up hands, one where the opponent min-raises from the SB and
//! one where `hero` pushes J♠T♣ from the SB at 2–4 BB.

use std::path::PathBuf;
use std::process::{Command, Output};

fn nitro(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nitro"))
        .args(args)
        .output()
        .expect("the nitro binary runs")
}

fn fixtures() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../hh/tests/fixtures")
        .to_str()
        .unwrap()
        .to_owned()
}

#[test]
fn leaks_compares_each_node_to_the_equilibrium_without_naming_anyone() {
    let output = nitro(&["leaks", &fixtures(), "--hero", "hero"]);
    assert!(output.status.success());
    let out = String::from_utf8(output.stdout).unwrap();
    assert!(out.contains("3 hands read"), "{out}");
    let line = out
        .lines()
        .find(|l| l.starts_with("SB open, 2–4 BB:"))
        .unwrap_or_else(|| panic!("{out}"));
    assert!(line.contains("push 100.0% on 1"), "{line}");
    assert!(line.contains("at equilibrium (3 BB each)"), "{line}");
    assert!(line.contains("chips lost"), "{line}");
    assert!(line.ends_with("inconclusive"), "{line}");
    assert!(!out.contains("hero"), "{out}");
}

#[test]
fn leaks_reports_the_hands_it_could_not_read() {
    let damaged = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("leaks-damaged.txt");
    let hands = std::fs::read_to_string(PathBuf::from(fixtures()).join("nitro-618031930.txt"))
        .unwrap()
        .replace("618031930", "618031931")
        .replace("raises 80 to 160", "raises 80 to");
    std::fs::write(&damaged, hands).unwrap();

    let output = nitro(&[
        "leaks",
        &fixtures(),
        damaged.to_str().unwrap(),
        "--hero",
        "hero",
    ]);

    assert!(output.status.success());
    let out = String::from_utf8(output.stdout).unwrap();
    assert!(out.contains("5 hands read"), "{out}");
    assert!(out.contains("1 files or hands skipped"), "{out}");
}

#[test]
fn leaks_of_an_account_without_hands_is_an_error() {
    let output = nitro(&["leaks", &fixtures(), "--hero", "nobody"]);
    assert!(!output.status.success());
    let err = String::from_utf8(output.stderr).unwrap();
    assert!(err.contains("no hand"), "{err}");
}
