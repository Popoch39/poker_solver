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
fn solve_rejects_a_spot_without_two_players() {
    let output = nitro(&["solve", "--stacks", "0,10"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("positive number of BB"));
    let output = nitro(&["solve", "--stacks", "10,10,10,10"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("two or three stacks"));
}

#[test]
fn solve_prints_every_three_max_range_and_each_players_exploitability() {
    let out = stdout(&nitro(&["solve", "--stacks", "10,8,12"]));
    assert!(
        out.contains("3-max push/fold, stacks BTN 10 BB / SB 8 BB / BB 12 BB"),
        "{out}"
    );
    let exploitability = out.lines().find(|l| l.contains("exploitability")).unwrap();
    for player in ["BTN ", "SB ", "BB "] {
        assert!(exploitability.contains(player), "{exploitability}");
    }
    for heading in [
        "BTN open: push",
        "SB vs BTN push: call",
        "SB open: push",
        "BB vs BTN push: call",
        "BB vs BTN push and SB call: call",
        "BB vs SB push: call",
    ] {
        assert!(out.contains(heading), "{heading}\n{out}");
    }
    let header = "     A   K   Q   J   T   9   8   7   6   5   4   3   2";
    assert_eq!(out.matches(header).count(), 6, "{out}");
}

#[test]
fn solve_answers_for_one_hand_at_one_three_max_node() {
    let out = stdout(&nitro(&[
        "solve",
        "--stacks",
        "10,10,10",
        "--node",
        "bb-vs-btn-push-sb-call",
        "--hand",
        "AA",
    ]));
    assert_eq!(
        out.trim_end(),
        "AA at BB vs BTN push and SB call: fold 0.0%, call 100.0%"
    );
}

#[test]
fn solve_stops_at_the_iteration_count_when_the_target_is_zero() {
    let out = stdout(&nitro(&[
        "solve",
        "--stacks",
        "10,10,10",
        "--iterations",
        "12",
        "--target",
        "0",
    ]));
    assert!(out.contains("12 iterations, exploitability"), "{out}");
    let out = stdout(&nitro(&["solve", "--stacks", "10,10,10", "--target", "5"]));
    let ran: u32 = out
        .split(" iterations")
        .next()
        .and_then(|head| head.rsplit('\n').next())
        .unwrap()
        .parse()
        .unwrap();
    assert!(ran < 100, "{out}");
}

#[test]
fn solve_with_limp_and_min_raise_prints_a_grid_per_action_and_the_realization_factors() {
    let out = stdout(&nitro(&[
        "solve",
        "--stacks",
        "15,15,15",
        "--limp",
        "--min-raise",
        "--iterations",
        "20",
    ]));
    assert!(
        out.contains("3-max push/fold + limp + min-raise, stacks BTN 15 BB / SB 15 BB / BB 15 BB"),
        "{out}"
    );
    assert!(
        out.contains(
            "realization factors: heads-up OOP 0.9 / IP 1.1, three-way SB 0.9 / BB 1 / BTN 1.1"
        ),
        "{out}"
    );
    for heading in [
        "BTN open: limp",
        "BTN open: raise",
        "BTN open: push",
        "BB vs BTN limp and SB limp: raise",
        "BB vs BTN limp and SB limp: push",
        "BTN vs BTN limp and SB push: call",
    ] {
        assert!(out.contains(heading), "{heading}\n{out}");
    }
    // The passive action (fold, check) has no grid of its own.
    assert!(!out.contains("BTN open: fold"), "{out}");
    assert!(!out.contains(": check"), "{out}");
}

#[test]
fn solve_takes_realization_factors_per_spot_type() {
    let out = stdout(&nitro(&[
        "solve",
        "--stacks",
        "12,12",
        "--limp",
        "--realization-oop",
        "0.8",
        "--realization-ip",
        "1.2",
        "--realization-3way",
        "0.85,1,1.15",
        "--node",
        "bb-vs-sb-limp",
        "--hand",
        "AA",
    ]));
    assert!(out.starts_with("AA at BB vs SB limp: check "), "{out}");
    assert!(out.contains("%, push "), "{out}");

    let output = nitro(&[
        "solve",
        "--stacks",
        "12,12",
        "--limp",
        "--realization-ip",
        "-1",
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("realization factor"));
    let output = nitro(&["solve", "--stacks", "12,12", "--realization-3way", "1,1"]);
    assert!(!output.status.success());
}

#[test]
fn solve_prints_the_ranges_of_one_node() {
    let out = stdout(&nitro(&[
        "solve", "--stacks", "12,12", "--limp", "--node", "sb-open",
    ]));
    assert!(out.contains("exploitability"), "{out}");
    assert!(out.contains("SB open: limp"), "{out}");
    assert!(out.contains("SB open: push"), "{out}");
    assert!(!out.contains("BB vs SB limp"), "{out}");
    let header = "     A   K   Q   J   T   9   8   7   6   5   4   3   2";
    assert_eq!(out.matches(header).count(), 2, "{out}");
}

#[test]
fn solve_exports_the_solution_as_csv() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("solve-csv");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("solution.csv");
    let out = stdout(&nitro(&[
        "solve",
        "--stacks",
        "10,10,10",
        "--csv",
        path.to_str().unwrap(),
    ]));
    assert!(out.contains("exploitability"), "{out}");
    let csv = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(
        csv.starts_with("node,position,hand,action,frequency\n"),
        "{csv}"
    );
    assert_eq!(csv.lines().count(), 1 + 6 * 169 * 2);
}
