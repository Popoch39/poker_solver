//! Expresso Nitro tournament simulator.

mod game;
mod prize;
mod seat;
mod simulation;
mod structure;
mod table;

pub use game::{ActError, HandSummary, NitroGame, Seat, Step};
pub use prize::{PrizeTable, SPLIT_PERCENT, WINNER_TAKES_ALL_UP_TO};
pub use seat::{
    Card, Decision, ParseTrivialBotError, PlayerView, Position, SeatStrategy, SeatView, Street,
    Suit, TrivialBot, Value,
};
pub use simulation::{Report, SeatReport, SimulationConfig, simulate};
pub use structure::{BlindLevel, NITRO_BLIND_LEVELS, Structure};
pub use table::{TableSeat, TableView};
