//! Slow tests (full solves at several stacks): the heads-up push/fold solution
//! against the published Nash tables, and its exploitability. They live in
//! their own test binary so the fast ones can run without them:
//! `cargo test -p nitro-solver --test convergence` runs only these.
//!
//! Oracle: HoldemResources "HeadsUp Push/Fold Nash Equilibrium" chart, no ante
//! (https://www.holdemresources.net/hune, retrieved 2026-10-07), cross-checked
//! with HRC's raw 0.05 BB equilibrium data and with pokerpro.tools'
//! independent solve. A chart cell is the largest effective stack (BB, before
//! blinds) at which the hand is pushed or called; `20+` means always up to
//! 20 BB, `*` marks the three hands whose push range has gaps (63s, 53s, 43s).

use nitro_solver::{Action, HandClass, Node, SolveOptions, Spot, solve};

const PUSH_CHART: &str = "
   20+   20+   20+   20+   20+   20+   20+   20+   20+   20+   20+   20+   20+
   20+   20+   20+   20+   20+   20+   20+   20+   20+   20+   20+  19.9  19.3
   20+   20+   20+   20+   20+   20+   20+   20+   20+   20+  16.3  13.5  12.7
   20+   20+   20+   20+   20+   20+   20+   20+  18.6  14.7  13.5  10.6   8.5
   20+   20+   20+   20+   20+   20+   20+   20+   20+  11.9  10.5   7.7   6.5
   20+   20+   20+   20+   20+   20+   20+   20+   20+  14.4   6.9   4.9   3.7
   20+  18.0  13.0  13.3  17.5   20+   20+   20+   20+  18.8  10.1   2.7   2.5
   20+  16.1  10.3   8.5   9.0  10.8  14.7   20+   20+   20+  13.9   2.5   2.1
   20+  15.1   9.6   6.5   5.7   5.2   7.0  10.7   20+   20+  16.3     *   2.0
   20+  14.2   8.9   6.0   4.1   3.5   3.0   2.6   2.4   20+   20+     *   2.0
   20+  13.1   7.9   5.4   3.8   2.7   2.3   2.1   2.0   2.1   20+     *   1.8
   20+  12.2   7.5   5.0   3.4   2.5   1.9   1.8   1.7   1.8   1.6   20+   1.7
   20+  11.6   7.0   4.6   2.9   2.2   1.8   1.6   1.5   1.5   1.4   1.4   20+
";

const CALL_CHART: &str = "
   20+   20+   20+   20+   20+   20+   20+   20+   20+   20+   20+   20+   20+
   20+   20+   20+   20+   20+   20+  17.6  15.2  14.3  13.2  12.1  11.4  10.7
   20+   20+   20+   20+   20+  16.1  13.0  10.5   9.9   8.9   8.4   7.8   7.2
   20+   20+  19.5   20+  18.0  13.4  10.6   8.8   7.0   6.9   6.1   5.8   5.6
   20+   20+  15.3  12.7   20+  11.5   9.3   7.4   6.3   5.2   5.2   4.8   4.5
   20+  17.1  11.7   9.5   8.4   20+   8.2   7.0   5.8   5.0   4.3   4.1   3.9
   20+  13.8   9.7   7.6   6.6   6.0   20+   6.5   5.6   4.8   4.1   3.6   3.5
   20+  12.4   8.0   6.4   5.5   5.0   4.7   20+   5.4   4.8   4.1   3.6   3.3
   20+  11.0   7.3   5.4   4.6   4.2   4.1   4.0   20+   4.9   4.3   3.8   3.3
   20+  10.2   6.8   5.1   4.0   3.7   3.6   3.6   3.7   20+   4.6   4.0   3.6
  18.3   9.1   6.2   4.7   3.8   3.3   3.2   3.2   3.3   3.5   20+   3.8   3.4
  16.6   8.7   5.9   4.5   3.6   3.1   2.9   2.9   2.9   3.1   3.0   20+   3.3
  15.8   8.1   5.6   4.2   3.5   3.0   2.8   2.6   2.7   2.8   2.7   2.6  15.0
";

/// Hands not asserted at a stack: those within 0.5 BB of their chart
/// threshold, those the chart simplifies (gaps), and those whose raw HRC
/// frequency is mixed or not constant within 0.5 BB of the stack. Near their
/// threshold, hands are close to indifferent and a correct solver may play
/// them either way.
const NOT_ASSERTED: [(f64, &str, &str); 5] = [
    (
        5.0,
        "93s 96o 63s 53s J4o 43s J3o J2o T6o",
        "T5s T4s T3s T2s 95s 85s T7o 97o 87o 76s 75s J6o T6o 65s J5o 54s J4o J3o",
    ),
    (
        8.0,
        "J2s T3s J7o 63s 53s Q4o 43s Q3o 84s T7o",
        "Q4s Q3s T9o 98s J8o Q7o K2o",
    ),
    (
        10.0,
        "T4s 84s Q7o Q6o 63s 53s 43s T7o",
        "Q7s Q6s J9o Q8o K5o",
    ),
    (
        12.0,
        "T5s 63s 53s 43s K3o K2o J8o 74s",
        "K4s T9s Q9o K7o JTo",
    ),
    (15.0, "J5s 87o K6o 63s 53s 43s Q4s", "K7s QTo 22"),
];

/// Share of the 1326 combos pushed and called at equilibrium according to
/// HRC's raw data at exactly this stack (mixed hands counted by frequency).
/// The raw frequencies have two decimals, hence the half-point tolerance.
const RANGE_SHARES: [(f64, f64, f64); 5] = [
    (5.0, 0.7149, 0.6207),
    (8.0, 0.6186, 0.4497),
    (10.0, 0.5829, 0.3738),
    (12.0, 0.5324, 0.3303),
    (15.0, 0.4566, 0.2855),
];

/// Chart threshold of each hand, `None` for a gap hand.
fn chart(text: &str) -> Vec<(HandClass, Option<f64>)> {
    text.split_whitespace()
        .enumerate()
        .map(|(i, cell)| {
            let threshold = match cell {
                "*" => None,
                "20+" => Some(f64::INFINITY),
                v => Some(v.parse().unwrap()),
            };
            (HandClass::at_grid(i / 13, i % 13), threshold)
        })
        .collect()
}

fn assert_matches_chart(stack: f64, node: Node, action: Action, chart_text: &str, skip: &str) {
    let solution = solve(
        &Spot::heads_up(stack, stack).unwrap(),
        &SolveOptions::default(),
    );
    let skip: Vec<HandClass> = skip
        .split_whitespace()
        .map(|h| h.parse().unwrap())
        .collect();
    let mut wrong = Vec::new();
    let mut asserted = 0;
    for (hand, threshold) in chart(chart_text) {
        if skip.contains(&hand) {
            continue;
        }
        let threshold = threshold.expect("gap hands are never asserted");
        let expected = if stack <= threshold { 1.0 } else { 0.0 };
        let freq = solution.strategy(node, hand).unwrap().frequency(action);
        asserted += 1;
        if (freq - expected).abs() > 0.05 {
            wrong.push(format!("{hand} {action} {freq:.3} (chart {threshold})"));
        }
    }
    assert!(asserted >= 150, "only {asserted} hands asserted");
    assert!(wrong.is_empty(), "{stack} BB, {node}: {wrong:?}");
}

#[test]
fn sb_push_range_matches_the_nash_chart() {
    for (stack, push_skip, _) in NOT_ASSERTED {
        assert_matches_chart(stack, Node::SbOpen, Action::Push, PUSH_CHART, push_skip);
    }
}

#[test]
fn bb_call_range_matches_the_nash_chart() {
    for (stack, _, call_skip) in NOT_ASSERTED {
        assert_matches_chart(stack, Node::BbVsSbPush, Action::Call, CALL_CHART, call_skip);
    }
}

#[test]
fn push_and_call_range_sizes_match_the_raw_nash_data() {
    for (stack, push, call) in RANGE_SHARES {
        let solution = solve(
            &Spot::heads_up(stack, stack).unwrap(),
            &SolveOptions::default(),
        );
        let push_share = solution.action_share(Node::SbOpen, Action::Push).unwrap();
        let call_share = solution
            .action_share(Node::BbVsSbPush, Action::Call)
            .unwrap();
        assert!(
            (push_share - push).abs() < 0.005,
            "{stack} BB push {push_share:.4} vs {push}"
        );
        assert!(
            (call_share - call).abs() < 0.005,
            "{stack} BB call {call_share:.4} vs {call}"
        );
    }
}

#[test]
fn exploitability_falls_below_a_tenth_of_a_milli_big_blind_per_hand() {
    for stack in [3.0, 10.0, 20.0] {
        let solution = solve(
            &Spot::heads_up(stack, stack).unwrap(),
            &SolveOptions::default(),
        );
        let exploitability = solution.exploitability();
        assert!(exploitability.total() >= 0.0);
        assert!(
            exploitability.total() < 1e-4,
            "{stack} BB: {exploitability:?}"
        );
    }
}
