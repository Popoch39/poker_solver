//! Heads-up push/fold through the solver's public interface.

use nitro_solver::{Action, HandClass, Node, Position, SolveOptions, Spot, solve};

fn hand(s: &str) -> HandClass {
    s.parse().unwrap()
}

#[test]
fn heads_up_tree_has_sb_open_then_bb_facing_the_push() {
    let solution = solve(
        &Spot::heads_up(10.0, 10.0).unwrap(),
        &SolveOptions::default(),
    );
    assert_eq!(solution.nodes(), &[Node::SbOpen, Node::BbVsSbPush]);
    assert_eq!(Node::SbOpen.actor(), Position::Sb);
    assert_eq!(Node::SbOpen.actions(), &[Action::Fold, Action::Push]);
    assert_eq!(Node::BbVsSbPush.actor(), Position::Bb);
    assert_eq!(Node::BbVsSbPush.actions(), &[Action::Fold, Action::Call]);
}

#[test]
fn nodes_have_short_identifiers_for_the_command_line() {
    for node in [Node::SbOpen, Node::BbVsSbPush] {
        assert_eq!(node.id().parse::<Node>(), Ok(node));
    }
    assert_eq!("sb-open".parse::<Node>(), Ok(Node::SbOpen));
    assert_eq!("bb-vs-sb-push".parse::<Node>(), Ok(Node::BbVsSbPush));
    assert!("co-open".parse::<Node>().is_err());
}

#[test]
fn premium_hands_always_go_all_in_and_trash_folds_at_ten_bb() {
    let solution = solve(
        &Spot::heads_up(10.0, 10.0).unwrap(),
        &SolveOptions::default(),
    );
    let freq = |node, h, action| solution.strategy(node, hand(h)).unwrap().frequency(action);

    assert!(freq(Node::SbOpen, "AA", Action::Push) > 0.99);
    assert!(freq(Node::SbOpen, "72o", Action::Fold) > 0.99);
    assert!(freq(Node::BbVsSbPush, "AA", Action::Call) > 0.99);
    assert!(freq(Node::BbVsSbPush, "72o", Action::Fold) > 0.99);

    let strategy = solution.strategy(Node::SbOpen, hand("K7o")).unwrap();
    let total: f64 = Node::SbOpen
        .actions()
        .iter()
        .map(|&a| strategy.frequency(a))
        .sum();
    assert!((total - 1.0).abs() < 1e-9);
}

#[test]
fn an_action_not_available_at_the_node_has_zero_frequency() {
    let solution = solve(
        &Spot::heads_up(10.0, 10.0).unwrap(),
        &SolveOptions::default(),
    );
    let strategy = solution.strategy(Node::SbOpen, hand("AA")).unwrap();
    assert_eq!(strategy.frequency(Action::Call), 0.0);
}

#[test]
fn spot_stacks_must_be_positive_and_may_be_below_a_blind() {
    assert!(Spot::heads_up(0.0, 10.0).is_err());
    assert!(Spot::heads_up(10.0, f64::NAN).is_err());
    assert!(Spot::heads_up(0.5, 10.0).is_ok());
    let spot = Spot::heads_up(9.0, 25.0).unwrap();
    assert_eq!(spot.stack(Position::Sb), 9.0);
    assert_eq!(spot.stack(Position::Bb), 25.0);
    assert_eq!(spot.effective_stack(), 9.0);
}
