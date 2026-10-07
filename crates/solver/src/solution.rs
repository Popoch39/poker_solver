use std::io;

use crate::cfr::{self, Game, Table};
use crate::hand::HandClass;
use crate::heads_up::HeadsUpPushFold;
use crate::spot::{Position, Spot};
use crate::three_max::ThreeMaxPushFold;
use crate::tree::{Action, Node};

/// How long to run the solver: until the exploitability target is met, or
/// for at most `iterations`.
#[derive(Clone, Debug, PartialEq)]
pub struct SolveOptions {
    /// Maximum number of iterations.
    pub iterations: u32,
    /// Stop as soon as the exploitability falls below this, in BB per hand
    /// (checked every few iterations); `None` runs every iteration.
    pub target_exploitability: Option<f64>,
}

impl Default for SolveOptions {
    /// A tenth of a milli-big-blind per hand: below the 3-max chance model's
    /// own sampling error, so iterating further would not make the strategy
    /// more accurate.
    fn default() -> Self {
        SolveOptions {
            iterations: 1000,
            target_exploitability: Some(1e-5),
        }
    }
}

/// Computes an equilibrium strategy for `spot`.
pub fn solve(spot: &Spot, options: &SolveOptions) -> Solution {
    if spot.is_heads_up() {
        solve_game(spot, options, &HeadsUpPushFold::new(spot))
    } else {
        solve_game(spot, options, &ThreeMaxPushFold::new(spot))
    }
}

fn solve_game(spot: &Spot, options: &SolveOptions, game: &impl Game) -> Solution {
    let run = cfr::solve(game, options.iterations, options.target_exploitability);
    // Players without a decision cannot deviate: they gain nothing.
    let per_player = spot
        .positions()
        .into_iter()
        .map(|position| {
            let gain = run.exploitability.iter().find(|(p, _)| *p == position);
            (position, gain.map_or(0.0, |(_, gain)| *gain))
        })
        .collect();
    Solution {
        spot: spot.clone(),
        iterations: run.iterations,
        profile: run.profile,
        exploitability: Exploitability { per_player },
    }
}

/// The solved strategy of a spot, queryable by (node, hand class).
#[derive(Clone, Debug)]
pub struct Solution {
    spot: Spot,
    iterations: u32,
    profile: Table,
    exploitability: Exploitability,
}

impl Solution {
    pub fn spot(&self) -> &Spot {
        &self.spot
    }

    /// Number of iterations actually run (fewer than asked when the target
    /// exploitability was reached first).
    pub fn iterations(&self) -> u32 {
        self.iterations
    }

    /// The decision nodes of the spot's tree, in the order they are played.
    /// A player all-in from the blind, or with nothing to call, has no node.
    pub fn nodes(&self) -> &[Node] {
        self.profile.nodes()
    }

    /// The action frequencies of `hand` at `node`, or `None` if the node is
    /// not in this spot's tree.
    pub fn strategy(&self, node: Node, hand: HandClass) -> Option<Strategy> {
        let n = self.nodes().iter().position(|&x| x == node)?;
        let frequencies = self.profile.infoset(n, hand.index());
        Some(Strategy {
            frequencies: node
                .actions()
                .iter()
                .copied()
                .zip(frequencies.iter().copied())
                .collect(),
        })
    }

    /// Share of the 1326 starting combos that take `action` at `node`, each
    /// hand counted by its frequency; `None` if the node is not in the tree.
    pub fn action_share(&self, node: Node, action: Action) -> Option<f64> {
        let mut combos = 0.0;
        for hand in HandClass::all() {
            combos += f64::from(hand.combos()) * self.strategy(node, hand)?.frequency(action);
        }
        Some(combos / 1326.0)
    }

    pub fn exploitability(&self) -> &Exploitability {
        &self.exploitability
    }

    /// Writes the strategy as CSV: a `node,position,hand,action,frequency`
    /// header, then one row per node, hand class and legal action.
    pub fn write_csv(&self, mut out: impl io::Write) -> io::Result<()> {
        writeln!(out, "node,position,hand,action,frequency")?;
        for &node in self.nodes() {
            for hand in HandClass::all() {
                let strategy = self.strategy(node, hand).expect("the node is in the tree");
                for (action, frequency) in strategy.iter() {
                    let (id, actor) = (node.id(), node.actor());
                    writeln!(out, "{id},{actor},{hand},{action},{frequency:.6}")?;
                }
            }
        }
        Ok(())
    }
}

/// A mixed strategy at one information set: a frequency for each legal action.
#[derive(Clone, Debug, PartialEq)]
pub struct Strategy {
    frequencies: Vec<(Action, f64)>,
}

impl Strategy {
    /// Probability of taking `action`, between 0 and 1 (0 if it is not legal).
    pub fn frequency(&self, action: Action) -> f64 {
        self.frequencies
            .iter()
            .find(|(a, _)| *a == action)
            .map_or(0.0, |(_, f)| *f)
    }

    /// The legal actions with their frequencies.
    pub fn iter(&self) -> impl Iterator<Item = (Action, f64)> + '_ {
        self.frequencies.iter().copied()
    }
}

/// How far a solution is from an equilibrium: what each player would win per
/// hand, in BB, by switching to a best response while the others keep the
/// solution's strategy.
#[derive(Clone, Debug, PartialEq)]
pub struct Exploitability {
    per_player: Vec<(Position, f64)>,
}

impl Exploitability {
    /// Sum of the players' best-response gains (NashConv), in BB per hand.
    pub fn total(&self) -> f64 {
        self.per_player.iter().map(|(_, gain)| gain).sum()
    }

    /// Best-response gain of `position`, in BB per hand (0 for a player who
    /// is not in the spot).
    pub fn of(&self, position: Position) -> f64 {
        self.per_player
            .iter()
            .find(|(p, _)| *p == position)
            .map_or(0.0, |(_, gain)| *gain)
    }

    /// Each player's best-response gain, in BB per hand.
    pub fn per_player(&self) -> &[(Position, f64)] {
        &self.per_player
    }
}
