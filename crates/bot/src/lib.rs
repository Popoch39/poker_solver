//! The clicker bot's vision.

mod read;
mod state;
mod text;

pub use read::{ReadError, TableReader};
pub use state::{Cards, SeatState, TableState};
