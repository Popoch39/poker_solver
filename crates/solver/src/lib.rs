//! Preflop equilibrium solver for Expresso Nitro spots.
//!
//! The public interface is a spot configuration in, a queryable solution out:
//! [`solve`] takes a [`Spot`] and returns a [`Solution`] that gives the action
//! frequencies for any ([`Node`], [`HandClass`]) and the solution's
//! [`Exploitability`]. Spots are heads-up or 3-max push/fold. The tabular
//! CFR engine behind it is an internal detail (see
//! `docs/adr/0001-cfr-plus-maison-sur-169-classes.md` and
//! `docs/adr/0003-banque-de-donnes-stratifiee-et-dcfr.md`).

mod cfr;
mod equity;
mod hand;
mod heads_up;
mod push_fold;
mod solution;
mod spot;
mod three_max;
mod three_way_equity;
mod tree;

pub use hand::{HandClass, ParseHandClassError};
pub use solution::{Exploitability, Solution, SolveOptions, Strategy, solve};
pub use spot::{Position, Spot, SpotError};
pub use tree::{Action, Node, ParseNodeError};
