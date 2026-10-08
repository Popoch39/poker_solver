//! Expresso Nitro tournament simulator.

mod apart;
mod game;
mod history;
mod population_bot;
mod prize;
mod push_fold;
mod seat;
mod simulation;
mod solver_hero;
mod structure;
mod table;

pub use game::{ActError, HandSummary, NitroGame, Seat, Step};
pub use population_bot::PopulationBot;
pub use prize::{PrizeTable, SPLIT_PERCENT, WINNER_TAKES_ALL_UP_TO};
pub use seat::{
    Card, Decision, ParseTrivialBotError, PlayerView, Position, SeatStrategy, SeatView, Street,
    Suit, TrivialBot, Value,
};
pub use simulation::{
    Comparison, Gain, Report, SeatReport, SimulationConfig, Verdict, compare, hand_histories,
    simulate,
};
pub use solver_hero::SolverHero;
pub use structure::{BlindLevel, NITRO_BLIND_LEVELS, Structure};
pub use table::{TableSeat, TableView};
