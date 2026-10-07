//! The measurement mode: how often the bot misreads the table.
//!
//! Games are played on the local client offscreen, the hero clicking a
//! random legal button. Every frame the client would show is read and
//! compared with what the simulator says is on the table.

use std::sync::Arc;
use std::time::Duration;

use nitro_local_client::{ClientConfig, Frame, LAYOUT, LocalClient, Next};
use nitro_simulator::{SeatStrategy, Structure};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use crate::read::ReadError;
use crate::state::TableState;

/// What to measure on.
#[derive(Clone)]
pub struct MeasureConfig {
    /// Hands to play, over as many games as needed.
    pub hands: u32,
    /// Seeds the games and the hero's clicks.
    pub seed: u64,
    /// The strategies of seats 1 and 2.
    pub bots: [Arc<dyn SeatStrategy>; 2],
}

/// How well the frames were read.
#[derive(Debug, Clone, PartialEq)]
pub struct Measurement {
    pub hands: u32,
    /// Frames read.
    pub states: u32,
    /// Frames read wrong, or not read at all.
    pub misread: u32,
    pub first_misread: Option<Misread>,
}

/// A frame read wrong.
#[derive(Debug, Clone, PartialEq)]
pub struct Misread {
    pub hand_number: u32,
    /// What the client drew.
    pub expected: TableState,
    pub read: Result<TableState, ReadError>,
}

impl Measurement {
    /// The share of states misread.
    pub fn error_rate(&self) -> f64 {
        if self.states == 0 {
            0.0
        } else {
            f64::from(self.misread) / f64::from(self.states)
        }
    }
}

/// Plays `config.hands` hands and reads every frame with `read`.
pub fn measure(
    config: &MeasureConfig,
    mut read: impl FnMut(&Frame) -> Result<TableState, ReadError>,
) -> Measurement {
    let mut measurement = Measurement {
        hands: 0,
        states: 0,
        misread: 0,
        first_misread: None,
    };
    let mut clicks = StdRng::seed_from_u64(config.seed);
    for game in 0.. {
        let mut client = LocalClient::new(ClientConfig {
            structure: Structure::expresso_nitro(),
            seed: config.seed.wrapping_add(game),
            bots: config.bots.clone(),
            bot_delay: Duration::ZERO,
            hand_over_delay: Duration::ZERO,
        });
        let mut hand_number = 0;
        loop {
            let view = client.view();
            if view.hand_number != hand_number {
                if measurement.hands == config.hands {
                    return measurement;
                }
                hand_number = view.hand_number;
                measurement.hands += 1;
            }
            let expected = TableState::drawn(&view);
            let state = read(&client.frame());
            measurement.states += 1;
            if state.as_ref() != Ok(&expected) {
                measurement.misread += 1;
                measurement.first_misread.get_or_insert(Misread {
                    hand_number,
                    expected,
                    read: state,
                });
            }
            match client.advance() {
                Next::After(_) => {}
                Next::WaitForClick => {
                    let legal = client.view().legal;
                    let decision = legal[clicks.random_range(0..legal.len())];
                    let (x, y) = LAYOUT.button(decision).center();
                    client.click(x, y).expect("a legal button");
                }
                Next::GameOver => break,
            }
        }
    }
    unreachable!("games are played until enough hands are")
}
