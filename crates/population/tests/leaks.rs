//! The leak report of a hero built from small batches of hands written by
//! hand, at the starting stacks of an Expresso Nitro (15 BB each).
//!
//! The hands used are pure at equilibrium for those stacks: AA and KK always
//! push or call, 72o, 83o, 94o and 32o always fold.

pub mod support;

use nitro_hh::Hand;
use nitro_population::{LeakOptions, LeakReport, Node};
use support::{BB, BTN, HERO, HandBuilder, SB};

const START: [u32; 3] = [300, 300, 300];

fn options(min_sample: u32) -> LeakOptions {
    LeakOptions {
        min_sample,
        ..LeakOptions::default()
    }
}

/// The hero at the BTN pushes `cards` (folded to by both blinds).
fn btn_push(cards: &str) -> Hand {
    HandBuilder::three_handed(START)
        .hero(BTN, cards)
        .push(BTN)
        .fold(SB)
        .fold(BB)
        .build()
}

/// The hero at the BTN folds `cards`.
fn btn_fold(cards: &str) -> Hand {
    HandBuilder::three_handed(START)
        .hero(BTN, cards)
        .fold(BTN)
        .fold(SB)
        .build()
}

/// The hero in the BB calls the BTN's push with `cards`.
fn bb_call(cards: &str) -> Hand {
    HandBuilder::three_handed(START)
        .hero(BB, cards)
        .push(BTN)
        .fold(SB)
        .call(BB)
        .build()
}

#[test]
fn a_hero_who_calls_everything_in_the_bb_has_that_node_at_the_top() {
    let hands = [
        btn_push("Ah Ad"),
        btn_push("Kc Kd"),
        btn_fold("7c 2d"),
        btn_fold("3s 2h"),
        bb_call("Ah As"),
        bb_call("7h 2c"),
        bb_call("8d 3c"),
        bb_call("9s 4h"),
    ];

    let report = LeakReport::build(&hands, HERO, &options(1));

    let top = &report.leaks()[0];
    assert_eq!(top.node, Node::BbVsBtnPush);
    assert_eq!(top.sample, 4);
    assert_eq!(top.hero_frequency, 1.0);
    let equilibrium = top.equilibrium_frequency.expect("BB vs BTN push is solved");
    assert!(equilibrium > 0.05 && equilibrium < 0.5, "{equilibrium}");
    // Three calls with hands that fold at equilibrium, at 20 chips a BB:
    // more than a BB each, less than the stack.
    let lost = top.chips_lost.expect("a loss is estimated");
    assert!(lost > 3.0 * 20.0 && lost < 3.0 * 300.0, "{lost}");
    assert!(!top.inconclusive);
}

#[test]
fn nodes_below_the_sample_threshold_are_inconclusive_and_listed_last() {
    let hands = [
        btn_push("Ah Ad"),
        btn_fold("7c 2d"),
        btn_fold("3s 2h"),
        bb_call("7h 2c"),
    ];

    let report = LeakReport::build(&hands, HERO, &options(3));

    let leaks = report.leaks();
    assert_eq!(leaks.len(), 2);
    assert_eq!(
        (leaks[0].node, leaks[0].inconclusive),
        (Node::BtnOpen, false)
    );
    // The worse leak, but on a single decision.
    assert_eq!(
        (leaks[1].node, leaks[1].inconclusive),
        (Node::BbVsBtnPush, true)
    );
    assert!(leaks[1].chips_lost > leaks[0].chips_lost);
}

#[test]
fn only_the_hands_of_the_named_account_owner_are_read_and_no_name_is_kept() {
    let someone_else = HandBuilder::three_handed(START)
        .account_owner(BB, "Somebody", "7h 2c")
        .push(BTN)
        .fold(SB)
        .call(BB)
        .build();
    let hands = [btn_push("Ah Ad"), someone_else];

    let report = LeakReport::build(&hands, HERO, &options(1));

    assert_eq!(report.hands(), 1);
    let nodes: Vec<Node> = report.leaks().iter().map(|leak| leak.node).collect();
    assert_eq!(nodes, [Node::BtnOpen]);
    let printed = format!("{report:?}");
    assert!(!printed.contains(HERO) && !printed.contains("Somebody"));
}

#[test]
fn a_hero_who_plays_the_equilibrium_has_no_leak() {
    let hands = [
        btn_push("Ah Ad"),
        btn_push("Kc Kd"),
        btn_fold("7c 2d"),
        btn_fold("3s 2h"),
        bb_call("Ah As"),
        bb_call("Kh Ks"),
        HandBuilder::three_handed(START)
            .hero(BB, "8d 3c")
            .push(BTN)
            .fold(SB)
            .fold(BB)
            .build(),
        HandBuilder::three_handed(START)
            .hero(SB, "9s 4h")
            .push(BTN)
            .fold(SB)
            .fold(BB)
            .build(),
        HandBuilder::three_handed(START)
            .hero(SB, "Qs Qh")
            .fold(BTN)
            .push(SB)
            .fold(BB)
            .build(),
    ];

    let report = LeakReport::build(&hands, HERO, &options(1));

    assert_eq!(report.hands(), hands.len());
    let nodes: Vec<Node> = report.leaks().iter().map(|leak| leak.node).collect();
    assert_eq!(nodes.len(), 4, "{nodes:?}");
    for leak in report.leaks() {
        let lost = leak.chips_lost.expect("a loss is estimated");
        assert!(lost.abs() < 1.0, "{} lost {lost}", leak.node);
    }
}
