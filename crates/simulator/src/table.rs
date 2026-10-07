//! What an observer sitting at one seat sees of the whole table: what a
//! client displays.

use crate::seat::{Card, Decision, Position, Street};

/// The table as one seat sees it, at any step of the game.
///
/// Between hands it keeps showing the last hand, finished, until the next
/// one is dealt.
#[derive(Debug, Clone, PartialEq)]
pub struct TableView {
    /// The seat looking at the table.
    pub observer: usize,
    /// Hand on the table, from 1 (0 before the first one is dealt).
    pub hand_number: u32,
    /// Blind level of that hand, from 1.
    pub level: u32,
    pub small_blind: f32,
    pub big_blind: f32,
    /// Seat of the button.
    pub button: usize,
    /// The last street dealt.
    pub street: Street,
    pub board: Vec<Card>,
    /// All chips in the middle, current street included; once the hand is
    /// over, what it was played for.
    pub pot: f32,
    /// The seat whose decision is pending.
    pub to_act: Option<usize>,
    /// The observer's legal decisions; empty unless it is to act.
    pub legal: Vec<Decision>,
    /// Chips the observer needs to call, when it is to act.
    pub to_call: f32,
    /// The hand on the table is finished.
    pub hand_over: bool,
    /// By table seat.
    pub seats: [TableSeat; 3],
}

/// One seat as an observer sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TableSeat {
    /// Chips behind.
    pub stack: f32,
    /// `None` when not dealt in this hand.
    pub position: Option<Position>,
    /// Chips put in on the current street.
    pub street_bet: f32,
    /// Dealt in and not folded.
    pub in_hand: bool,
    pub all_in: bool,
    /// The observer's own cards, and the cards shown down at the end of a
    /// hand; `None` when hidden.
    pub hole_cards: Option<[Card; 2]>,
    /// Chips won in the hand, once it is over.
    pub won: f32,
    /// Finishing place, once eliminated (or once the game is won).
    pub place: Option<u8>,
}
