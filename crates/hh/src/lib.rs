//! Winamax Expresso Nitro hand histories, as structured hands.
//!
//! The public interface is text in, structured hands out: [`parse_hands`]
//! turns the text of a hand-history file into one [`Hand`] (or one
//! [`ParseError`]) per hand, [`parse_summary`] reads the tournament summary
//! file that goes with it, and [`parse_path`] does both for a whole folder.
//! [`to_ohh`] and [`from_ohh`] convert hands to and from Open Hand History,
//! the format shared with `rs_poker` (re-exported as [`ohh`] for its reader
//! and writer).

mod batch;
mod hand;
mod ohh_convert;
mod parse;
mod summary;

pub use batch::{Batch, FileError, Tournament, parse_path};
pub use hand::{Action, ActionKind, BuyIn, Euros, Hand, Player, Street, TournamentInfo};
pub use ohh_convert::{OhhError, from_ohh, to_ohh};
pub use parse::{ErrorKind, ParseError, parse_hands};
pub use rs_poker::core::Card;
pub use rs_poker::open_hand_history as ohh;
pub use summary::{Level, Summary, parse_summary};
