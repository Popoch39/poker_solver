//! Smoke tests of the `nitro solve` command.

use std::process::{Command, Output};

fn nitro(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nitro"))
        .args(args)
        .output()
        .expect("the nitro binary runs")
}

fn stdout(output: &Output) -> String {
    assert!(
        output.status.success(),
        "nitro failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}

#[test]
fn solve_prints_both_ranges_as_grids_and_the_exploitability() {
    let out = stdout(&nitro(&["solve", "--stacks", "10,10"]));
    assert!(out.contains("exploitability"), "{out}");
    assert!(out.contains("SB open: push"), "{out}");
    assert!(out.contains("BB vs SB push: call"), "{out}");
    // Header row of each 13×13 grid.
    let header = "     A   K   Q   J   T   9   8   7   6   5   4   3   2";
    assert_eq!(out.matches(header).count(), 2, "{out}");
    // AA is always played; 72o is never pushed at 10 BB.
    let aa_row = out.lines().find(|l| l.starts_with("  A ")).unwrap();
    assert!(aa_row.starts_with("  A 100"), "{aa_row}");
    let seven_row = out.lines().find(|l| l.starts_with("  7 ")).unwrap();
    assert!(seven_row.ends_with("."), "{seven_row}");
}

#[test]
fn solve_answers_for_one_hand_at_one_node() {
    let out = stdout(&nitro(&[
        "solve", "--stacks", "9,9", "--node", "sb-open", "--hand", "K7o",
    ]));
    assert_eq!(out.lines().count(), 1, "{out}");
    assert_eq!(out.trim_end(), "K7o at SB open: fold 0.0%, push 100.0%");
}

#[test]
fn solve_rejects_a_stack_below_one_big_blind() {
    let output = nitro(&["solve", "--stacks", "0.5,10"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("at least 1 BB"));
}
