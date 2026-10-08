//! Heads-up push/fold: the SB pushes or folds, the BB calls or folds.

use crate::cfr::{self, Game, Table};
use crate::equity::HeadsUpEquity;
use crate::hand::NUM_CLASSES;
use crate::push_fold::{PushFold, SB_BB, Slot};
use crate::spot::{Position, Spot};
use crate::tree::{Action, Node};

const FOLD: usize = 0;
const ALL_IN: usize = 1;

pub(crate) struct HeadsUpPushFold {
    equity: &'static HeadsUpEquity,
    tree: PushFold,
    nodes: Vec<Node>,
    actions: Vec<&'static [Action]>,
}

impl HeadsUpPushFold {
    pub(crate) fn new(spot: &Spot) -> HeadsUpPushFold {
        let tree = PushFold::new(spot);
        let nodes = tree.decisions();
        HeadsUpPushFold {
            equity: HeadsUpEquity::get(),
            actions: nodes.iter().map(|n| n.actions()).collect(),
            nodes,
            tree,
        }
    }

    /// Probability that each class goes all-in at `node`: from the profile at
    /// a decision, 1 where the player is forced in.
    fn all_in(&self, profile: &Table, node: Node) -> Vec<f64> {
        match self.nodes.iter().position(|&n| n == node) {
            Some(n) => (0..NUM_CLASSES)
                .map(|class| profile.infoset(n, class)[ALL_IN])
                .collect(),
            None => vec![1.0; NUM_CLASSES],
        }
    }
}

impl Game for HeadsUpPushFold {
    fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    fn actions(&self) -> &[&'static [Action]] {
        &self.actions
    }

    fn exploitability(&self, profile: &Table, locks: &[Option<&[f64]>]) -> Vec<(Position, f64)> {
        cfr::one_shot_exploitability(self, profile, locks)
    }

    fn action_values(&self, profile: &Table) -> Table {
        let (sb, bb) = (Position::Sb.index(), Position::Bb.index());
        let walk = self.tree.payoff(&[Position::Bb]);
        let steal = self.tree.payoff(&[Position::Sb]);
        let showdown = self.tree.payoff(&[Position::Sb, Position::Bb]);
        // The SB's payoff when the all-in is called, per class pair; the BB's
        // is its opposite.
        let called = |e: f64| showdown.constant[sb] + showdown.terms[sb][SB_BB] * e;

        let pushes = self.all_in(profile, Node::SbOpen);
        let calls = self.all_in(profile, Node::BbVsSbPush);
        let mut sb_values = vec![[0.0; 2]; NUM_CLASSES];
        let mut bb_values = vec![[0.0; 2]; NUM_CLASSES];
        for ((s, sb_value), push) in sb_values.iter_mut().enumerate().zip(pushes) {
            for ((b, bb_value), &call) in bb_values.iter_mut().enumerate().zip(&calls) {
                let pair = s * NUM_CLASSES + b;
                let w = self.equity.weight[pair];
                let sb_called = called(self.equity.equity[pair]);
                sb_value[FOLD] += w * walk.constant[sb];
                sb_value[ALL_IN] += w * ((1.0 - call) * steal.constant[sb] + call * sb_called);
                bb_value[FOLD] += w * push * steal.constant[bb];
                bb_value[ALL_IN] -= w * push * sb_called;
            }
        }

        let mut values = profile.zeros_like();
        for (n, &node) in self.nodes.iter().enumerate() {
            let source = match node {
                Node::SbOpen => &sb_values,
                _ => &bb_values,
            };
            debug_assert_eq!(self.tree.slot(node), Slot::Decision);
            for (class, v) in source.iter().enumerate() {
                values.infoset_mut(n, class).copy_from_slice(v);
            }
        }
        values
    }
}
