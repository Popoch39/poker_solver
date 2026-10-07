//! Population model: what the players of a set of hand histories do at each
//! node of the push/fold tree.
//!
//! [`PopulationModel::build`] takes parsed hands and returns the action
//! frequencies per ([`Node`], [`StackBucket`]) with their sample size, and
//! the hands known at each action (shown at showdown) from which
//! [`NodeStats::estimated_frequency`] estimates a range. The nodes are the
//! solver's, so that the model can be compared to, or locked into, a solved
//! spot. Nothing that names a player is kept: players are told apart by
//! seat, and only to leave out (or keep alone) the account owner.
//!
//! - The bucket of a hand is that of its effective stack, the smallest stack
//!   dealt in, in BB, before the blinds: [`nitro_solver::Spot::effective_stack`]
//!   of the hand's spot.
//! - A move that puts every opponent still in all-in is a push (or a call
//!   when facing one), whatever its size: the tree's all-in is for the
//!   effective stack.
//! - Limps, raises short of all-in and what follows them, and every
//!   postflop decision, are counted apart in [`OffTree`]. The decisions
//!   before a hand leaves the tree still count at their nodes.
//!
//! [`LeakReport::build`] compares the hero's model to the equilibrium of the
//! solver, node by node, with the chips each deviation is estimated to cost.

mod bucket;
mod ev;
mod leak;
mod model;
mod stats;

pub use bucket::StackBucket;
pub use leak::{Leak, LeakOptions, LeakReport};
pub use model::{OffTree, OffTreeAction, Players, PopulationModel};
pub use nitro_solver::{Action, HandClass, Node, Position};
pub use stats::{KnownHands, NodeStats};
