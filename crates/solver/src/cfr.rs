//! Tabular Discounted CFR over (decision node, hand class) information sets.
//!
//! DCFR (Brown and Sandholm, 2019) with the parameters the authors recommend
//! (α = 3/2, β = 0, γ = 2), simultaneous updates. On a 3-max push/fold spot
//! it needs about ten times fewer iterations than CFR+ to reach a given
//! exploitability (see `docs/adr/0003-banque-de-donnes-stratifiee-et-dcfr.md`).
//! The engine only sees a [`Game`] that turns a strategy profile into
//! counterfactual action values; the game owns the tree, the chance model and
//! the payoffs.

use crate::hand::NUM_CLASSES;
use crate::spot::Position;
use crate::tree::Node;

/// Discount of positive cumulative regrets at iteration t: t^α / (t^α + 1).
const ALPHA: f64 = 1.5;
/// Discount of negative cumulative regrets: t^β / (t^β + 1) with β = 0.
const NEGATIVE_DISCOUNT: f64 = 0.5;
/// Iteration t's strategy weighs t^γ in the average.
const GAMMA: i32 = 2;
/// How often the stopping rule measures the exploitability, which costs one
/// more evaluation of the game.
const CHECK_EVERY: u32 = 10;

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

/// The average strategy profile after the run, with how long it ran and the
/// best-response gain of each player who has a decision.
pub(crate) struct Run {
    pub(crate) profile: Table,
    pub(crate) iterations: u32,
    pub(crate) exploitability: Vec<(Position, f64)>,
}

/// Runs DCFR for `iterations`, or until the average strategy's exploitability
/// falls below `target` (BB per hand), whichever comes first.
pub(crate) fn solve(game: &impl Game, iterations: u32, target: Option<f64>) -> Run {
    let nodes = game.nodes();
    let mut regrets = Table::new(nodes, 0.0);
    let mut average = Table::new(nodes, 0.0);
    let mut current = Table::uniform(nodes);
    for t in 1..=iterations {
        let values = game.action_values(&current);
        let tf = f64::from(t);
        let positive_discount = tf.powf(ALPHA) / (tf.powf(ALPHA) + 1.0);
        let weight = tf.powi(GAMMA);
        for (n, c) in current.infosets() {
            let sigma = current.infoset(n, c);
            let cv = values.infoset(n, c);
            let node_value: f64 = sigma.iter().zip(cv).map(|(s, v)| s * v).sum();
            for (r, v) in regrets.infoset_mut(n, c).iter_mut().zip(cv) {
                let discount = if *r > 0.0 {
                    positive_discount
                } else {
                    NEGATIVE_DISCOUNT
                };
                *r = *r * discount + v - node_value;
            }
            for (a, s) in average.infoset_mut(n, c).iter_mut().zip(sigma) {
                *a += weight * s;
            }
        }
        for (n, c) in current.infosets() {
            let regret = regrets.infoset(n, c);
            let total: f64 = regret.iter().map(|r| r.max(0.0)).sum();
            let width = regret.len() as f64;
            for (s, r) in current.infoset_mut(n, c).iter_mut().zip(regret) {
                *s = if total > 0.0 {
                    r.max(0.0) / total
                } else {
                    1.0 / width
                };
            }
        }
        if let Some(target) = target
            && (t % CHECK_EVERY == 0 || t == iterations)
        {
            let profile = normalize(average.clone());
            let exploitability = exploitability(game, &profile);
            if exploitability.iter().map(|(_, gain)| gain).sum::<f64>() < target {
                return Run {
                    profile,
                    iterations: t,
                    exploitability,
                };
            }
        }
    }
    let profile = normalize(average);
    Run {
        exploitability: exploitability(game, &profile),
        profile,
        iterations,
    }
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
