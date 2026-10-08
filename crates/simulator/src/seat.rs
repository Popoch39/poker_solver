//! The seat-strategy seam: what a seat sees and how it answers.

use std::fmt;
use std::str::FromStr;

use rand::{Rng, RngExt};
pub use rs_poker::core::{Card, Suit, Value};

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
    /// Raise to two big blinds while nobody has raised (the min-raise of the
    /// glossary): preflop only, and short of all-in.
    MinRaise,
}

impl Decision {
    pub const ALL: [Decision; 4] = [Self::Fold, Self::Call, Self::AllIn, Self::MinRaise];
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

impl SeatView {
    /// The decisions that do something different from each other, in
    /// [`Decision::ALL`] order: fold only when facing a bet, all-in only
    /// when it puts in more than a call against someone who can still call,
    /// and the min-raise only preflop while the bet is still one big blind,
    /// with chips left behind it.
    pub fn legal_decisions(&self) -> Vec<Decision> {
        let me = self.players.iter().find(|p| p.seat == self.seat);
        let (stack, bet) = me.map_or((0.0, 0.0), |p| (p.stack, p.street_bet));
        let someone_can_call = self
            .players
            .iter()
            .any(|p| p.seat != self.seat && !p.folded && !p.all_in);
        let unraised = self.street == Street::Preflop
            && self
                .players
                .iter()
                .map(|p| p.street_bet)
                .fold(0.0, f32::max)
                == self.big_blind;
        Decision::ALL
            .into_iter()
            .filter(|d| match d {
                Decision::Fold => self.to_call > 0.0,
                Decision::Call => true,
                Decision::AllIn => stack > self.to_call && someone_can_call,
                Decision::MinRaise => {
                    unraised && stack + bet > 2.0 * self.big_blind && someone_can_call
                }
            })
            .collect()
    }
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
