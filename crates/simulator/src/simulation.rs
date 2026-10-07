//! Many Expresso Nitro in parallel, summed up as win rate and ROI.

use std::sync::Arc;

use nitro_hh::{BuyIn, Euros, Hand, TournamentInfo};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use rayon::prelude::*;

use crate::game::NitroGame;
use crate::history;
use crate::prize::PrizeTable;
use crate::seat::SeatStrategy;
use crate::structure::Structure;

/// z for a two-sided 95 % interval.
const Z95: f64 = 1.959_963_984_540_054;

/// What to simulate.
#[derive(Clone)]
pub struct SimulationConfig {
    pub prize_table: PrizeTable,
    pub structure: Structure,
    pub games: u64,
    /// Same seed, same report, whatever the number of threads.
    pub seed: u64,
    /// Strategy of each seat; the first button is drawn every game.
    pub seats: [Arc<dyn SeatStrategy>; 3],
}

/// Results of a simulation.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub games: u64,
    pub buy_in_cents: u32,
    pub rake_percent: u32,
    /// Win rate that pays back the buy-ins, rake included.
    pub break_even_win_rate: f64,
    /// Mean multiplier drawn over the simulated games.
    pub average_multiplier: f64,
    pub seats: [SeatReport; 3],
}

/// Results of one seat.
#[derive(Debug, Clone, PartialEq)]
pub struct SeatReport {
    pub name: String,
    pub wins: u64,
    pub win_rate: f64,
    /// Wilson score interval.
    pub win_rate_ci95: (f64, f64),
    /// `win_rate − break_even_win_rate`.
    pub break_even_gap: f64,
    /// Mean net result per game, in buy-ins (prizes minus the full buy-in,
    /// so after rake).
    pub roi: f64,
    /// Normal approximation; jackpots make it optimistic on small samples.
    pub roi_ci95: (f64, f64),
}

struct GameOutcome {
    multiplier: u32,
    places: [u8; 3],
}

/// Plays `config.games` games on every core of the current rayon pool.
pub fn simulate(config: &SimulationConfig) -> Report {
    // Games are seeded by index and summed in index order, so neither the
    // scheduling nor the thread count changes the result.
    let outcomes: Vec<GameOutcome> = (0..config.games)
        .into_par_iter()
        .map(|index| play_game(config, index))
        .collect();
    let table = &config.prize_table;
    let break_even_win_rate = table.break_even_win_rate();
    let seats = std::array::from_fn(|seat| {
        let mut wins = 0;
        let (mut sum, mut sum_sq) = (0.0, 0.0);
        for game in &outcomes {
            let place = game.places[seat];
            wins += u64::from(place == 1);
            let net = table.prize(game.multiplier, place) - 1.0;
            sum += net;
            sum_sq += net * net;
        }
        let n = outcomes.len() as f64;
        let win_rate = wins as f64 / n;
        let roi = sum / n;
        let variance = (sum_sq - n * roi * roi) / (n - 1.0).max(1.0);
        let half_width = Z95 * (variance.max(0.0) / n).sqrt();
        SeatReport {
            name: config.seats[seat].name().to_owned(),
            wins,
            win_rate,
            win_rate_ci95: wilson_interval(wins, outcomes.len() as u64),
            break_even_gap: win_rate - break_even_win_rate,
            roi,
            roi_ci95: (roi - half_width, roi + half_width),
        }
    });
    Report {
        games: config.games,
        buy_in_cents: table.buy_in_cents(),
        rake_percent: table.rake_percent(),
        break_even_win_rate,
        average_multiplier: outcomes
            .iter()
            .map(|g| f64::from(g.multiplier))
            .sum::<f64>()
            / outcomes.len() as f64,
        seats,
    }
}

fn play_game(config: &SimulationConfig, index: u64) -> GameOutcome {
    let (multiplier, mut game) = new_game(config, index);
    GameOutcome {
        multiplier,
        places: game.play_to_end(),
    }
}

/// Game `index` of the simulation, and its multiplier.
fn new_game(config: &SimulationConfig, index: u64) -> (u32, NitroGame) {
    let mut rng = StdRng::seed_from_u64(game_seed(config.seed, index));
    let multiplier = config.prize_table.draw(&mut rng);
    let game = NitroGame::new(config.structure.clone(), config.seats.clone(), rng.random());
    (multiplier, game)
}

/// The hands of game `index` of the simulation, the very game [`simulate`]
/// plays and counts, written down by `hero` (the table seat whose cards are
/// dealt face up, if any) as the hand histories of one tournament, ready for
/// [`nitro_hh::to_ohh`].
pub fn hand_histories(config: &SimulationConfig, index: u64, hero: Option<usize>) -> Vec<Hand> {
    let (_, mut game) = new_game(config, index);
    let table = &config.prize_table;
    let rake = (table.buy_in_cents() * table.rake_percent()).div_ceil(100);
    let tournament = TournamentInfo {
        id: format!("{}-{index}", config.seed),
        name: history::TABLE_NAME.into(),
        buy_in: BuyIn {
            prize: Euros::from_cents(u64::from(table.buy_in_cents() - rake)),
            rake: Euros::from_cents(u64::from(rake)),
        },
    };
    let mut hands = Vec::new();
    while game.play_hand().is_some() {
        let mut hand = game.hand_history(hero).expect("a hand is over");
        hand.game_number = format!("{}-{}", tournament.id, hand.game_number);
        hand.tournament = Some(tournament.clone());
        hands.push(hand);
    }
    hands
}

/// SplitMix64 of the seed and the game index: decorrelated per-game seeds.
fn game_seed(seed: u64, index: u64) -> u64 {
    let mut z = seed ^ index.wrapping_add(1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn wilson_interval(successes: u64, trials: u64) -> (f64, f64) {
    let n = trials as f64;
    let p = successes as f64 / n;
    let z2 = Z95 * Z95;
    let denominator = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denominator;
    let half_width = Z95 * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt() / denominator;
    (center - half_width, center + half_width)
}
