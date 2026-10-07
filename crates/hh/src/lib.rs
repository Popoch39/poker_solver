//! Winamax Expresso Nitro hand histories, as structured hands.
//!
//! The public interface is text in, structured hands out: [`parse_hands`]
//! turns the text of a hand-history file into one [`Hand`] (or one
//! [`ParseError`]) per hand, [`parse_summary`] reads the tournament summary
//! file that goes with it, and [`parse_path`] does both for a whole folder.

mod batch;
mod hand;
mod parse;
mod summary;

pub use batch::{Batch, FileError, Tournament, parse_path};
pub use hand::{Action, ActionKind, BuyIn, Euros, Hand, Player, Street, TournamentInfo};
pub use parse::{ErrorKind, ParseError, parse_hands};
pub use rs_poker::core::Card;
pub use summary::{Level, Summary, parse_summary};
