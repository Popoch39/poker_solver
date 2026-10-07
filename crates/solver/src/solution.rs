use crate::cfr::{self, Table};
use crate::hand::HandClass;
use crate::heads_up::HeadsUpPushFold;
use crate::spot::{Position, Spot};
use crate::tree::{Action, Node};

/// How long to run the solver.
#[derive(Clone, Debug, PartialEq)]
pub struct SolveOptions {
    /// Number of CFR+ iterations.
    pub iterations: u32,
}

impl Default for SolveOptions {
    fn default() -> Self {
        SolveOptions { iterations: 1000 }
    }
}

/// Computes an equilibrium strategy for `spot`.
pub fn solve(spot: &Spot, options: &SolveOptions) -> Solution {
    let game = HeadsUpPushFold::new(spot);
    let profile = cfr::solve(&game, options.iterations);
    let exploitability = Exploitability {
        per_player: cfr::exploitability(&game, &profile),
    };
    Solution {
        spot: spot.clone(),
        iterations: options.iterations,
        profile,
        exploitability,
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

    pub fn iterations(&self) -> u32 {
        self.iterations
    }

    /// The decision nodes of the spot's tree, in the order they are played.
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
