//! Multiplier tables, rake and prize split of the Expresso (Nitro included).

use rand::{Rng, RngExt};

/// Multiplier table of one buy-in: the prize pool is `multiplier × buy-in`
/// (the full buy-in, rake included), drawn before the game.
#[derive(Debug, Clone, PartialEq)]
pub struct PrizeTable {
    buy_in_cents: u32,
    rake_percent: u32,
    /// Sum of the weights: the denominator of the published probabilities.
    denominator: u64,
    /// `(multiplier, weight)`: the multiplier comes out `weight / denominator`
    /// of the time.
    outcomes: &'static [(u32, u64)],
}

/// The pool goes entirely to the winner up to this multiplier; above it, it
/// is split [`SPLIT_PERCENT`] between the three places.
pub const WINNER_TAKES_ALL_UP_TO: u32 = 20;
/// Shares of 1st, 2nd and 3rd place above [`WINNER_TAKES_ALL_UP_TO`].
pub const SPLIT_PERCENT: [u32; 3] = [80, 12, 8];

// Source: https://www.winamax.fr/expresso, retrieved 2026-10-07. The page has
// one set of tables for the classic Expresso and the Nitro. The distributions
// changed on 2026-04-10, so older tables (forums, 2023 datasets) differ.
// Rake (same page): 8 % at 0.25 and 0.50 €, 7 % from 1 €. Split rule (same
// page): winner takes all up to x20, 80/12/8 above.
// Rows are (multiplier, count out of the published denominator), largest
// multiplier first, exactly as published.
const TABLES: [PrizeTable; 11] = [
    PrizeTable::new(
        25,
        8,
        &[
            (100_000, 2),
            (1_000, 100),
            (100, 2_000),
            (20, 10_000),
            (10, 100_000),
            (5, 400_000),
            (4, 800_000),
            (3, 3_324_204),
            (2, 5_363_694),
        ],
    ),
    PrizeTable::new(
        50,
        8,
        &[
            (100_000, 2),
            (1_000, 100),
            (100, 2_000),
            (20, 10_000),
            (10, 100_000),
            (5, 400_000),
            (4, 800_000),
            (3, 3_324_204),
            (2, 5_363_694),
        ],
    ),
    PrizeTable::new(
        100,
        7,
        &[
            (100_000, 4),
            (1_000, 100),
            (100, 2_000),
            (20, 10_000),
            (10, 100_000),
            (5, 400_000),
            (4, 850_000),
            (3, 3_324_208),
            (2, 5_313_688),
        ],
    ),
    PrizeTable::new(
        200,
        7,
        &[
            (500_000, 1),
            (1_000, 100),
            (100, 2_000),
            (20, 10_000),
            (10, 100_000),
            (5, 400_000),
            (4, 800_000),
            (3, 3_324_202),
            (2, 5_363_697),
        ],
    ),
    PrizeTable::new(
        500,
        7,
        &[
            (200_000, 1),
            (1_000, 100),
            (100, 2_000),
            (20, 15_000),
            (10, 100_000),
            (5, 400_000),
            (4, 900_000),
            (3, 3_334_202),
            (2, 5_248_697),
        ],
    ),
    PrizeTable::new(
        1_000,
        7,
        &[
            (100_000, 2),
            (1_000, 100),
            (100, 2_000),
            (20, 15_000),
            (10, 100_000),
            (5, 400_000),
            (4, 900_000),
            (3, 3_334_204),
            (2, 5_248_694),
        ],
    ),
    PrizeTable::new(
        2_500,
        7,
        &[
            (40_000, 5),
            (1_000, 100),
            (100, 2_000),
            (20, 15_000),
            (10, 100_000),
            (5, 400_000),
            (4, 900_000),
            (3, 3_334_210),
            (2, 5_248_685),
        ],
    ),
    PrizeTable::new(
        5_000,
        7,
        &[
            (20_000, 10),
            (1_000, 100),
            (100, 2_000),
            (20, 15_000),
            (10, 100_000),
            (5, 400_000),
            (4, 900_000),
            (3, 3_334_220),
            (2, 5_248_670),
        ],
    ),
    PrizeTable::new(
        10_000,
        7,
        &[
            (10_000, 20),
            (1_000, 100),
            (100, 2_000),
            (20, 15_000),
            (10, 100_000),
            (5, 400_000),
            (4, 900_000),
            (3, 3_334_240),
            (2, 5_248_640),
        ],
    ),
    // 250 and 500 € are published out of 1 000 000 instead of 10 000 000.
    PrizeTable::new(
        25_000,
        7,
        &[
            (4_000, 4),
            (400, 25),
            (100, 200),
            (20, 1_500),
            (10, 10_000),
            (5, 40_000),
            (4, 90_000),
            (3, 337_458),
            (2, 520_813),
        ],
    ),
    PrizeTable::new(
        50_000,
        7,
        &[
            (4_000, 4),
            (400, 25),
            (100, 200),
            (20, 1_500),
            (10, 10_000),
            (5, 40_000),
            (4, 90_000),
            (3, 337_458),
            (2, 520_813),
        ],
    ),
];

impl PrizeTable {
    /// A table with its outcomes; the denominator is the sum of the weights.
    pub const fn new(
        buy_in_cents: u32,
        rake_percent: u32,
        outcomes: &'static [(u32, u64)],
    ) -> Self {
        let mut denominator = 0;
        let mut i = 0;
        while i < outcomes.len() {
            denominator += outcomes[i].1;
            i += 1;
        }
        Self {
            buy_in_cents,
            rake_percent,
            denominator,
            outcomes,
        }
    }

    /// The official table of a buy-in given in euro cents (25 for 0.25 €).
    pub fn for_buy_in(buy_in_cents: u32) -> Option<&'static PrizeTable> {
        TABLES.iter().find(|t| t.buy_in_cents == buy_in_cents)
    }

    /// The official tables, cheapest buy-in first.
    pub fn official_tables() -> &'static [PrizeTable] {
        &TABLES
    }

    pub fn buy_in_cents(&self) -> u32 {
        self.buy_in_cents
    }

    /// Share of the buy-in kept by the room, in percent.
    pub fn rake_percent(&self) -> u32 {
        self.rake_percent
    }

    /// `(multiplier, probability)` pairs.
    pub fn outcomes(&self) -> impl Iterator<Item = (u32, f64)> + '_ {
        self.outcomes
            .iter()
            .map(|&(m, w)| (m, w as f64 / self.denominator as f64))
    }

    /// Expected prize pool, in buy-ins.
    pub fn expected_multiplier(&self) -> f64 {
        self.outcomes().map(|(m, p)| f64::from(m) * p).sum()
    }

    /// Win rate at which the prizes pay back the buy-ins, rake included.
    ///
    /// As in the spec, every pool is counted as winner-takes-all: the 80/12/8
    /// split only applies from x100, and moves this by less than 0.2 point.
    pub fn break_even_win_rate(&self) -> f64 {
        1.0 / self.expected_multiplier()
    }

    /// Draws the multiplier of one game.
    pub fn draw<R: Rng + ?Sized>(&self, rng: &mut R) -> u32 {
        let mut ticket = rng.random_range(0..self.denominator);
        for &(multiplier, weight) in self.outcomes {
            if ticket < weight {
                return multiplier;
            }
            ticket -= weight;
        }
        unreachable!("the denominator is the sum of the weights")
    }

    /// Prize of a finishing place (1, 2 or 3) at a multiplier, in buy-ins.
    pub fn prize(&self, multiplier: u32, place: u8) -> f64 {
        let share = if multiplier <= WINNER_TAKES_ALL_UP_TO {
            if place == 1 { 100 } else { 0 }
        } else {
            match place {
                1..=3 => SPLIT_PERCENT[usize::from(place - 1)],
                _ => 0,
            }
        };
        f64::from(multiplier) * f64::from(share) / 100.0
    }
}
