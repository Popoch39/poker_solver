//! The push/fold strategies of the simulator: bots that play the population
//! model, and the hero that plays the solver's strategy, at hand-made
//! decision points.

#[path = "support/populations.rs"]
mod populations;
#[path = "support/views.rs"]
mod views;

use std::sync::Arc;

use nitro_population::{StackBucket, TableSize};
use nitro_simulator::{
    Decision, PopulationBot, Position, SeatStrategy, SeatView, SolverHero, Structure, TrivialBot,
};
use nitro_solver::{Node, SolveOptions};
use populations::{Station, population_of};
use rand::Rng;
use views::{Seat, on_flop, shares, view};

use Position::{BigBlind, Button, SmallBlind};

const FOLD: [f64; 3] = [1.0, 0.0, 0.0];
const CALL: [f64; 3] = [0.0, 1.0, 0.0];
const ALL_IN: [f64; 3] = [0.0, 0.0, 1.0];

fn bots(bot: TrivialBot) -> [Arc<dyn SeatStrategy>; 3] {
    [bot; 3].map(|b| Arc::new(b) as Arc<dyn SeatStrategy>)
}

fn bot_of(seats: [Arc<dyn SeatStrategy>; 3]) -> PopulationBot {
    PopulationBot::new(Arc::new(population_of(
        Structure::expresso_nitro(),
        seats,
        300,
        false,
    )))
}

/// The BTN first to act, three-handed at 15 BB.
const BTN_OPEN: [Seat; 3] = [
    (Button, 300.0, 0.0, false, false),
    (SmallBlind, 290.0, 10.0, false, false),
    (BigBlind, 280.0, 20.0, false, false),
];

/// The BB facing the BTN's push and the SB's call.
const BB_VS_BTN_PUSH_SB_CALL: [Seat; 3] = [
    (Button, 0.0, 300.0, false, true),
    (SmallBlind, 0.0, 300.0, false, true),
    (BigBlind, 280.0, 20.0, false, false),
];

/// Heads-up, the SB (the button) first to act.
const HEADS_UP_SB_OPEN: [Seat; 2] = [
    (SmallBlind, 440.0, 10.0, false, false),
    (BigBlind, 430.0, 20.0, false, false),
];

/// Three-handed, the SB first to act once the BTN folded.
const SB_OPEN: [Seat; 3] = [
    (Button, 300.0, 0.0, true, false),
    (SmallBlind, 290.0, 10.0, false, false),
    (BigBlind, 280.0, 20.0, false, false),
];

#[test]
fn a_population_bot_pushes_and_calls_as_the_population_does() {
    let pushers = bot_of(bots(TrivialBot::AlwaysAllIn));
    assert_eq!(shares(&pushers, &view(0, "7h 2c", &BTN_OPEN)), ALL_IN);
    assert_eq!(
        shares(&pushers, &view(2, "7h 2c", &BB_VS_BTN_PUSH_SB_CALL)),
        CALL
    );

    let folders = bot_of(bots(TrivialBot::AlwaysFold));
    assert_eq!(shares(&folders, &view(0, "Ah Ad", &BTN_OPEN)), FOLD);
}

#[test]
fn a_population_bot_pushes_its_strongest_hands_first() {
    // Random bots push half of the time they do not limp.
    let bot = bot_of(bots(TrivialBot::Random));
    assert_eq!(shares(&bot, &view(0, "Ah Ad", &BTN_OPEN)), ALL_IN);
    assert_eq!(shares(&bot, &view(0, "7h 2c", &BTN_OPEN)), FOLD);
}

/// Three-handed, folds on the button and moves all-in from the blinds;
/// heads-up, folds.
struct BlindPusher;

impl SeatStrategy for BlindPusher {
    fn name(&self) -> &str {
        "blind pusher"
    }

    fn decide(&self, view: &SeatView, _: &mut dyn Rng) -> Decision {
        if view.players.len() == 3 && view.position != Button {
            Decision::AllIn
        } else {
            Decision::Fold
        }
    }
}

#[test]
fn heads_up_and_three_handed_are_played_from_their_own_decisions() {
    let bot = bot_of([0; 3].map(|_| Arc::new(BlindPusher) as Arc<dyn SeatStrategy>));
    assert_eq!(shares(&bot, &view(0, "Ah Ad", &HEADS_UP_SB_OPEN)), FOLD);
    assert_eq!(shares(&bot, &view(1, "7h 2c", &SB_OPEN)), ALL_IN);
}

#[test]
fn stacks_never_observed_are_played_from_the_nearest_ones() {
    // Only the first hands, at 15 BB.
    let bot = PopulationBot::new(Arc::new(population_of(
        Structure::expresso_nitro(),
        bots(TrivialBot::AlwaysAllIn),
        300,
        true,
    )));
    let mut short = BTN_OPEN;
    short[0].1 = 100.0;
    assert_eq!(shares(&bot, &view(0, "7h 2c", &short)), ALL_IN);
}

#[test]
fn off_the_tree_a_bot_answers_a_limp_as_a_push_and_checks_or_calls_postflop() {
    let pushers = bot_of(bots(TrivialBot::AlwaysAllIn));
    // Heads-up, the SB limps: the BB answers as if it had pushed.
    let limped: [Seat; 2] = [
        (SmallBlind, 420.0, 20.0, false, false),
        (BigBlind, 420.0, 20.0, false, false),
    ];
    assert_eq!(shares(&pushers, &view(1, "7h 2c", &limped)), ALL_IN);
    // A node the population never reached is folded, a check when free.
    let folders = bot_of(bots(TrivialBot::AlwaysFold));
    assert_eq!(shares(&folders, &view(1, "Ah Ad", &limped)), FOLD);
    // Postflop, the hand goes to showdown.
    assert_eq!(shares(&folders, &on_flop(view(1, "7h 2c", &limped))), CALL);
}

/// Heads-up at 10 BB effective, the SB (the button) first to act.
const HEADS_UP_10_BB: [Seat; 2] = [
    (SmallBlind, 190.0, 10.0, false, false),
    (BigBlind, 500.0, 20.0, false, false),
];

/// Cheap solves: the hero's tests check what it plays, not how precisely.
fn options() -> SolveOptions {
    SolveOptions {
        iterations: 1_000,
        target_exploitability: Some(1e-4),
    }
}

#[test]
fn the_hero_plays_the_equilibrium_of_the_spot_at_hand() {
    let hero = SolverHero::equilibrium(options());
    assert_eq!(hero.name(), "solver-equilibrium");
    assert_eq!(shares(&hero, &view(0, "Ah Ad", &BTN_OPEN)), ALL_IN);
    assert_eq!(shares(&hero, &view(0, "7h 2c", &BTN_OPEN)), FOLD);
    // Facing a push, the aggressive action is a call.
    let facing: [Seat; 2] = [
        (SmallBlind, 0.0, 200.0, false, true),
        (BigBlind, 480.0, 20.0, false, false),
    ];
    assert_eq!(shares(&hero, &view(1, "Kh Kd", &facing)), CALL);
    // Off the tree, the same fallback as the bots.
    assert_eq!(shares(&hero, &on_flop(view(1, "7h 2c", &facing))), CALL);
}

#[test]
fn nearby_stacks_share_one_solve() {
    let hero = SolverHero::equilibrium(options());
    let mut deeper = HEADS_UP_10_BB;
    // 10.25 BB rounds to 10 BB; a covering stack plays like the effective.
    deeper[0].1 += 5.0;
    deeper[1].1 += 1_000.0;
    shares(&hero, &view(0, "Ah Ad", &HEADS_UP_10_BB));
    shares(&hero, &view(0, "Ah Ad", &deeper));
    assert_eq!(hero.spots_solved(), 1);
    let mut shorter = HEADS_UP_10_BB;
    shorter[0].1 -= 60.0;
    shares(&hero, &view(0, "Ah Ad", &shorter));
    assert_eq!(hero.spots_solved(), 2);
}

#[test]
fn against_a_population_that_calls_too_often_the_hero_exploits_it() {
    let model = population_of(
        Structure::expresso_nitro(),
        [0; 3].map(|_| Arc::new(Station) as Arc<dyn SeatStrategy>),
        500,
        false,
    );
    let calls = model
        .get_at(Node::BbVsSbPush, StackBucket::of(10.0), TableSize::HeadsUp)
        .expect("the station reached heads-up at 10 BB");
    assert!(calls.sample() >= 20, "{}", calls.sample());
    let equilibrium = SolverHero::equilibrium(options());
    let exploit = SolverHero::exploit(Arc::new(model), 20, options());
    assert_eq!(exploit.name(), "solver-exploit");

    // Always called, the SB pushes the hands with the most raw equity:
    // high cards such as Q4o rather than suited connectors such as 65s.
    let push = |hero: &SolverHero, cards| shares(hero, &view(0, cards, &HEADS_UP_10_BB))[2];
    assert!(push(&equilibrium, "6h 5h") > 0.99);
    assert!(push(&equilibrium, "Qh 4c") < 0.01);
    assert!(push(&exploit, "6h 5h") < 0.01);
    assert!(push(&exploit, "Qh 4c") > 0.99);
}
