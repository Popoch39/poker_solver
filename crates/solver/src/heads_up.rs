//! Heads-up push/fold: the SB pushes or folds, the BB calls or folds.

use crate::cfr::{Game, Table};
use crate::equity::HeadsUpEquity;
use crate::hand::NUM_CLASSES;
use crate::spot::Spot;
use crate::tree::Node;

const SB_BLIND: f64 = 0.5;
const BB_BLIND: f64 = 1.0;

const NODES: [Node; 2] = [Node::SbOpen, Node::BbVsSbPush];
const SB_OPEN: usize = 0;
const BB_VS_PUSH: usize = 1;
const FOLD: usize = 0;
const ALL_IN: usize = 1;

pub(crate) struct HeadsUpPushFold {
    equity: &'static HeadsUpEquity,
    /// SB's net chips when the all-in is called, per class pair `[sb * 169 + bb]`.
    sb_showdown: Vec<f64>,
}

impl HeadsUpPushFold {
    pub(crate) fn new(spot: &Spot) -> HeadsUpPushFold {
        let equity = HeadsUpEquity::get();
        let contested = spot.effective_stack();
        let sb_showdown = equity
            .equity
            .iter()
            .map(|e| e * 2.0 * contested - contested)
            .collect();
        HeadsUpPushFold {
            equity,
            sb_showdown,
        }
    }
}

impl Game for HeadsUpPushFold {
    fn nodes(&self) -> &[Node] {
        &NODES
    }

    fn action_values(&self, profile: &Table) -> Table {
        let mut values = Table::new(&NODES, 0.0);
        for sb in 0..NUM_CLASSES {
            let push = profile.infoset(SB_OPEN, sb)[ALL_IN];
            let (mut fold_value, mut push_value) = (0.0, 0.0);
            for bb in 0..NUM_CLASSES {
                let pair = sb * NUM_CLASSES + bb;
                let w = self.equity.weight[pair];
                let call = profile.infoset(BB_VS_PUSH, bb)[ALL_IN];
                let showdown = self.sb_showdown[pair];
                fold_value -= w * SB_BLIND;
                push_value += w * ((1.0 - call) * BB_BLIND + call * showdown);
                // Heads-up chips are zero-sum: the BB's payoff mirrors the SB's.
                let bb_values = values.infoset_mut(BB_VS_PUSH, bb);
                bb_values[FOLD] -= w * push * BB_BLIND;
                bb_values[ALL_IN] -= w * push * showdown;
            }
            let sb_values = values.infoset_mut(SB_OPEN, sb);
            sb_values[FOLD] = fold_value;
            sb_values[ALL_IN] = push_value;
        }
        values
    }
}
