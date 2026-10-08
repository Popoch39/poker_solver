//! A seat's decision point read as a node of the solver's push/fold tree,
//! with the fallback policy for what the tree does not cover. The solver's
//! hero and the population bots share it, so that they meet the same tree.
//!
//! # Fallback policy (see ADR 0006)
//!
//! - **Postflop**, and **preflop once the player has put chips in
//!   voluntarily** (it called a push and someone moved in over it): check
//!   or call, to showdown. The push/fold tree values every call as a
//!   showdown, so the hand is played out as the tree priced it.
//! - **A limp or a raise short of all-in** counts as a push of the same
//!   player: the player answers at the node facing that push, its call
//!   becoming an all-in (an isolation) and its fold a check when there is
//!   nothing to call. Ticket #5 brings limps and min-raises into the tree.
//! - **A node missing from the solved tree** (the player is all-in from the
//!   blind, or the stacks rounding moved the blind's all-in) is a call: such
//!   a player has nothing left to decide.

use nitro_solver::{Action, HandClass, Node, Spot};

use crate::seat::{Card, Decision, PlayerView, Position, SeatView, Street};

/// What a seat's decision point is, for a push/fold strategy.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Reading {
    /// A decision of the push/fold tree.
    Tree(TreeDecision),
    /// Off the tree: check or call down to showdown.
    CallDown,
}

/// A decision point placed in the push/fold tree.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TreeDecision {
    /// The players dealt in, with their stacks before the blinds, in BB.
    pub(crate) spot: Spot,
    pub(crate) node: Node,
    pub(crate) hand: HandClass,
    /// Someone the player answers is all-in: its aggressive action is then
    /// a call, not an all-in of its own.
    facing_all_in: bool,
    can_go_all_in: bool,
}

impl TreeDecision {
    /// The simulator's decision for an action of the tree.
    pub(crate) fn decision(&self, action: Action) -> Decision {
        // The spots read here are push/fold; the simulator has no bet size
        // short of all-in, so a min-raise could only be played as one.
        match action {
            Action::Fold => Decision::Fold,
            Action::Check | Action::Limp => Decision::Call,
            Action::Push | Action::Call | Action::Raise
                if self.facing_all_in || !self.can_go_all_in =>
            {
                Decision::Call
            }
            Action::Push | Action::Call | Action::Raise => Decision::AllIn,
        }
    }
}

/// Places `view` in the push/fold tree, or says it is off the tree.
pub(crate) fn read(view: &SeatView) -> Reading {
    if view.street != Street::Preflop {
        return Reading::CallDown;
    }
    let blind = |p: &PlayerView| match p.position {
        Position::SmallBlind => view.small_blind,
        Position::BigBlind => view.big_blind,
        Position::Button => 0.0,
    };
    // Chips put in beyond the blind, all-in from the blind excepted.
    let voluntary = |p: &PlayerView| !p.folded && p.street_bet > blind(p);
    let at = |position: Position| view.players.iter().find(|p| p.position == position);
    let Some(me) = view.players.iter().find(|p| p.seat == view.seat) else {
        return Reading::CallDown;
    };
    if voluntary(me) {
        return Reading::CallDown;
    }
    let btn_in = at(Position::Button).is_some_and(voluntary);
    let sb_in = at(Position::SmallBlind).is_some_and(voluntary);
    let node = match (view.position, btn_in, sb_in) {
        (Position::Button, _, _) => Node::BtnOpen,
        (Position::SmallBlind, true, _) => Node::SbVsBtnPush,
        (Position::SmallBlind, false, _) => Node::SbOpen,
        (Position::BigBlind, true, true) => Node::BbVsBtnPushSbCall,
        (Position::BigBlind, true, false) => Node::BbVsBtnPush,
        (Position::BigBlind, false, true) => Node::BbVsSbPush,
        // Nothing to answer: a walk, or a blind all-in to call for free.
        (Position::BigBlind, false, false) => return Reading::CallDown,
    };
    let in_bb = |position: Position| at(position).map_or(0.0, |p| p.stack + p.street_bet);
    let in_bb = |position: Position| f64::from(in_bb(position) / view.big_blind);
    let spot = match view.players.len() {
        2 => Spot::heads_up(in_bb(Position::SmallBlind), in_bb(Position::BigBlind)),
        _ => Spot::three_max(
            in_bb(Position::Button),
            in_bb(Position::SmallBlind),
            in_bb(Position::BigBlind),
        ),
    };
    let Ok(spot) = spot else {
        return Reading::CallDown;
    };
    let facing_all_in = view
        .players
        .iter()
        .any(|p| p.seat != view.seat && p.all_in && voluntary(p));
    Reading::Tree(TreeDecision {
        spot,
        node,
        hand: hand_class(view.hole_cards),
        facing_all_in,
        can_go_all_in: view.legal_decisions().contains(&Decision::AllIn),
    })
}

fn hand_class([a, b]: [Card; 2]) -> HandClass {
    let (high, low) = if a.value >= b.value { (a, b) } else { (b, a) };
    let (high_rank, low_rank) = (high.value.to_char(), low.value.to_char());
    let name = match () {
        _ if high_rank == low_rank => format!("{high_rank}{low_rank}"),
        _ if a.suit == b.suit => format!("{high_rank}{low_rank}s"),
        _ => format!("{high_rank}{low_rank}o"),
    };
    name.parse().expect("two cards make a hand class")
}
