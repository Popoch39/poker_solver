use std::fmt;

use chrono::{DateTime, Utc};
use rs_poker::core::Card;

/// One hand of an Expresso Nitro, as written in a Winamax hand history.
///
/// Chip amounts are integers. Players are identified by their seat number
/// everywhere but in [`Player::name`], which only exists to recognise the
/// hero: nothing downstream should key anything on a name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hand {
    /// Unique id of the hand on the site (Winamax `HandId` without the `#`).
    pub game_number: String,
    pub started_at: DateTime<Utc>,
    pub tournament: Option<TournamentInfo>,
    /// 1-based blind level; Open Hand History has no field for it.
    pub level: Option<u32>,
    pub table_name: String,
    /// Number of seats at the table (3 in Expresso Nitro, even heads-up).
    pub table_size: u8,
    /// Seat of the button.
    pub button: u8,
    pub small_blind: u32,
    pub big_blind: u32,
    /// Players dealt in, by increasing seat number.
    pub players: Vec<Player>,
    /// Seat of the account owner, the only player whose cards are dealt face up.
    pub hero: Option<u8>,
    /// Every action in order, blinds included.
    pub actions: Vec<Action>,
    /// Community cards actually dealt: 0, 3, 4 or 5.
    pub board: Vec<Card>,
}

/// Tournament a hand belongs to, as far as the hand itself tells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TournamentInfo {
    pub id: String,
    pub name: String,
    pub buy_in: BuyIn,
}

/// A buy-in split as Winamax writes it: `0.93€ + 0.07€`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuyIn {
    /// The part that goes to the prize pool.
    pub prize: Euros,
    pub rake: Euros,
}

impl BuyIn {
    pub fn total(self) -> Euros {
        Euros::from_cents(self.prize.cents() + self.rake.cents())
    }
}

/// An amount of money, held in whole cents so that it compares exactly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Euros(u64);

impl Euros {
    pub fn from_cents(cents: u64) -> Euros {
        Euros(cents)
    }

    pub fn cents(self) -> u64 {
        self.0
    }
}

impl fmt::Display for Euros {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:02}€", self.0 / 100, self.0 % 100)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player {
    pub seat: u8,
    pub name: String,
    /// Chips at the start of the hand, before the blinds.
    pub stack: u32,
    /// Hole cards, when known (the hero's, or cards shown).
    pub cards: Option<[Card; 2]>,
    /// Whether the player showed their cards.
    pub showed: bool,
    /// Chips taken from the pot at the end of the hand, including any part of
    /// the player's own bet that nobody called.
    pub collected: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Street {
    Preflop,
    Flop,
    Turn,
    River,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Action {
    pub street: Street,
    pub seat: u8,
    pub kind: ActionKind,
    /// The action put the player's last chip in.
    pub all_in: bool,
}

/// What a player did. Amounts are chips added to the pot by the action,
/// except for [`ActionKind::Raise`], which gives the player's total on the
/// street after raising.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    SmallBlind(u32),
    BigBlind(u32),
    Fold,
    Check,
    Call(u32),
    Bet(u32),
    Raise { to: u32 },
}
