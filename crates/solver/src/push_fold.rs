//! The push/fold betting model shared by the heads-up and 3-max games: which
//! nodes are real decisions for a spot, and what each player wins at the end
//! of every line.
//!
//! Players act once each, BTN then SB then BB, and either fold or go all-in.
//! A player who goes all-in commits the whole stack; the part nobody can
//! match comes back through the side-pot layers. A folded player leaves the
//! blind in the pot.

use crate::spot::{Position, Spot};
use crate::tree::Node;

/// Showdown terms a payoff depends on, all from the dealt hands: the share of
/// a two-way pot won by the first of the pair (BTN-SB, BTN-BB, SB-BB), then
/// the BTN's and the SB's shares of a three-way pot.
pub(crate) const NUM_TERMS: usize = 5;
pub(crate) const BTN_SB: usize = 0;
pub(crate) const BTN_BB: usize = 1;
pub(crate) const SB_BB: usize = 2;
pub(crate) const BTN_SHARE3: usize = 3;
pub(crate) const SB_SHARE3: usize = 4;

/// Each player's net chips at the end of a line, in BB, as an affine function
/// of the showdown terms.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Payoff {
    pub(crate) constant: [f64; 3],
    pub(crate) terms: [[f64; NUM_TERMS]; 3],
}

/// What the tree looks like at one node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    /// The player chooses between folding and going all-in.
    Decision,
    /// The player has nothing to decide (all-in from the blind, or already
    /// covering every bet) and stays in the hand.
    ForcedIn,
    /// No line of the spot reaches the node.
    Unreachable,
}

/// The push/fold game of one spot, apart from the deal.
pub(crate) struct PushFold {
    /// Indexed like [`Node::ALL`].
    slots: [Slot; 6],
    /// Indexed by the set of players still in at showdown, as a bit mask over
    /// [`Position::index`].
    payoffs: [Payoff; 8],
}

impl PushFold {
    pub(crate) fn new(spot: &Spot) -> PushFold {
        let stacks = Position::ALL.map(|p| spot.stack(p));
        let blinds = Position::ALL.map(|p| p.blind().min(spot.stack(p)));
        let mut slots = [Slot::Unreachable; 6];
        walk(&stacks, &blinds, 0, 0, &mut slots);
        PushFold {
            slots,
            payoffs: std::array::from_fn(|live| payoff(&stacks, &blinds, live)),
        }
    }

    pub(crate) fn slot(&self, node: Node) -> Slot {
        self.slots[node_index(node)]
    }

    /// The real decisions, in play order.
    pub(crate) fn decisions(&self) -> Vec<Node> {
        Node::ALL
            .into_iter()
            .filter(|&n| self.slot(n) == Slot::Decision)
            .collect()
    }

    /// Payoff of the line where exactly the players in `live` stay in.
    pub(crate) fn payoff(&self, live: &[Position]) -> &Payoff {
        &self.payoffs[live.iter().fold(0, |m, p| m | 1 << p.index())]
    }
}

fn node_index(node: Node) -> usize {
    Node::ALL
        .iter()
        .position(|&n| n == node)
        .expect("ALL lists every node")
}

/// Visits every line from the player at `seat`, given the players already
/// in (`live`) and already folded (`folded`), and records how each node is
/// played.
fn walk(stacks: &[f64; 3], blinds: &[f64; 3], live: u8, folded: u8, slots: &mut [Slot; 6]) {
    let acted = live | folded;
    let Some(seat) = (0..3).find(|&s| acted & 1 << s == 0) else {
        return;
    };
    if stacks[seat] == 0.0 {
        // An empty seat (heads-up BTN) neither bets nor wins.
        return walk(stacks, blinds, live, folded | 1 << seat, slots);
    }
    // What the others still in have committed: a stack once all-in, the
    // blind before acting.
    let facing = (0..3)
        .filter(|&s| s != seat && folded & 1 << s == 0 && stacks[s] > 0.0)
        .map(|s| {
            if live & 1 << s != 0 {
                stacks[s]
            } else {
                blinds[s]
            }
        })
        .fold(0.0, f64::max);
    let decides = stacks[seat] > blinds[seat] && facing > blinds[seat];
    let in_line = live | 1 << seat;
    if let Some(node) = node_at(seat, live) {
        let slot = &mut slots[node_index(node)];
        *slot = if decides {
            Slot::Decision
        } else {
            Slot::ForcedIn
        };
    }
    walk(stacks, blinds, in_line, folded, slots);
    if decides {
        walk(stacks, blinds, live, folded | 1 << seat, slots);
    }
}

/// The node where `seat` acts after the players in `live` went all-in, or
/// `None` for the BB when everyone folded to it (a walk, never a decision).
fn node_at(seat: usize, live: u8) -> Option<Node> {
    let (btn_in, sb_in) = (live & 1 != 0, live & 2 != 0);
    match (seat, btn_in, sb_in) {
        (0, _, _) => Some(Node::BtnOpen),
        (1, true, _) => Some(Node::SbVsBtnPush),
        (1, false, _) => Some(Node::SbOpen),
        (_, true, false) => Some(Node::BbVsBtnPush),
        (_, true, true) => Some(Node::BbVsBtnPushSbCall),
        (_, false, true) => Some(Node::BbVsSbPush),
        (_, false, false) => None,
    }
}

/// Payoff when the players in the `live` mask stay in and the others fold.
///
/// The pot is cut into layers at each player's commitment. A layer is shared
/// by the live players who committed up to it, by equity; a layer that no
/// live player reaches only exists on lines the tree never plays, and goes
/// back to whoever put it in so that chips are still conserved.
fn payoff(stacks: &[f64; 3], blinds: &[f64; 3], live: usize) -> Payoff {
    let is_live = |p: usize| live & 1 << p != 0 && stacks[p] > 0.0;
    let commit: [f64; 3] = std::array::from_fn(|p| if is_live(p) { stacks[p] } else { blinds[p] });
    let mut levels: Vec<f64> = commit.iter().copied().filter(|&c| c > 0.0).collect();
    levels.sort_by(f64::total_cmp);
    levels.dedup();

    let mut payoff = Payoff {
        constant: commit.map(|c| -c),
        ..Payoff::default()
    };
    let mut floor = 0.0;
    for level in levels {
        let part = |p: usize| commit[p].min(level) - commit[p].min(floor);
        let amount: f64 = (0..3).map(part).sum();
        let eligible: Vec<usize> = (0..3)
            .filter(|&p| is_live(p) && commit[p] >= level)
            .collect();
        match eligible[..] {
            [] => (0..3).for_each(|p| payoff.constant[p] += part(p)),
            [p] => payoff.constant[p] += amount,
            [p, q] => {
                let term = match (p, q) {
                    (0, 1) => BTN_SB,
                    (0, 2) => BTN_BB,
                    _ => SB_BB,
                };
                payoff.terms[p][term] += amount;
                payoff.constant[q] += amount;
                payoff.terms[q][term] -= amount;
            }
            _ => {
                payoff.terms[0][BTN_SHARE3] += amount;
                payoff.terms[1][SB_SHARE3] += amount;
                payoff.constant[2] += amount;
                payoff.terms[2][BTN_SHARE3] -= amount;
                payoff.terms[2][SB_SHARE3] -= amount;
            }
        }
        floor = level;
    }
    payoff
}
