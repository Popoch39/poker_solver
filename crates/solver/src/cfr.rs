//! Tabular CFR+ over (decision node, hand class) information sets.
//!
//! Regret matching+ with linear averaging, simultaneous updates. The engine
//! only sees a [`Game`] that turns a strategy profile into counterfactual
//! action values; the game owns the tree, the chance model and the payoffs.

use crate::hand::NUM_CLASSES;
use crate::spot::Position;
use crate::tree::Node;

/// One number per (node, hand class, action), for the nodes of one tree.
#[derive(Clone, Debug)]
pub(crate) struct Table {
    nodes: Vec<Node>,
    /// Start of each node's block in `values`.
    offsets: Vec<usize>,
    values: Vec<f64>,
}

impl Table {
    pub(crate) fn new(nodes: &[Node], fill: f64) -> Table {
        let mut offsets = Vec::with_capacity(nodes.len());
        let mut len = 0;
        for node in nodes {
            offsets.push(len);
            len += NUM_CLASSES * node.actions().len();
        }
        Table {
            nodes: nodes.to_vec(),
            offsets,
            values: vec![fill; len],
        }
    }

    /// A strategy profile where every information set plays uniformly.
    pub(crate) fn uniform(nodes: &[Node]) -> Table {
        let mut table = Table::new(nodes, 0.0);
        for (n, node) in nodes.iter().enumerate() {
            let p = 1.0 / node.actions().len() as f64;
            for class in 0..NUM_CLASSES {
                table.infoset_mut(n, class).fill(p);
            }
        }
        table
    }

    pub(crate) fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Values of the information set (node index `node`, class `class`), one
    /// per action of the node.
    pub(crate) fn infoset(&self, node: usize, class: usize) -> &[f64] {
        let width = self.nodes[node].actions().len();
        &self.values[self.offsets[node] + class * width..][..width]
    }

    pub(crate) fn infoset_mut(&mut self, node: usize, class: usize) -> &mut [f64] {
        let width = self.nodes[node].actions().len();
        &mut self.values[self.offsets[node] + class * width..][..width]
    }

    fn infosets(&self) -> impl Iterator<Item = (usize, usize)> + use<> {
        let num_nodes = self.nodes.len();
        (0..num_nodes).flat_map(|n| (0..NUM_CLASSES).map(move |c| (n, c)))
    }
}

/// A game the engine can solve.
///
/// Every player must act at most once on any path of the tree, so that the
/// counterfactual values of a player's information sets do not depend on that
/// player's own strategy. This is what lets [`exploitability`] read the best
/// response straight off the action values.
pub(crate) trait Game {
    fn nodes(&self) -> &[Node];

    /// Counterfactual value of every action at every information set under
    /// `profile`: the acting player's expected payoff (in BB per hand) for
    /// taking the action, weighted by chance and by the other players' reach.
    fn action_values(&self, profile: &Table) -> Table;
}

/// Runs `iterations` of CFR+ and returns the average strategy profile.
pub(crate) fn solve(game: &impl Game, iterations: u32) -> Table {
    let nodes = game.nodes();
    let mut regrets = Table::new(nodes, 0.0);
    let mut average = Table::new(nodes, 0.0);
    let mut current = Table::uniform(nodes);
    for t in 1..=iterations {
        let values = game.action_values(&current);
        for (n, c) in current.infosets() {
            let sigma = current.infoset(n, c);
            let cv = values.infoset(n, c);
            let node_value: f64 = sigma.iter().zip(cv).map(|(s, v)| s * v).sum();
            for (r, v) in regrets.infoset_mut(n, c).iter_mut().zip(cv) {
                *r = (*r + v - node_value).max(0.0);
            }
            for (a, s) in average.infoset_mut(n, c).iter_mut().zip(sigma) {
                *a += t as f64 * s;
            }
        }
        for (n, c) in current.infosets() {
            let regret = regrets.infoset(n, c);
            let total: f64 = regret.iter().sum();
            let width = regret.len() as f64;
            for (s, r) in current.infoset_mut(n, c).iter_mut().zip(regret) {
                *s = if total > 0.0 { r / total } else { 1.0 / width };
            }
        }
    }
    normalize(average)
}

fn normalize(mut sums: Table) -> Table {
    for (n, c) in sums.infosets() {
        let infoset = sums.infoset_mut(n, c);
        let total: f64 = infoset.iter().sum();
        let width = infoset.len() as f64;
        for x in infoset {
            *x = if total > 0.0 { *x / total } else { 1.0 / width };
        }
    }
    sums
}

/// Each player's best-response gain against `profile`, in BB per hand.
pub(crate) fn exploitability(game: &impl Game, profile: &Table) -> Vec<(Position, f64)> {
    let values = game.action_values(profile);
    let mut gains: Vec<(Position, f64)> = Vec::new();
    for (n, c) in profile.infosets() {
        let cv = values.infoset(n, c);
        let played: f64 = profile
            .infoset(n, c)
            .iter()
            .zip(cv)
            .map(|(s, v)| s * v)
            .sum();
        let best = cv.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let actor = profile.nodes()[n].actor();
        match gains.iter_mut().find(|(p, _)| *p == actor) {
            Some((_, gain)) => *gain += best - played,
            None => gains.push((actor, best - played)),
        }
    }
    gains
}
