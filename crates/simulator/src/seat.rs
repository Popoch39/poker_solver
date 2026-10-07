//! The seat-strategy seam: what a seat sees and how it answers.

use std::fmt;
use std::str::FromStr;

use rand::{Rng, RngExt};
pub use rs_poker::core::Card;

/// Who plays a seat: a bot, a solver strategy, a population model, or a
/// human through a client.
///
/// A strategy only sees its own [`SeatView`] (never the other hole cards).
/// It is shared between games running in parallel, hence `&self`: a
/// strategy that needs memory keeps it behind interior mutability. Every
/// random choice must come from `rng`, which the simulator seeds, so that a
/// seed reproduces a simulation exactly.
pub trait SeatStrategy: Send + Sync {
    /// Short name shown in reports.
    fn name(&self) -> &str;

    /// The action to take at a decision point.
    fn decide(&self, view: &SeatView, rng: &mut dyn Rng) -> Decision;
}

/// An answer to a decision point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Give up the hand. With nothing to call, this checks instead.
    Fold,
    /// Check, or call the current bet (all-in when short).
    Call,
    /// Put the whole stack in.
    AllIn,
}

/// Table position. Heads-up, the button posts the small blind and is
/// reported as [`Position::SmallBlind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Position {
    Button,
    SmallBlind,
    BigBlind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Street {
    Preflop,
    Flop,
    Turn,
    River,
}

/// What a seat knows when it has to act.
#[derive(Debug, Clone, PartialEq)]
pub struct SeatView {
    /// Hand number in the game, from 1.
    pub hand_number: u32,
    /// Blind level, from 1.
    pub level: u32,
    pub small_blind: f32,
    pub big_blind: f32,
    /// Table seat (0, 1 or 2) of the player to act.
    pub seat: usize,
    pub position: Position,
    pub hole_cards: [Card; 2],
    pub board: Vec<Card>,
    pub street: Street,
    /// All chips in the middle, current street included.
    pub pot: f32,
    /// Chips needed to call, at most the stack.
    pub to_call: f32,
    /// The players dealt in this hand, in table-seat order.
    pub players: Vec<PlayerView>,
}

/// Public information about one player in the hand.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerView {
    pub seat: usize,
    pub position: Position,
    /// Chips behind.
    pub stack: f32,
    /// Chips put in on the current street.
    pub street_bet: f32,
    pub folded: bool,
    pub all_in: bool,
}

/// The trivial bots used to check the simulator on known outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrivialBot {
    /// Goes all-in at every decision.
    AlwaysAllIn,
    /// Folds at every decision (checks when it is free).
    AlwaysFold,
    /// Picks uniformly among the distinct legal actions: fold (only when
    /// facing a bet), check or call, all-in.
    Random,
}

impl TrivialBot {
    pub const ALL: [TrivialBot; 3] = [Self::AlwaysAllIn, Self::AlwaysFold, Self::Random];
}

impl SeatStrategy for TrivialBot {
    fn name(&self) -> &str {
        match self {
            Self::AlwaysAllIn => "always-all-in",
            Self::AlwaysFold => "always-fold",
            Self::Random => "random",
        }
    }

    fn decide(&self, view: &SeatView, rng: &mut dyn Rng) -> Decision {
        match self {
            Self::AlwaysAllIn => Decision::AllIn,
            Self::AlwaysFold => Decision::Fold,
            Self::Random if view.to_call > 0.0 => {
                [Decision::Fold, Decision::Call, Decision::AllIn][rng.random_range(0..3)]
            }
            Self::Random => [Decision::Call, Decision::AllIn][rng.random_range(0..2)],
        }
    }
}

impl fmt::Display for TrivialBot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseTrivialBotError(String);

impl fmt::Display for ParseTrivialBotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = TrivialBot::ALL.iter().map(|b| b.name()).collect();
        write!(
            f,
            "unknown bot `{}` (expected {})",
            self.0,
            names.join(", ")
        )
    }
}

impl std::error::Error for ParseTrivialBotError {}

impl FromStr for TrivialBot {
    type Err = ParseTrivialBotError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|b| b.name() == s)
            .ok_or_else(|| ParseTrivialBotError(s.to_owned()))
    }
}
