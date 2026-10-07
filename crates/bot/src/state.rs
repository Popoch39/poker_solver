//! What the bot reads off the table.

use nitro_local_client::HERO;
use nitro_simulator::{Card, Decision, Position};

/// The game as the local client draws it, read from its pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct TableState {
    /// Hand on the table, from 1 (0 before the first one is dealt).
    pub hand_number: u32,
    pub level: u32,
    pub small_blind: f32,
    pub big_blind: f32,
    /// Seat of the dealer button.
    pub button: usize,
    pub board: Vec<Card>,
    /// All chips in the middle; 0 when no pot is drawn.
    pub pot: f32,
    /// The seat whose turn it is.
    pub to_act: Option<usize>,
    /// The hero's buttons, in [`Decision::ALL`] order; empty unless the
    /// hero is to act.
    pub legal: Vec<Decision>,
    /// Chips the hero needs to call (0 to check).
    pub to_call: f32,
    /// By table seat; the hero is [`HERO`].
    pub seats: [SeatState; 3],
}

/// One seat as drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeatState {
    /// `None` when not dealt in this hand.
    pub position: Option<Position>,
    /// Chips behind.
    pub stack: f32,
    /// Shown as `ALL-IN`, with nothing behind.
    pub all_in: bool,
    /// The finishing place, shown as `OUT 3RD` once eliminated.
    pub place: Option<u8>,
    /// Chips put in on this street.
    pub street_bet: f32,
    /// Chips won, once the hand is over.
    pub won: f32,
    pub cards: Cards,
}

/// The cards drawn in front of a seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cards {
    /// Not dealt in, or folded.
    None,
    /// Dealt in, face down.
    Hidden,
    Shown([Card; 2]),
}

impl TableState {
    pub fn hero_cards(&self) -> Option<[Card; 2]> {
        match self.seats[HERO].cards {
            Cards::Shown(cards) => Some(cards),
            Cards::None | Cards::Hidden => None,
        }
    }

    pub fn hero_position(&self) -> Option<Position> {
        self.seats[HERO].position
    }
}
