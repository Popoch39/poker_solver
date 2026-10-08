//! Bots that play the population model.

use std::sync::Arc;

use nitro_population::{NodeStats, PopulationModel, StackBucket, TableSize};
use nitro_solver::{Action, HandClass, Node, Spot};
use rand::{Rng, RngExt};

use crate::apart;
use crate::push_fold::{self, Reading};
use crate::seat::{Decision, SeatStrategy, SeatView};

/// A bot-population: at each node of the push/fold tree, it enters the pot
/// as often as the population did at the same stacks and table size, with
/// the hands the population is locked on in the solver
/// ([`NodeStats::strategy`]: the strongest first), so that the exploit is
/// computed against what the bot really plays.
///
/// - First in, it limps, min-raises or pushes in the proportions the
///   population entered with, whatever its hand; facing a push, it calls.
/// - Stacks the population never reached at a node are played from the
///   nearest stack bucket observed there, at the same table size.
/// - A node never reached at that table size is folded (a check when free).
/// - Once it has limped or raised, facing a limp or a raise short of
///   all-in, and postflop, it follows the fallback policy of the push/fold
///   strategies (ADR 0007): it calls whatever comes after its own limp or
///   raise, and answers another player's as a push.
pub struct PopulationBot {
    model: Arc<PopulationModel>,
}

impl PopulationBot {
    pub fn new(model: Arc<PopulationModel>) -> PopulationBot {
        // Ranking the hands builds an equity table on first use: built here,
        // apart from the games, rather than from a game thread.
        if let (Some((_, _, stats)), Some(hand)) = (model.entries().next(), HandClass::all().next())
        {
            apart::run(|| stats.strategy(hand));
        }
        PopulationBot { model }
    }

    fn stats(&self, node: Node, spot: &Spot) -> Option<&NodeStats> {
        let table = TableSize::of(spot);
        let index = |bucket: StackBucket| {
            StackBucket::all()
                .position(|b| b == bucket)
                .expect("every bucket is listed")
        };
        let at = index(StackBucket::of(spot.effective_stack()));
        StackBucket::all()
            .filter_map(|bucket| Some((index(bucket), self.model.get_at(node, bucket, table)?)))
            .min_by_key(|&(i, _)| i.abs_diff(at))
            .map(|(_, stats)| stats)
    }
}

impl SeatStrategy for PopulationBot {
    fn name(&self) -> &str {
        "population"
    }

    fn decide(&self, view: &SeatView, rng: &mut dyn Rng) -> Decision {
        let decision = match push_fold::read(view) {
            Reading::CallDown => return Decision::Call,
            Reading::Tree(decision) => decision,
        };
        let Some(stats) = self.stats(decision.node, &decision.spot) else {
            return Decision::Fold;
        };
        let aggressive = decision.node.aggressive_action();
        let enters = stats.strategy(decision.hand).frequency(aggressive);
        let action = if rng.random::<f64>() < enters {
            entry(stats, rng).unwrap_or(aggressive)
        } else {
            Action::Fold
        };
        decision.decision(action)
    }
}

/// How a bot that enters the pot first in does it: a limp, a min-raise or a
/// push, in the proportions the population entered with, whatever its hand.
/// A raise above the min-raise is played as one, the simulator's only raise
/// short of all-in. `None` where the population never limped nor raised
/// (facing a push, the call is the only way in).
fn entry(stats: &NodeStats, rng: &mut dyn Rng) -> Option<Action> {
    let limps = stats.count(Action::Limp);
    let raises = stats.count(Action::Raise) + stats.other_raises();
    if limps + raises == 0 {
        return None;
    }
    let draw = rng.random_range(0..limps + raises + stats.count(Action::Push));
    Some(match draw {
        _ if draw < limps => Action::Limp,
        _ if draw < limps + raises => Action::Raise,
        _ => Action::Push,
    })
}
