//! Node-locking through the solver's public interface: nodes fixed on given
//! strategies, the rest of the spot re-solved around them.

use nitro_solver::{
    Action, HandClass, Node, Position, SolveOptions, Spot, SpotError, Strategy, solve,
};

fn hand(s: &str) -> HandClass {
    s.parse().unwrap()
}

/// A BB that calls every push with a third of its hands, whatever it holds:
/// mixed frequencies that no solve would land on by itself.
fn third_caller(_: HandClass) -> Strategy {
    Strategy::new([(Action::Fold, 2.0 / 3.0), (Action::Call, 1.0 / 3.0)])
}

#[test]
fn a_locked_node_keeps_exactly_its_frequencies_after_the_solve() {
    let spot = Spot::heads_up(10.0, 10.0)
        .unwrap()
        .lock(Node::BbVsSbPush, third_caller)
        .unwrap();

    let solution = solve(&spot, &SolveOptions::default());

    for hand in HandClass::all() {
        assert_eq!(
            solution.strategy(Node::BbVsSbPush, hand),
            Some(third_caller(hand)),
            "{hand}"
        );
    }
    // The SB is re-solved around it: against a BB that folds two thirds of
    // the time, even 72o pushes (-0.36 BB against -0.5 BB for a fold).
    let push = |h| {
        solution
            .strategy(Node::SbOpen, hand(h))
            .unwrap()
            .frequency(Action::Push)
    };
    assert!(push("72o") > 0.99, "{}", push("72o"));
    // Only the free nodes count: the locked BB cannot deviate, and the SB
    // has nothing left to gain against it.
    let exploitability = solution.exploitability();
    assert_eq!(exploitability.of(Position::Bb), 0.0);
    assert!(exploitability.of(Position::Sb) < 1e-4, "{exploitability:?}");
}

#[test]
fn a_lock_must_give_the_nodes_actions_frequencies_that_sum_to_one() {
    let spot = Spot::heads_up(10.0, 10.0).unwrap();
    let invalid = [
        // Push is not an action of a node facing a push.
        Strategy::new([(Action::Push, 1.0)]),
        Strategy::new([(Action::Fold, 0.5), (Action::Call, 0.4)]),
        Strategy::new([(Action::Fold, 1.5), (Action::Call, -0.5)]),
        Strategy::new([(Action::Fold, f64::NAN), (Action::Call, 1.0)]),
    ];
    for strategy in invalid {
        let err = spot
            .clone()
            .lock(Node::BbVsSbPush, |_| strategy.clone())
            .unwrap_err();
        assert_eq!(
            err,
            SpotError::InvalidLock {
                node: Node::BbVsSbPush,
                hand: hand("AA")
            },
            "{strategy:?}"
        );
    }
}

#[test]
fn without_any_locked_node_the_solve_is_the_equilibrium() {
    let spot = Spot::three_max(10.0, 8.0, 12.0).unwrap();
    let equilibrium = solve(&spot, &SolveOptions::default());

    // Locking nodes on the equilibrium's own frequencies changes nothing.
    let mut locked = spot.clone();
    for node in [Node::BbVsBtnPush, Node::BbVsSbPush] {
        let strategy = |hand| equilibrium.strategy(node, hand).unwrap();
        locked = locked.lock(node, strategy).unwrap();
    }
    let relocked = solve(&locked, &SolveOptions::default());

    for &node in equilibrium.nodes() {
        let aggressive = *node.actions().last().unwrap();
        let share = |s: &nitro_solver::Solution| s.action_share(node, aggressive).unwrap();
        assert!(
            (share(&relocked) - share(&equilibrium)).abs() < 0.005,
            "{node}: {} against {}",
            share(&relocked),
            share(&equilibrium)
        );
    }
    for position in [Position::Btn, Position::Sb] {
        let gain = relocked.gain_over(&equilibrium, position, &relocked);
        assert!(gain.abs() < 1e-4, "{position}: {gain}");
    }
}

/// A BB that calls every push, whatever it holds.
fn calling_station(_: HandClass) -> Strategy {
    Strategy::new([(Action::Call, 1.0)])
}

#[test]
fn against_a_population_that_calls_too_often_the_locked_strategy_wins_more_than_the_equilibrium() {
    let spot = Spot::heads_up(10.0, 10.0).unwrap();
    let equilibrium = solve(&spot, &SolveOptions::default());
    let exploit = solve(
        &spot.lock(Node::BbVsSbPush, calling_station).unwrap(),
        &SolveOptions::default(),
    );

    // Against the calling station, the SB's exploit wins strictly more than
    // its equilibrium strategy.
    let gain = exploit.gain_over(&equilibrium, Position::Sb, &exploit);
    assert!(gain > 0.05, "gain {gain} BB/hand");
    // Always called, the SB pushes the hands with the most raw equity
    // against any hand: high cards such as Q4o rather than suited connectors
    // such as 65s, which only did well against a calling range.
    let push = |solution: &nitro_solver::Solution, h| {
        solution
            .strategy(Node::SbOpen, hand(h))
            .unwrap()
            .frequency(Action::Push)
    };
    assert!(push(&equilibrium, "65s") > 0.99 && push(&exploit, "65s") < 0.01);
    assert!(push(&equilibrium, "Q4o") < 0.01 && push(&exploit, "Q4o") > 0.99);
    // Against an equilibrium BB, it wins no more than the equilibrium.
    let edge = exploit.gain_over(&equilibrium, Position::Sb, &equilibrium);
    assert!(edge < 1e-4, "edge {edge} BB/hand");
    // An equilibrium strategy gains nothing over itself.
    assert_eq!(
        equilibrium.gain_over(&equilibrium, Position::Sb, &exploit),
        0.0
    );
}
