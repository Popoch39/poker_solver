//! The push/fold strategies of the simulator: bots that play the population
//! model, and the hero that plays the solver's strategy, at hand-made
//! decision points.

mod support;

use std::sync::Arc;

use nitro_simulator::{Decision, PopulationBot, Position, SeatStrategy, SeatView, TrivialBot};
use rand::Rng;
use support::{Seat, on_flop, population_of, shares, view};

use Position::{BigBlind, Button, SmallBlind};

const FOLD: [f64; 3] = [1.0, 0.0, 0.0];
const CALL: [f64; 3] = [0.0, 1.0, 0.0];
const ALL_IN: [f64; 3] = [0.0, 0.0, 1.0];

fn bots(bot: TrivialBot) -> [Arc<dyn SeatStrategy>; 3] {
    [bot; 3].map(|b| Arc::new(b) as Arc<dyn SeatStrategy>)
}

fn bot_of(seats: [Arc<dyn SeatStrategy>; 3]) -> PopulationBot {
    PopulationBot::new(Arc::new(population_of(seats, 300, false)))
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
