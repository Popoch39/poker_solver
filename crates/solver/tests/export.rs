//! CSV export of a solution.

use nitro_solver::{HandClass, Node, SolveOptions, Spot, solve};

#[test]
fn csv_has_one_row_per_node_hand_and_action() {
    let solution = solve(
        &Spot::three_max(10.0, 10.0, 10.0).unwrap(),
        &SolveOptions::default(),
    );
    let mut csv = Vec::new();
    solution.write_csv(&mut csv).unwrap();
    let csv = String::from_utf8(csv).unwrap();
    let rows: Vec<&str> = csv.lines().collect();

    assert_eq!(rows[0], "node,position,hand,action,frequency");
    assert_eq!(rows.len(), 1 + 6 * 169 * 2);
    assert!(rows.contains(&"btn-open,BTN,AA,push,1.000000"));
    assert!(rows.contains(&"btn-open,BTN,72o,fold,1.000000"));
    assert!(rows.contains(&"bb-vs-btn-push-sb-call,BB,AA,call,1.000000"));

    // Each row agrees with the solution's own strategy.
    for row in &rows[1..] {
        let [node, _, hand, action, frequency] = row.split(',').collect::<Vec<_>>()[..] else {
            panic!("bad row {row}");
        };
        let node: Node = node.parse().unwrap();
        let hand: HandClass = hand.parse().unwrap();
        let strategy = solution.strategy(node, hand).unwrap();
        let (_, expected) = strategy
            .iter()
            .find(|(a, _)| a.to_string() == action)
            .unwrap();
        let frequency: f64 = frequency.parse().unwrap();
        assert!((frequency - expected).abs() < 1e-6, "{row}");
    }
}

#[test]
fn csv_has_the_limp_and_min_raise_nodes_and_actions() {
    let spot = Spot::heads_up(12.0, 12.0)
        .unwrap()
        .with_limp(true)
        .with_min_raise(true);
    let solution = solve(&spot, &SolveOptions::default());
    let mut csv = Vec::new();
    solution.write_csv(&mut csv).unwrap();
    let csv = String::from_utf8(csv).unwrap();

    let actions: usize = solution
        .nodes()
        .iter()
        .map(|&n| solution.actions(n).unwrap().len())
        .sum();
    assert_eq!(csv.lines().count(), 1 + 169 * actions);
    for prefix in [
        "sb-open,SB,AA,limp,",
        "sb-open,SB,AA,raise,",
        "bb-vs-sb-limp,BB,72o,check,",
        "bb-vs-sb-raise,BB,KK,call,",
        "sb-vs-sb-limp-bb-raise,SB,QQ,push,",
    ] {
        assert!(csv.lines().any(|row| row.starts_with(prefix)), "{prefix}");
    }
}
