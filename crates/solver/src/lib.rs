//! Preflop equilibrium solver for Expresso Nitro spots.
//!
//! The public interface is a spot configuration in, a queryable solution out:
//! [`solve`] takes a [`Spot`] and returns a [`Solution`] that gives the action
//! frequencies for any ([`Node`], [`HandClass`]) and the solution's
//! [`Exploitability`]. Spots are heads-up or 3-max push/fold, optionally with
//! limps and min-raises ([`Spot::with_limp`], [`Spot::with_min_raise`]), the
//! lines that reach the flop being valued by an equity model
//! ([`RealizationFactors`]). A spot may lock
//! nodes on given strategies ([`Spot::lock`], node-locking): the solve then
//! exploits them, and [`Solution::gain_over`] measures what that gains
//! against them and costs against the equilibrium. The tabular
//! CFR engine behind it is an internal detail (see
//! `docs/adr/0001-cfr-plus-maison-sur-169-classes.md`,
//! `docs/adr/0003-banque-de-donnees-stratifiee-et-dcfr.md` and
//! `docs/adr/0005-modele-d-equite-au-flop-pour-le-limp-et-le-min-raise.md`).

mod betting_tree;
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
mod tree_game;

pub use hand::{HandClass, ParseHandClassError};
pub use solution::{Exploitability, Solution, SolveOptions, Strategy, solve};
pub use spot::{Position, RealizationFactors, Spot, SpotError};
pub use tree::{Action, Line, Node, ParseNodeError};
