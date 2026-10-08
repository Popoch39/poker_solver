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
    /// Where the win rate stands against the break-even, at 95 %.
    pub verdict: Verdict,
}

/// Where a win rate stands against the break-even win rate: the answer to
/// "is it break even?".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The whole 95 % interval of the win rate is above the break-even.
    AboveBreakEven,
    /// The whole 95 % interval is below it.
    BelowBreakEven,
    /// The interval straddles it: more games are needed to tell.
    Undecided,
}

/// What one strategy wins over another at the same seat, played on the same
/// games.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gain {
    /// Difference of the win rates.
    pub win_rate: f64,
    pub win_rate_ci95: (f64, f64),
    /// Difference of the ROIs after rake, in buy-ins per game.
    pub roi: f64,
    pub roi_ci95: (f64, f64),
}

/// Two simulations of the same games that differ by the strategy at one
/// seat, and what the challenger wins over the baseline there.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub baseline: Report,
    pub challenger: Report,
    pub gain: Gain,
}

/// Plays the games of `baseline` and of `challenger`, the same games but for
/// the strategies, and compares them at `seat`.
///
/// The gain's intervals pair the games: game *i* is dealt the same cards in
/// both until the players' stacks part ways, so the luck of the deal
/// cancels out and the intervals are much narrower than those of two
/// independent samples.
///
/// # Panics
/// If the two simulations are not of the same games (count, seed, prizes
/// and structure).
pub fn compare(
    baseline: &SimulationConfig,
    challenger: &SimulationConfig,
    seat: usize,
) -> Comparison {
    assert!(
        baseline.games == challenger.games
            && baseline.seed == challenger.seed
            && baseline.prize_table == challenger.prize_table
            && baseline.structure == challenger.structure,
        "strategies are compared on the same games"
    );
    let (ours, theirs) = (play_games(baseline), play_games(challenger));
    let table = &baseline.prize_table;
    let pairs = || ours.iter().zip(&theirs);
    let won = |game: &GameOutcome| f64::from(u8::from(game.places[seat] == 1));
    let net = |game: &GameOutcome| table.prize(game.multiplier, game.places[seat]) - 1.0;
    let (win_rate, win_rate_ci95) = mean_ci95(pairs().map(|(b, c)| won(c) - won(b)));
    let (roi, roi_ci95) = mean_ci95(pairs().map(|(b, c)| net(c) - net(b)));
    Comparison {
        baseline: report(baseline, &ours),
        challenger: report(challenger, &theirs),
        gain: Gain {
            win_rate,
            win_rate_ci95,
            roi,
            roi_ci95,
        },
    }
}

struct GameOutcome {
    multiplier: u32,
    places: [u8; 3],
}

/// Plays `config.games` games on every core of the current rayon pool.
pub fn simulate(config: &SimulationConfig) -> Report {
    report(config, &play_games(config))
}

fn play_games(config: &SimulationConfig) -> Vec<GameOutcome> {
    // Games are seeded by index and summed in index order, so neither the
    // scheduling nor the thread count changes the result.
    (0..config.games)
        .into_par_iter()
        .map(|index| play_game(config, index))
        .collect()
}

/// Mean of `values`, with its 95 % interval by the normal approximation.
fn mean_ci95(values: impl Iterator<Item = f64>) -> (f64, (f64, f64)) {
    let (mut n, mut sum, mut sum_sq) = (0.0, 0.0, 0.0);
    for value in values {
        n += 1.0;
        sum += value;
        sum_sq += value * value;
    }
    let mean = sum / n;
    let variance = (sum_sq - n * mean * mean) / (n - 1.0).max(1.0);
    let half_width = Z95 * (variance.max(0.0) / n).sqrt();
    (mean, (mean - half_width, mean + half_width))
}

fn report(config: &SimulationConfig, outcomes: &[GameOutcome]) -> Report {
    let table = &config.prize_table;
    let break_even_win_rate = table.break_even_win_rate();
    let seats = std::array::from_fn(|seat| {
        let wins = outcomes.iter().filter(|g| g.places[seat] == 1).count() as u64;
        let win_rate = wins as f64 / outcomes.len() as f64;
        let (roi, roi_ci95) = mean_ci95(
            outcomes
                .iter()
                .map(|g| table.prize(g.multiplier, g.places[seat]) - 1.0),
        );
        let win_rate_ci95 = wilson_interval(wins, outcomes.len() as u64);
        SeatReport {
            name: config.seats[seat].name().to_owned(),
            wins,
            win_rate,
            win_rate_ci95,
            break_even_gap: win_rate - break_even_win_rate,
            roi,
            roi_ci95,
            verdict: match win_rate_ci95 {
                (low, _) if low > break_even_win_rate => Verdict::AboveBreakEven,
                (_, high) if high < break_even_win_rate => Verdict::BelowBreakEven,
                _ => Verdict::Undecided,
            },
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
/// [`nitro_hh::to_ohh`]. The tournament is named after the seed, the game
/// and the hero's strategy, so that the games of two hero strategies stay
/// apart once read back.
pub fn hand_histories(config: &SimulationConfig, index: u64, hero: Option<usize>) -> Vec<Hand> {
    let (_, mut game) = new_game(config, index);
    let table = &config.prize_table;
    let rake = (table.buy_in_cents() * table.rake_percent()).div_ceil(100);
    let strategy = hero.map_or(String::new(), |seat| {
        format!("{}-", config.seats[seat].name())
    });
    let tournament = TournamentInfo {
        id: format!("{strategy}{}-{index}", config.seed),
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
