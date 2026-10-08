//! Spots with limps and min-raises through the solver's public interface.

use nitro_solver::{
    Action, HandClass, Node, Position, RealizationFactors, Solution, SolveOptions, Spot, solve,
};

fn node(id: &str) -> Node {
    id.parse().unwrap()
}

/// Largest gap between two solutions' frequencies, which must have the same
/// nodes (in any order) and actions.
fn largest_gap(a: &Solution, b: &Solution) -> f64 {
    let mut a_nodes = a.nodes().to_vec();
    let mut b_nodes = b.nodes().to_vec();
    a_nodes.sort_by_key(|n| n.id());
    b_nodes.sort_by_key(|n| n.id());
    assert_eq!(a_nodes, b_nodes);
    let mut gap: f64 = 0.0;
    for node in a_nodes {
        assert_eq!(a.actions(node), b.actions(node), "{node}");
        for hand in HandClass::all() {
            let (sa, sb) = (
                a.strategy(node, hand).unwrap(),
                b.strategy(node, hand).unwrap(),
            );
            for (action, f) in sa.iter() {
                gap = gap.max((f - sb.frequency(action)).abs());
            }
        }
    }
    gap
}

#[test]
fn heads_up_min_raise_can_be_called_or_pushed_over_and_every_node_id_reads_back() {
    let spot = Spot::heads_up(12.0, 12.0)
        .unwrap()
        .with_limp(true)
        .with_min_raise(true);
    // A deeper tree than push/fold takes more iterations.
    let options = SolveOptions {
        iterations: 3000,
        target_exploitability: Some(1e-4),
    };
    let solution = solve(&spot, &options);
    use Action::{Call, Check, Fold, Limp, Push, Raise};
    let expected: [(&str, &[Action]); 4] = [
        ("sb-open", &[Fold, Limp, Raise, Push]),
        ("bb-vs-sb-limp", &[Check, Raise, Push]),
        ("bb-vs-sb-raise", &[Fold, Call, Push]),
        ("sb-vs-sb-limp-bb-raise", &[Fold, Call, Push]),
    ];
    for (id, actions) in expected {
        assert_eq!(solution.actions(node(id)), Some(actions), "{id}");
    }
    assert_eq!(solution.nodes().len(), 8);
    for &n in solution.nodes() {
        assert_eq!(n.id().parse::<Node>(), Ok(n));
    }
    assert!(
        solution.exploitability().total() < 1e-4,
        "{:?}",
        solution.exploitability()
    );
}

#[test]
fn line_node_identifiers_name_the_actor_and_the_actions_before_it() {
    let n = node("btn-vs-btn-limp-sb-limp-bb-raise");
    assert_eq!(n.actor(), Position::Btn);
    assert_eq!(n.id(), "btn-vs-btn-limp-sb-limp-bb-raise");
    assert_eq!(n.to_string(), "BTN vs BTN limp, SB limp and BB raise");
    assert_eq!(node("BB-vs-SB-Limp").to_string(), "BB vs SB limp");
    // The push/fold nodes keep their names whatever the spot.
    assert_eq!(node("bb-vs-btn-push-sb-call"), Node::BbVsBtnPushSbCall);
    assert_eq!(node("sb-open"), Node::SbOpen);
    for bad in [
        "bb-vs",
        "bb-vs-btn",
        "bb-vs-btn-fold",
        "bb-vs-sb-check",
        "co-open",
        "bb-vs-btn-limp-sb",
        "btn-open-sb-limp",
    ] {
        assert!(bad.parse::<Node>().is_err(), "{bad}");
    }
}

#[test]
fn short_and_uneven_stacks_give_well_formed_trees() {
    let stacks = [0.0, 0.7, 1.0, 1.5, 2.0, 2.5, 9.0];
    let options = SolveOptions {
        iterations: 3,
        target_exploitability: None,
    };
    for btn in stacks {
        for sb in stacks {
            for bb in stacks {
                let Ok(spot) = Spot::three_max(btn, sb, bb) else {
                    continue;
                };
                let spot = spot.with_limp(true).with_min_raise(true);
                let solution = solve(&spot, &options);
                for &n in solution.nodes() {
                    assert_eq!(n.id().parse::<Node>(), Ok(n));
                    let actions = solution.actions(n).unwrap();
                    assert!(actions.len() >= 2, "{btn}/{sb}/{bb} {n}");
                    for hand in HandClass::all() {
                        let total: f64 = solution
                            .strategy(n, hand)
                            .unwrap()
                            .iter()
                            .map(|(_, f)| f)
                            .sum();
                        assert!((total - 1.0).abs() < 1e-9);
                    }
                }
                for (position, gain) in solution.exploitability().per_player() {
                    assert!(*gain >= -1e-9, "{btn}/{sb}/{bb} {position}: {gain}");
                }
            }
        }
    }
}

#[test]
fn with_limp_and_min_raise_disallowed_the_solution_is_push_fold() {
    let push_fold = Spot::three_max(12.0, 8.0, 10.0).unwrap();
    let disallowed = push_fold
        .clone()
        .with_limp(true)
        .with_min_raise(true)
        .with_limp(false)
        .with_min_raise(false);
    assert!(!disallowed.allows_limp() && !disallowed.allows_min_raise());
    let reference = solve(&push_fold, &SolveOptions::default());
    let solution = solve(&disallowed, &SolveOptions::default());
    assert_eq!(solution.nodes(), reference.nodes());
    assert_eq!(largest_gap(&solution, &reference), 0.0);
    assert_eq!(solution.exploitability(), reference.exploitability());
}

#[test]
fn with_one_big_blind_stacks_and_a_realization_of_one_hands_are_valued_at_raw_equity() {
    // Every chip a player adds puts it all-in, so no line reaches the flop:
    // the spot is push/fold at all-in equity whatever the actions allowed.
    let push_fold = Spot::three_max(1.0, 1.0, 1.0).unwrap();
    let reference = solve(&push_fold, &SolveOptions::default());
    for factors in [
        RealizationFactors::RAW_EQUITY,
        RealizationFactors::default(),
    ] {
        let spot = push_fold
            .clone()
            .with_limp(true)
            .with_min_raise(true)
            .with_realization(factors)
            .unwrap();
        let solution = solve(&spot, &SolveOptions::default());
        assert_eq!(largest_gap(&solution, &reference), 0.0);
    }
    // Pushing 0.5 BB more from the SB to win the 1.5 BB in the pot needs
    // 25% equity against the BB's hand, which every hand has.
    for hand in HandClass::all() {
        let push = reference.strategy(Node::SbOpen, hand).unwrap();
        assert!(push.frequency(Action::Push) > 0.99, "{hand}");
    }
}

#[test]
fn a_better_realization_out_of_position_makes_the_sb_limp_more() {
    let spot = Spot::three_max(15.0, 15.0, 15.0).unwrap().with_limp(true);
    let limps = |out_of_position: f64| {
        let factors = RealizationFactors {
            out_of_position,
            ..RealizationFactors::default()
        };
        // Coarse is enough to compare the two ranges.
        let options = SolveOptions {
            iterations: 300,
            target_exploitability: None,
        };
        let solution = solve(&spot.clone().with_realization(factors).unwrap(), &options);
        solution.action_share(Node::SbOpen, Action::Limp).unwrap()
    };
    // Blind against blind, the SB is out of position after the flop.
    let (poor, good) = (limps(0.7), limps(1.1));
    assert!(good > poor + 0.2, "limps {poor} at 0.7, {good} at 1.1");
}

#[test]
fn realization_factors_must_be_non_negative_numbers() {
    let spot = Spot::heads_up(10.0, 10.0).unwrap().with_limp(true);
    for bad in [-0.1, f64::NAN, f64::INFINITY] {
        let factors = RealizationFactors {
            in_position: bad,
            ..RealizationFactors::default()
        };
        assert!(spot.clone().with_realization(factors).is_err(), "{bad}");
    }
    assert_eq!(
        spot.with_realization(RealizationFactors::RAW_EQUITY)
            .unwrap()
            .realization(),
        &RealizationFactors::RAW_EQUITY
    );
}

#[test]
fn with_stacks_of_two_big_blinds_the_min_raise_is_the_push_and_the_solution_is_push_fold() {
    for (btn, sb, bb) in [(2.0, 2.0, 2.0), (1.6, 2.0, 1.3)] {
        let push_fold = Spot::three_max(btn, sb, bb).unwrap();
        let min_raise = push_fold.clone().with_min_raise(true);
        let reference = solve(&push_fold, &SolveOptions::default());
        let solution = solve(&min_raise, &SolveOptions::default());
        // Both engines solve the same game; only float rounding differs.
        let gap = largest_gap(&solution, &reference);
        assert!(gap < 1e-4, "{btn}/{sb}/{bb}: frequencies differ by {gap}");
        for position in min_raise.positions() {
            let (a, b) = (
                solution.exploitability().of(position),
                reference.exploitability().of(position),
            );
            assert!((a - b).abs() < 1e-9, "{position}: {a} vs {b}");
        }
    }
}

#[test]
fn heads_up_limp_tree_lets_the_sb_limp_and_the_bb_check_or_push() {
    let spot = Spot::heads_up(10.0, 10.0).unwrap().with_limp(true);
    let solution = solve(&spot, &SolveOptions::default());
    assert_eq!(
        solution.nodes(),
        &[
            Node::SbOpen,
            node("bb-vs-sb-limp"),
            Node::BbVsSbPush,
            node("sb-vs-sb-limp-bb-push"),
        ]
    );
    use Action::{Call, Check, Fold, Limp, Push};
    let expected: [(&str, &[Action]); 4] = [
        ("sb-open", &[Fold, Limp, Push]),
        ("bb-vs-sb-limp", &[Check, Push]),
        ("bb-vs-sb-push", &[Fold, Call]),
        ("sb-vs-sb-limp-bb-push", &[Fold, Call]),
    ];
    for (id, actions) in expected {
        assert_eq!(solution.actions(node(id)), Some(actions), "{id}");
        for hand in HandClass::all() {
            let strategy = solution.strategy(node(id), hand).unwrap();
            let total: f64 = strategy.iter().map(|(_, f)| f).sum();
            assert!((total - 1.0).abs() < 1e-9, "{id} {hand}");
        }
    }
    assert!(
        solution.exploitability().total() < 1e-4,
        "{:?}",
        solution.exploitability()
    );
}
