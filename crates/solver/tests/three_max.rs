//! 3-max push/fold (BTN, SB, BB) through the solver's public interface.

use nitro_solver::{
    Action, HandClass, Node, Position, Solution, SolveOptions, Spot, SpotError, solve,
};

#[test]
fn three_max_tree_has_every_push_fold_decision_in_play_order() {
    let solution = solve(
        &Spot::three_max(10.0, 10.0, 10.0).unwrap(),
        &SolveOptions::default(),
    );
    assert_eq!(
        solution.nodes(),
        &[
            Node::BtnOpen,
            Node::SbVsBtnPush,
            Node::SbOpen,
            Node::BbVsBtnPush,
            Node::BbVsBtnPushSbCall,
            Node::BbVsSbPush,
        ]
    );
    let expected = [
        (Node::BtnOpen, Position::Btn, Action::Push),
        (Node::SbVsBtnPush, Position::Sb, Action::Call),
        (Node::SbOpen, Position::Sb, Action::Push),
        (Node::BbVsBtnPush, Position::Bb, Action::Call),
        (Node::BbVsBtnPushSbCall, Position::Bb, Action::Call),
        (Node::BbVsSbPush, Position::Bb, Action::Call),
    ];
    for (node, actor, aggressive) in expected {
        assert_eq!(node.actor(), actor, "{node}");
        assert_eq!(node.actions(), &[Action::Fold, aggressive], "{node}");
        assert_eq!(node.id().parse::<Node>(), Ok(node));
    }
}

#[test]
fn an_eliminated_player_leaves_the_others_heads_up() {
    let heads_up = solve(
        &Spot::heads_up(9.0, 14.0).unwrap(),
        &SolveOptions::default(),
    );
    for (btn, sb, bb) in [(0.0, 9.0, 14.0), (9.0, 0.0, 14.0), (9.0, 14.0, 0.0)] {
        let spot = Spot::three_max(btn, sb, bb).unwrap();
        assert_eq!(spot.positions(), [Position::Sb, Position::Bb]);
        assert_eq!(spot.stack(Position::Sb), 9.0);
        assert_eq!(spot.stack(Position::Bb), 14.0);

        let solution = solve(&spot, &SolveOptions::default());
        assert_eq!(solution.nodes(), heads_up.nodes());
        for &node in heads_up.nodes() {
            for hand in HandClass::all() {
                assert_eq!(
                    solution.strategy(node, hand),
                    heads_up.strategy(node, hand),
                    "{node} {hand}"
                );
            }
        }
        assert_eq!(solution.exploitability(), heads_up.exploitability());
    }
}

#[test]
fn ten_big_blind_ranges_match_the_cfr_spike_and_poker_sense() {
    let solution = solve(
        &Spot::three_max(10.0, 10.0, 10.0).unwrap(),
        &SolveOptions::default(),
    );
    let share = |node: Node| {
        let action = node.aggressive_action();
        solution.action_share(node, action).unwrap()
    };
    // Shares of combos measured by the independent CFR+ prototype of ticket
    // #2 on its own Monte Carlo deal bank (ADR 0001).
    for (node, spike) in [
        (Node::BtnOpen, 0.327),
        (Node::SbOpen, 0.569),
        (Node::BbVsSbPush, 0.363),
    ] {
        assert!(
            (share(node) - spike).abs() < 0.01,
            "{node}: {}",
            share(node)
        );
    }
    // Overcalling a push and a call takes a stronger hand than calling one
    // push; calling a push takes a stronger hand than pushing first.
    assert!(share(Node::BbVsBtnPushSbCall) < share(Node::BbVsBtnPush));
    assert!(share(Node::SbVsBtnPush) < share(Node::SbOpen));

    let freq = |node, hand: &str, action| {
        let hand: HandClass = hand.parse().unwrap();
        solution.strategy(node, hand).unwrap().frequency(action)
    };
    for &node in solution.nodes() {
        let aggressive = node.aggressive_action();
        assert!(freq(node, "AA", aggressive) > 0.99, "{node}");
    }
    assert!(freq(Node::BtnOpen, "72o", Action::Fold) > 0.99);
    assert!(freq(Node::BbVsBtnPushSbCall, "A2o", Action::Fold) > 0.99);
}

fn always(solution: &Solution, node: Node, action: Action) -> bool {
    HandClass::all().all(|hand| solution.strategy(node, hand).unwrap().frequency(action) > 0.99)
}

#[test]
fn a_player_all_in_from_the_blind_has_no_decision() {
    let solution = solve(
        &Spot::three_max(10.0, 0.4, 10.0).unwrap(),
        &SolveOptions::default(),
    );
    // Whatever the BTN does, the SB is in; the BB only decides against the
    // BTN's push, since it already covers the SB's 0.4 BB.
    assert_eq!(solution.nodes(), &[Node::BtnOpen, Node::BbVsBtnPushSbCall]);
    assert_eq!(solution.exploitability().of(Position::Sb), 0.0);

    let nobody_decides = solve(
        &Spot::heads_up(0.5, 10.0).unwrap(),
        &SolveOptions::default(),
    );
    assert!(nobody_decides.nodes().is_empty());
    assert_eq!(nobody_decides.exploitability().total(), 0.0);
}

#[test]
fn facing_a_big_blind_all_in_for_one_big_blind_the_sb_goes_all_in_with_every_hand() {
    // Calling 0.5 BB to win 1.5 BB needs 25% equity, which every hand has.
    let heads_up = solve(
        &Spot::heads_up(10.0, 1.0).unwrap(),
        &SolveOptions::default(),
    );
    assert_eq!(heads_up.nodes(), &[Node::SbOpen]);
    assert!(always(&heads_up, Node::SbOpen, Action::Push));

    let three_max = solve(
        &Spot::three_max(10.0, 10.0, 1.0).unwrap(),
        &SolveOptions::default(),
    );
    assert_eq!(
        three_max.nodes(),
        &[Node::BtnOpen, Node::SbVsBtnPush, Node::SbOpen]
    );
    assert!(always(&three_max, Node::SbOpen, Action::Push));
}

#[test]
fn a_big_blind_that_already_covers_the_sb_push_has_no_decision_and_the_sb_pushes_everything() {
    // The SB's 0.8 BB is less than the BB's blind: pushing risks 0.3 BB more
    // to win 1.6 BB, which needs under 19% equity.
    let solution = solve(
        &Spot::three_max(10.0, 0.8, 10.0).unwrap(),
        &SolveOptions::default(),
    );
    assert_eq!(
        solution.nodes(),
        &[
            Node::BtnOpen,
            Node::SbVsBtnPush,
            Node::SbOpen,
            Node::BbVsBtnPush,
            Node::BbVsBtnPushSbCall,
        ]
    );
    assert!(always(&solution, Node::SbOpen, Action::Push));
    assert!(
        solution
            .strategy(Node::BbVsSbPush, HandClass::all().next().unwrap())
            .is_none()
    );
}

#[test]
fn a_spot_needs_two_players_with_valid_stacks() {
    assert_eq!(
        Spot::three_max(0.0, 0.0, 10.0),
        Err(SpotError::TooFewPlayers)
    );
    assert!(Spot::three_max(-1.0, 10.0, 10.0).is_err());
    assert!(Spot::three_max(10.0, f64::NAN, 10.0).is_err());
    assert!(Spot::heads_up(0.0, 10.0).is_err());
}

#[test]
fn reference_spot_is_solved_to_a_tenth_of_a_milli_big_blind_for_every_player() {
    let solution = solve(
        &Spot::three_max(10.0, 10.0, 10.0).unwrap(),
        &SolveOptions::default(),
    );
    let exploitability = solution.exploitability();
    let players: Vec<Position> = exploitability
        .per_player()
        .iter()
        .map(|(p, _)| *p)
        .collect();
    assert_eq!(players, [Position::Btn, Position::Sb, Position::Bb]);
    for (position, gain) in exploitability.per_player() {
        assert!(*gain >= 0.0, "{position}: {gain}");
    }
    assert!(exploitability.total() < 1e-4, "{exploitability:?}");
}

#[test]
fn solving_stops_at_the_target_exploitability_or_after_the_iterations() {
    let spot = Spot::three_max(12.0, 8.0, 6.0).unwrap();
    let target = 2e-3;
    let early = solve(
        &spot,
        &SolveOptions {
            iterations: 1000,
            target_exploitability: Some(target),
        },
    );
    assert!(early.exploitability().total() < target);
    assert!(
        early.iterations() < 1000,
        "{} iterations",
        early.iterations()
    );

    let fixed = solve(
        &spot,
        &SolveOptions {
            iterations: 7,
            target_exploitability: None,
        },
    );
    assert_eq!(fixed.iterations(), 7);
    assert!(fixed.exploitability().total() > target);
}
