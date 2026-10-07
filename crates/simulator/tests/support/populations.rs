//! Populations of known behaviour, built from simulated hand histories.

use std::sync::Arc;

use nitro_population::{Players, PopulationModel};
use nitro_simulator::{
    Decision, PrizeTable, SeatStrategy, SeatView, SimulationConfig, Structure, hand_histories,
};
use rand::{Rng, RngExt};

pub fn config(
    structure: Structure,
    seats: [Arc<dyn SeatStrategy>; 3],
    games: u64,
    seed: u64,
) -> SimulationConfig {
    SimulationConfig {
        prize_table: PrizeTable::for_buy_in(100).unwrap().clone(),
        structure,
        games,
        seed,
        seats,
    }
}

/// The population model of every player of `games` simulated games
/// between `seats`; with `first_hands_only`, of their first hands only
/// (three-handed, at the starting stacks).
pub fn population_of(
    structure: Structure,
    seats: [Arc<dyn SeatStrategy>; 3],
    games: u64,
    first_hands_only: bool,
) -> PopulationModel {
    let config = config(structure, seats, games, 99);
    let hands: Vec<_> = (0..games)
        .flat_map(|game| {
            let hands = hand_histories(&config, game, None);
            let keep = if first_hands_only { 1 } else { hands.len() };
            hands.into_iter().take(keep)
        })
        .collect();
    PopulationModel::build(&hands, Players::Opponents)
}

/// A calling station: opens all-in half of the time whatever its cards,
/// and calls every push.
pub struct Station;

impl SeatStrategy for Station {
    fn name(&self) -> &str {
        "station"
    }

    fn decide(&self, view: &SeatView, rng: &mut dyn Rng) -> Decision {
        let facing_all_in = view.players.iter().any(|p| p.all_in && p.seat != view.seat);
        match () {
            _ if facing_all_in => Decision::Call,
            _ if rng.random::<bool>() => Decision::AllIn,
            _ => Decision::Fold,
        }
    }
}
