use std::fmt;
use std::str::FromStr;

use rs_poker::core::Card;

use crate::equity::HeadsUpEquity;

/// Rank characters from the highest (ace) to the lowest (deuce), in the order
/// of the 13×13 grid rows and columns.
const RANKS: &[u8; 13] = b"AKQJT98765432";

/// Number of starting-hand classes (13 pairs, 78 suited, 78 offsuit).
pub(crate) const NUM_CLASSES: usize = 169;

/// One of the 169 preflop starting-hand classes (`AA`, `AKs`, `K7o`…).
///
/// Classes are laid out on the usual 13×13 grid: row and column 0 are aces,
/// pairs sit on the diagonal, suited hands above it and offsuit hands below.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HandClass(u8);

impl HandClass {
    /// Every class, in grid order (row by row, from `AA` to `22`).
    pub fn all() -> impl Iterator<Item = HandClass> {
        (0..NUM_CLASSES as u8).map(HandClass)
    }

    /// The class at `(row, col)` of the grid.
    ///
    /// # Panics
    /// If `row` or `col` is not below 13.
    pub fn at_grid(row: usize, col: usize) -> HandClass {
        assert!(row < 13 && col < 13, "grid position out of range");
        HandClass((row * 13 + col) as u8)
    }

    /// `(row, col)` of this class on the grid.
    pub fn grid_position(self) -> (usize, usize) {
        (self.index() / 13, self.index() % 13)
    }

    /// Number of card combinations in the class: 6, 4 or 12.
    pub fn combos(self) -> u32 {
        let (row, col) = self.grid_position();
        match row.cmp(&col) {
            std::cmp::Ordering::Equal => 6,
            std::cmp::Ordering::Less => 4,
            std::cmp::Ordering::Greater => 12,
        }
    }

    /// All-in equity against one random hand, card removal included: the
    /// usual measure of preflop hand strength.
    pub fn equity_vs_random(self) -> f64 {
        let table = HeadsUpEquity::get();
        let row = self.index() * NUM_CLASSES..(self.index() + 1) * NUM_CLASSES;
        let (weight, equity) = (&table.weight[row.clone()], &table.equity[row]);
        let total: f64 = weight.iter().sum();
        weight.iter().zip(equity).map(|(w, e)| w * e).sum::<f64>() / total
    }

    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }

    /// Class of two hole cards, in either order.
    pub fn from_cards(a: Card, b: Card) -> HandClass {
        let card = |c: Card| (u8::from(c.value), c.suit as u8);
        HandClass::of_cards(card(a), card(b))
    }

    /// Class of two concrete cards, given as `(rank, suit)` with rank 0 = deuce
    /// and 12 = ace (the `rs_poker` value order).
    pub(crate) fn of_cards(a: (u8, u8), b: (u8, u8)) -> HandClass {
        let (row_a, row_b) = (12 - a.0 as usize, 12 - b.0 as usize);
        let (high, low) = (row_a.min(row_b), row_a.max(row_b));
        if high == low || a.1 != b.1 {
            // Offsuit hands (and pairs) are below or on the diagonal.
            HandClass::at_grid(low, high)
        } else {
            HandClass::at_grid(high, low)
        }
    }
}

impl fmt::Display for HandClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (row, col) = self.grid_position();
        let (high, low) = (RANKS[row.min(col)] as char, RANKS[row.max(col)] as char);
        match row.cmp(&col) {
            std::cmp::Ordering::Equal => write!(f, "{high}{low}"),
            std::cmp::Ordering::Less => write!(f, "{high}{low}s"),
            std::cmp::Ordering::Greater => write!(f, "{high}{low}o"),
        }
    }
}

/// Error returned when a string is not a hand class such as `AKs`, `K7o` or `QQ`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "invalid hand class {0:?}: expected a pair (\"QQ\") or two ranks with s/o (\"AKs\", \"K7o\")"
)]
pub struct ParseHandClassError(String);

impl FromStr for HandClass {
    type Err = ParseHandClassError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseHandClassError(s.to_owned());
        let row_of = |c: u8| {
            RANKS
                .iter()
                .position(|&r| r == c.to_ascii_uppercase())
                .ok_or_else(err)
        };
        match s.as_bytes() {
            [a, b] => {
                let (ra, rb) = (row_of(*a)?, row_of(*b)?);
                if ra == rb {
                    Ok(HandClass::at_grid(ra, rb))
                } else {
                    Err(err())
                }
            }
            [a, b, kind] => {
                let (ra, rb) = (row_of(*a)?, row_of(*b)?);
                let (high, low) = (ra.min(rb), ra.max(rb));
                match kind.to_ascii_lowercase() {
                    _ if ra == rb => Err(err()),
                    b's' => Ok(HandClass::at_grid(high, low)),
                    b'o' => Ok(HandClass::at_grid(low, high)),
                    _ => Err(err()),
                }
            }
            _ => Err(err()),
        }
    }
}
