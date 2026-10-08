//! The population model built from small batches of hands written by hand.

mod support;

use nitro_hh::Street;
use nitro_population::{
    Action, HandClass, Node, OffTreeAction, Players, PopulationModel, Position, StackBucket,
    TableSize,
};
use nitro_solver::{SolveOptions, Spot, solve};
use support::{BB, BTN, HandBuilder, SB};

/// 15 BB each, the starting stacks of an Expresso Nitro.
const START: [u32; 3] = [300, 300, 300];

#[test]
fn btn_open_frequencies_are_counted_with_their_sample() {
    let hands = [
        HandBuilder::three_handed(START).push(BTN).fold(SB).fold(BB),
        HandBuilder::three_handed(START).fold(BTN).fold(SB),
        HandBuilder::three_handed(START).fold(BTN).fold(SB),
    ]
    .map(HandBuilder::build);

    let model = PopulationModel::build(&hands, Players::Opponents);

    let stats = model
        .get(Node::BtnOpen, StackBucket::of(15.0))
        .expect("BTN open observed at 15 BB");
    assert_eq!(stats.sample(), 3);
    assert_eq!(stats.count(Action::Push), 1);
    assert_eq!(stats.count(Action::Fold), 2);
    assert!((stats.frequency(Action::Push) - 1.0 / 3.0).abs() < 1e-12);
}

/// `(push or call count, sample)` at `node` for 15 BB effective.
fn at_start(model: &PopulationModel, node: Node) -> (u32, u32) {
    let stats = model
        .get(node, StackBucket::of(15.0))
        .unwrap_or_else(|| panic!("{node} observed"));
    let aggressive = *node.actions().last().unwrap();
    (stats.count(aggressive), stats.sample())
}

#[test]
fn each_decision_is_placed_at_its_push_fold_node() {
    let hands = [
        // BTN push, SB call, BB fold.
        HandBuilder::three_handed(START).push(BTN).call(SB).fold(BB),
        // BTN push, SB fold, BB call.
        HandBuilder::three_handed(START).push(BTN).fold(SB).call(BB),
        // BTN fold, SB push, BB call.
        HandBuilder::three_handed(START).fold(BTN).push(SB).call(BB),
        // BTN fold, SB push, BB fold.
        HandBuilder::three_handed(START).fold(BTN).push(SB).fold(BB),
        // Heads-up: the button pushes as the SB, the BB folds.
        HandBuilder::heads_up(300, 300).push(SB).fold(BB),
    ]
    .map(HandBuilder::build);

    let model = PopulationModel::build(&hands, Players::Opponents);

    assert_eq!(at_start(&model, Node::BtnOpen), (2, 4));
    assert_eq!(at_start(&model, Node::SbVsBtnPush), (1, 2));
    assert_eq!(at_start(&model, Node::BbVsBtnPushSbCall), (0, 1));
    assert_eq!(at_start(&model, Node::BbVsBtnPush), (1, 1));
    assert_eq!(at_start(&model, Node::SbOpen), (3, 3));
    assert_eq!(at_start(&model, Node::BbVsSbPush), (1, 3));
}

#[test]
fn heads_up_and_three_handed_decisions_are_also_kept_apart() {
    let hands = [
        HandBuilder::three_handed(START).fold(BTN).push(SB).fold(BB),
        HandBuilder::heads_up(300, 300).push(SB).call(BB),
        HandBuilder::heads_up(300, 300).fold(SB),
    ]
    .map(HandBuilder::build);

    let model = PopulationModel::build(&hands, Players::Opponents);

    let bucket = StackBucket::of(15.0);
    let counts = |node: Node, table: TableSize| {
        let stats = model.get_at(node, bucket, table).unwrap();
        (stats.count(*node.actions().last().unwrap()), stats.sample())
    };
    // `get` pools both, as before.
    assert_eq!(at_start(&model, Node::SbOpen), (2, 3));
    assert_eq!(counts(Node::SbOpen, TableSize::ThreeMax), (1, 1));
    assert_eq!(counts(Node::SbOpen, TableSize::HeadsUp), (1, 2));
    assert_eq!(counts(Node::BbVsSbPush, TableSize::ThreeMax), (0, 1));
    assert_eq!(counts(Node::BbVsSbPush, TableSize::HeadsUp), (1, 1));
    assert_eq!(counts(Node::BtnOpen, TableSize::ThreeMax), (0, 1));
    assert!(
        model
            .get_at(Node::BtnOpen, bucket, TableSize::HeadsUp)
            .is_none()
    );
    assert_eq!(
        TableSize::of(&Spot::heads_up(10.0, 10.0).unwrap()),
        TableSize::HeadsUp
    );
    assert_eq!(
        TableSize::of(&Spot::three_max(0.0, 10.0, 10.0).unwrap()),
        TableSize::HeadsUp
    );
    assert_eq!(
        TableSize::of(&Spot::three_max(5.0, 10.0, 10.0).unwrap()),
        TableSize::ThreeMax
    );
}

#[test]
fn observed_nodes_are_listed_in_tree_order_then_from_short_to_deep_stacks() {
    let hands = [
        HandBuilder::heads_up(300, 300).push(SB).fold(BB),
        HandBuilder::three_handed([100, 300, 300])
            .push(BTN)
            .fold(SB)
            .fold(BB),
        HandBuilder::three_handed(START).fold(BTN).fold(SB),
    ]
    .map(HandBuilder::build);

    let model = PopulationModel::build(&hands, Players::Opponents);

    let listed: Vec<String> = model
        .entries()
        .map(|(node, bucket, stats)| format!("{node} {bucket} {}", stats.sample()))
        .collect();
    assert_eq!(
        listed,
        [
            "BTN open 4–6 BB 1",
            "BTN open 12–15 BB 1",
            "SB vs BTN push 4–6 BB 1",
            "SB open 12–15 BB 2",
            "BB vs BTN push 4–6 BB 1",
            "BB vs SB push 12–15 BB 1",
        ]
    );
}

#[test]
fn buckets_include_their_upper_bound() {
    let names: Vec<String> = StackBucket::all().map(|b| b.to_string()).collect();
    assert_eq!(
        names,
        [
            "0–2 BB",
            "2–4 BB",
            "4–6 BB",
            "6–8 BB",
            "8–10 BB",
            "10–12 BB",
            "12–15 BB",
            ">15 BB"
        ]
    );
    assert_eq!(StackBucket::of(10.0).to_string(), "8–10 BB");
    assert_eq!(StackBucket::of(10.05).to_string(), "10–12 BB");
    assert_eq!(StackBucket::of(0.5).to_string(), "0–2 BB");
    assert_eq!(StackBucket::of(22.5).to_string(), ">15 BB");
}

#[test]
fn decisions_are_pooled_by_the_effective_stack_of_the_hand() {
    // The smallest stack sets the bucket, whoever holds it: 10 BB, then 10.05 BB.
    let hands = [
        HandBuilder::three_handed([400, 500, 200])
            .push(BTN)
            .fold(SB)
            .call(BB),
        HandBuilder::three_handed([201, 500, 400])
            .push(BTN)
            .fold(SB)
            .fold(BB),
    ]
    .map(HandBuilder::build);

    let model = PopulationModel::build(&hands, Players::Opponents);

    let call = |bucket: f64| {
        let stats = model
            .get(Node::BbVsBtnPush, StackBucket::of(bucket))
            .unwrap();
        (stats.count(Action::Call), stats.sample())
    };
    assert_eq!(call(9.0), (1, 1));
    assert_eq!(call(11.0), (0, 1));
    assert!(
        model
            .get(Node::BbVsBtnPush, StackBucket::of(15.0))
            .is_none()
    );
}

#[test]
fn limps_min_raises_and_postflop_play_are_counted_apart_from_the_tree() {
    let hands = [
        // In the tree.
        HandBuilder::three_handed(START).push(BTN).fold(SB).fold(BB),
        // BTN folds (in the tree), the SB limps, the BB checks; a checked flop.
        HandBuilder::three_handed(START)
            .fold(BTN)
            .call(SB)
            .check(BB)
            .deal(Street::Flop)
            .check(SB)
            .check(BB),
        // BTN min-raises, the blinds fold and call.
        HandBuilder::three_handed(START)
            .raise_to(BTN, 40)
            .fold(SB)
            .call(BB),
        // BTN raises to 3 BB, short of all-in.
        HandBuilder::three_handed(START)
            .raise_to(BTN, 60)
            .fold(SB)
            .fold(BB),
    ]
    .map(HandBuilder::build);

    let model = PopulationModel::build(&hands, Players::Opponents);

    assert_eq!(model.hands(), 4);
    assert_eq!(model.hands_in_tree(), 1);
    assert_eq!(model.hands_off_tree(), 3);
    assert_eq!(at_start(&model, Node::BtnOpen), (1, 2));
    assert!(model.get(Node::SbOpen, StackBucket::of(15.0)).is_none());

    let off_tree = model.off_tree();
    assert_eq!(off_tree.count(Position::Sb, OffTreeAction::Limp), 1);
    assert_eq!(off_tree.count(Position::Btn, OffTreeAction::MinRaise), 1);
    assert_eq!(off_tree.count(Position::Btn, OffTreeAction::Raise), 1);
    assert_eq!(off_tree.count(Position::Btn, OffTreeAction::Limp), 0);
    // The BB's check, then the fold and call and the two folds after a raise.
    assert_eq!(off_tree.later_preflop(), 5);
    assert_eq!(off_tree.postflop(), 2);
}

#[test]
fn moves_that_put_every_opponent_all_in_are_pushes_and_calls() {
    let hands = [
        // The BTN raises to 200 with 400 behind: the blinds have 200 at most.
        HandBuilder::three_handed([600, 200, 150])
            .raise_to(BTN, 200)
            .fold(SB)
            .fold(BB),
        // The SB re-raises all-in over the BTN's push to isolate: a call.
        HandBuilder::three_handed([100, 400, 400])
            .push(BTN)
            .push(SB)
            .fold(BB),
        // The BB is all-in with its blind: the SB completing is a push.
        HandBuilder::heads_up(300, 15).call(SB),
        // The BTN pushes less than a big blind, the SB completes and the BB,
        // with nothing to add, checks: it stays in, a call.
        HandBuilder::three_handed([15, 300, 300])
            .call(BTN)
            .call(SB)
            .check(BB),
    ]
    .map(HandBuilder::build);

    let model = PopulationModel::build(&hands, Players::Opponents);

    assert_eq!(model.hands_in_tree(), 4);
    let count = |node, bb: f64, action| model.get(node, StackBucket::of(bb)).unwrap().count(action);
    assert_eq!(count(Node::BtnOpen, 7.5, Action::Push), 1);
    assert_eq!(count(Node::SbVsBtnPush, 5.0, Action::Call), 1);
    assert_eq!(count(Node::BbVsBtnPushSbCall, 5.0, Action::Fold), 1);
    assert_eq!(count(Node::SbOpen, 0.75, Action::Push), 1);
    assert_eq!(count(Node::BtnOpen, 0.75, Action::Push), 1);
    assert_eq!(count(Node::SbVsBtnPush, 0.75, Action::Call), 1);
    assert_eq!(count(Node::BbVsBtnPushSbCall, 0.75, Action::Call), 1);
}

#[test]
fn the_account_owner_is_left_out_of_the_population_and_modelled_alone_on_demand() {
    let hands = [
        HandBuilder::three_handed(START)
            .hero(BTN, "Ah Kd")
            .push(BTN)
            .fold(SB)
            .call(BB),
        HandBuilder::three_handed(START)
            .hero(SB, "7c 2d")
            .fold(BTN)
            .call(SB)
            .check(BB),
    ]
    .map(HandBuilder::build);

    let population = PopulationModel::build(&hands, Players::Opponents);
    assert_eq!(at_start(&population, Node::BtnOpen), (0, 1));
    assert_eq!(at_start(&population, Node::BbVsBtnPush), (1, 1));
    assert_eq!(
        population
            .off_tree()
            .count(Position::Sb, OffTreeAction::Limp),
        0
    );
    assert_eq!(population.off_tree().later_preflop(), 1);

    let hero = PopulationModel::build(&hands, Players::Hero);
    assert_eq!(at_start(&hero, Node::BtnOpen), (1, 1));
    assert!(hero.get(Node::BbVsBtnPush, StackBucket::of(15.0)).is_none());
    assert_eq!(hero.off_tree().count(Position::Sb, OffTreeAction::Limp), 1);
    assert_eq!(hero.off_tree().later_preflop(), 0);
}

fn class(text: &str) -> HandClass {
    text.parse().unwrap()
}

#[test]
fn hands_shown_at_showdown_estimate_the_range_of_each_action() {
    // 663 BTN decisions (half the 1 326 combos): 4 pushes, three of them
    // called and shown (A♥A♦, A♣A♠, K♣K♦), and 659 folds.
    let mut hands = vec![
        HandBuilder::three_handed(START)
            .push(BTN)
            .call(SB)
            .fold(BB)
            .show(BTN, "Ah Ad")
            .show(SB, "Qh Qd"),
        HandBuilder::three_handed(START)
            .push(BTN)
            .fold(SB)
            .call(BB)
            .show(BTN, "Ac As")
            .show(BB, "8h 7h"),
        HandBuilder::three_handed(START)
            .push(BTN)
            .fold(SB)
            .call(BB)
            .show(BTN, "Kc Kd")
            .show(BB, "As Kh"),
        HandBuilder::three_handed(START).push(BTN).fold(SB).fold(BB),
    ];
    hands.extend((0..659).map(|_| HandBuilder::three_handed(START).fold(BTN).fold(SB)));
    let hands: Vec<_> = hands.into_iter().map(HandBuilder::build).collect();

    let model = PopulationModel::build(&hands, Players::Opponents);
    let btn = model.get(Node::BtnOpen, StackBucket::of(15.0)).unwrap();

    let shown = btn.known_hands(Action::Push);
    assert_eq!(shown.total(), 3);
    assert_eq!(shown.count(class("AA")), 2);
    assert_eq!(shown.count(class("KK")), 1);
    // P(push | AA) = P(AA | push) · P(push) / P(AA) = 2/3 · 4/663 / (6/1326).
    let push = |hand| btn.estimated_frequency(Action::Push, class(hand)).unwrap();
    assert!((push("AA") - 8.0 / 9.0).abs() < 1e-12);
    assert!((push("KK") - 4.0 / 9.0).abs() < 1e-12);
    assert_eq!(push("72o"), 0.0);
    // Nobody shows a fold.
    assert_eq!(btn.known_hands(Action::Fold).total(), 0);
    assert_eq!(btn.estimated_frequency(Action::Fold, class("72o")), None);

    // Two BB decisions, both calls, two hands shown: the estimate is capped.
    let bb = model.get(Node::BbVsBtnPush, StackBucket::of(15.0)).unwrap();
    assert_eq!(bb.known_hands(Action::Call).count(class("87s")), 1);
    assert_eq!(bb.known_hands(Action::Call).count(class("AKo")), 1);
    assert_eq!(
        bb.estimated_frequency(Action::Call, class("87s")),
        Some(1.0)
    );
    let sb = model.get(Node::SbVsBtnPush, StackBucket::of(15.0)).unwrap();
    assert_eq!(sb.known_hands(Action::Call).count(class("QQ")), 1);
}

/// Three BTN decisions (one push), two SB opens (both folds), one SB and
/// one BB decision facing the BTN's push.
fn lockable_model() -> PopulationModel {
    let hands = [
        HandBuilder::three_handed(START).push(BTN).fold(SB).fold(BB),
        HandBuilder::three_handed(START).fold(BTN).fold(SB),
        HandBuilder::three_handed(START).fold(BTN).fold(SB),
    ]
    .map(HandBuilder::build);
    PopulationModel::build(&hands, Players::Opponents)
}

#[test]
fn the_locked_range_is_the_strongest_hands_up_to_the_observed_frequency() {
    let model = lockable_model();
    let stats = model.get(Node::BtnOpen, StackBucket::of(15.0)).unwrap();

    let push = |hand| stats.strategy(hand).frequency(Action::Push);
    // A third of the combos push: the strongest by equity against a random
    // hand, so AA, AKs, TT and K9o do, 72o, 32o and J4o do not.
    let combos: f64 = HandClass::all()
        .map(|hand| f64::from(hand.combos()) * push(hand))
        .sum();
    assert!((combos / 1326.0 - 1.0 / 3.0).abs() < 1e-12, "{combos}");
    for hand in ["AA", "AKs", "TT", "K9o"] {
        assert_eq!(push(class(hand)), 1.0, "{hand}");
    }
    for hand in ["72o", "32o", "J4o"] {
        assert_eq!(push(class(hand)), 0.0, "{hand}");
    }
    // Each class's frequencies make a whole strategy.
    for hand in HandClass::all() {
        let strategy = stats.strategy(hand);
        let total = strategy.frequency(Action::Fold) + strategy.frequency(Action::Push);
        assert!((total - 1.0).abs() < 1e-12, "{hand}: {strategy:?}");
    }
}

#[test]
fn only_the_opponents_nodes_sampled_enough_at_the_spots_stacks_are_locked() {
    let model = lockable_model();
    let spot = || Spot::three_max(15.0, 15.0, 15.0).unwrap();

    let locked = model.lock(spot(), Position::Bb, 2);
    assert!(locked.is_locked(Node::BtnOpen));
    assert!(locked.is_locked(Node::SbOpen));
    // One decision only, below the threshold.
    assert!(!locked.is_locked(Node::SbVsBtnPush));
    // The hero's own nodes are what the solver exploits with.
    assert!(!locked.is_locked(Node::BbVsBtnPush));

    let locked = model.lock(spot(), Position::Btn, 2);
    assert!(!locked.is_locked(Node::BtnOpen));
    assert!(locked.is_locked(Node::SbOpen));

    let locked = model.lock(spot(), Position::Bb, 4);
    assert!(!locked.is_locked(Node::BtnOpen));
    assert!(!locked.is_locked(Node::SbOpen));

    // Nothing was observed at 5 BB effective.
    let short = model.lock(Spot::three_max(5.0, 15.0, 15.0).unwrap(), Position::Bb, 1);
    assert!(!short.is_locked(Node::BtnOpen));
}

#[test]
fn a_heads_up_spot_is_locked_on_heads_up_decisions_only() {
    // Three-handed, the SB pushes whenever the BTN folds; heads-up, it folds.
    let hands: Vec<_> = (0..3)
        .flat_map(|_| {
            [
                HandBuilder::three_handed(START).fold(BTN).push(SB).fold(BB),
                HandBuilder::heads_up(300, 300).fold(SB),
            ]
        })
        .map(HandBuilder::build)
        .collect();
    let model = PopulationModel::build(&hands, Players::Opponents);
    let heads_up = Spot::heads_up(15.0, 15.0).unwrap();

    assert!(
        model
            .lock(heads_up.clone(), Position::Bb, 3)
            .is_locked(Node::SbOpen)
    );
    assert!(
        !model
            .lock(heads_up.clone(), Position::Bb, 4)
            .is_locked(Node::SbOpen)
    );
    let solution = solve(
        &model.lock(heads_up, Position::Bb, 3),
        &SolveOptions::default(),
    );
    let aces = solution.strategy(Node::SbOpen, class("AA")).unwrap();
    assert_eq!(aces.frequency(Action::Push), 0.0);
}
