//! What the bot reads off the table.

use nitro_local_client::{HERO, chips};
use nitro_simulator::{Card, Decision, Position, TableView};

/// `amount` at the precision the client writes it.
fn displayed(amount: f32) -> f32 {
    chips(amount).parse().expect("the client writes numbers")
}

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
    /// What the client draws for `view`: the state a perfect reader reads.
    pub fn drawn(view: &TableView) -> Self {
        let seats = std::array::from_fn(|seat| {
            let shown = view.seats[seat];
            let out = shown.stack == 0.0 && shown.place.is_some();
            let won = view.hand_over && shown.won > 0.0;
            SeatState {
                position: shown.position,
                stack: displayed(shown.stack),
                all_in: shown.all_in && shown.stack == 0.0 && !out,
                place: shown.place.filter(|_| out),
                street_bet: if won {
                    0.0
                } else {
                    displayed(shown.street_bet)
                },
                won: if won { displayed(shown.won) } else { 0.0 },
                cards: match (shown.in_hand, shown.hole_cards) {
                    (false, _) => Cards::None,
                    (true, None) => Cards::Hidden,
                    (true, Some(cards)) => Cards::Shown(cards),
                },
            }
        });
        let call_button = view.legal.contains(&Decision::Call);
        Self {
            hand_number: view.hand_number,
            level: view.level,
            small_blind: displayed(view.small_blind),
            big_blind: displayed(view.big_blind),
            button: view.button,
            board: view.board.clone(),
            pot: displayed(view.pot),
            to_act: view.to_act,
            legal: view.legal.clone(),
            to_call: if call_button {
                displayed(view.to_call)
            } else {
                0.0
            },
            seats,
        }
    }

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
