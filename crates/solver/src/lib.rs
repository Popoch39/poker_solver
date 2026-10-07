//! Preflop equilibrium solver for Expresso Nitro spots.
//!
//! The public interface is a spot configuration in, a queryable solution out:
//! [`solve`] takes a [`Spot`] and returns a [`Solution`] that gives the action
//! frequencies for any ([`Node`], [`HandClass`]) and the solution's
//! [`Exploitability`]. The CFR+ engine behind it is an internal detail
//! (see `docs/adr/0001-cfr-plus-maison-sur-169-classes.md`).

mod cfr;
mod equity;
mod hand;
mod heads_up;
mod solution;
mod spot;
mod tree;

pub use hand::{HandClass, ParseHandClassError};
pub use solution::{Exploitability, Solution, SolveOptions, Strategy, solve};
pub use spot::{Position, Spot, SpotError};
pub use tree::{Action, Node, ParseNodeError};
