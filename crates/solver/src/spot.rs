use std::fmt;

use crate::betting_tree::BettingTree;
use crate::hand::{HandClass, NUM_CLASSES};
use crate::solution::Strategy;
use crate::tree::{Action, Node};

/// A seat at the table, named by its preflop position, in the order the
/// players act preflop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Position {
    /// Button: acts first preflop in 3-max and posts no blind.
    Btn,
    /// Small blind: posts 0.5 BB.
    Sb,
    /// Big blind: posts 1 BB.
    Bb,
}

impl Position {
    pub(crate) const ALL: [Position; 3] = [Position::Btn, Position::Sb, Position::Bb];

    pub(crate) fn index(self) -> usize {
        self as usize
    }

    /// Chips the seat must post before the cards are dealt, in BB.
    pub(crate) fn blind(self) -> f64 {
        match self {
            Position::Btn => 0.0,
            Position::Sb => 0.5,
            Position::Bb => 1.0,
        }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Position::Btn => "BTN",
            Position::Sb => "SB",
            Position::Bb => "BB",
        })
    }
}

/// A decision situation to solve: who is at the table, with which stacks,
/// which actions are allowed, and which nodes, if any, are locked on a given
/// strategy.
///
/// Stacks are in big blinds, measured before the blinds are posted. A stack
/// may be smaller than its blind: the player then posts it all and is all-in
/// without a decision. Payoffs are in chips (no ICM), which matches a
/// winner-takes-all format.
///
/// A new spot is push/fold. [`Spot::with_limp`] and [`Spot::with_min_raise`]
/// add the other actions; a hand that then reaches the flop with nobody
/// all-in is valued by the equity model (see [`RealizationFactors`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Spot {
    /// Indexed by [`Position::index`]; a heads-up spot has no BTN (stack 0).
    stacks: [f64; 3],
    limp: bool,
    min_raise: bool,
    realization: RealizationFactors,
    locks: Vec<Lock>,
}

/// Facteur de réalisation (see the glossary) of each player who sees the
/// flop, by spot type: how many players see it, and in which position.
///
/// A line that reaches the flop with nobody all-in ends there. Each player
/// still in wins `factor × equity × pot`, where `equity` is the all-in
/// equity of its hand against the others' hands. A factor of 1 for everyone
/// values the flop at raw equity. Factors that average 1 at a table keep an
/// even pot whole; beyond that, the model creates or destroys chips, so a
/// factor is only meaningful next to the others.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RealizationFactors {
    /// Two players at the flop: the one who acts first after it (the SB, or
    /// the BB against the BTN, or the BB heads-up).
    pub out_of_position: f64,
    /// Two players at the flop: the one who acts last after it (the BTN, or
    /// the BB against the SB, or the SB heads-up, where it has the button).
    pub in_position: f64,
    /// Three players at the flop: the SB, who acts first after it.
    pub three_way_sb: f64,
    /// Three players at the flop: the BB, who acts second.
    pub three_way_bb: f64,
    /// Three players at the flop: the BTN, who acts last.
    pub three_way_btn: f64,
}

impl RealizationFactors {
    /// Every hand realizes exactly its equity.
    pub const RAW_EQUITY: RealizationFactors = RealizationFactors {
        out_of_position: 1.0,
        in_position: 1.0,
        three_way_sb: 1.0,
        three_way_bb: 1.0,
        three_way_btn: 1.0,
    };

    fn all(&self) -> [f64; 5] {
        [
            self.out_of_position,
            self.in_position,
            self.three_way_sb,
            self.three_way_bb,
            self.three_way_btn,
        ]
    }
}

impl Default for RealizationFactors {
    /// 0.9 out of position and 1.1 in position heads-up; 0.9, 1.0 and 1.1
    /// three-way from the first to act after the flop to the last.
    ///
    /// These are a starting assumption, not a measurement: the player in
    /// position realizes more than its equity, the one out of position less,
    /// by a tenth either way, and the factors average 1 so that an even pot
    /// keeps its size. Tune them per spot type to match a postflop study.
    fn default() -> Self {
        RealizationFactors {
            out_of_position: 0.9,
            in_position: 1.1,
            three_way_sb: 0.9,
            three_way_bb: 1.0,
            three_way_btn: 1.1,
        }
    }
}

/// A node fixed on a strategy: the frequency of each of the node's actions
/// (in `actions` order, the node's actions in the spot when it was locked)
/// for every hand class, class by class.
#[derive(Clone, Debug, PartialEq)]
struct Lock {
    node: Node,
    actions: &'static [Action],
    frequencies: Vec<f64>,
}

/// Error returned when a spot cannot be built from the given stacks, or a
/// node cannot be locked on the given strategy.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SpotError {
    #[error("the {position} stack must be a positive number of BB, got {stack}")]
    InvalidStack { position: Position, stack: f64 },
    #[error("a spot needs at least two players with chips")]
    TooFewPlayers,
    #[error(
        "the locked strategy of {hand} at {node} must give the node's actions frequencies between 0 and 1 that sum to 1"
    )]
    InvalidLock { node: Node, hand: HandClass },
    #[error("a realization factor must be a non-negative number, got {0}")]
    InvalidRealization(f64),
    #[error("{node} is locked on {action}, which the spot would no longer allow there")]
    LockedActionDisallowed { node: Node, action: Action },
}

impl Spot {
    /// Heads-up push/fold: the SB moves all-in or folds, then the BB calls or
    /// folds.
    pub fn heads_up(sb_stack: f64, bb_stack: f64) -> Result<Spot, SpotError> {
        for (position, stack) in [(Position::Sb, sb_stack), (Position::Bb, bb_stack)] {
            // Written so that NaN is rejected too.
            if !(stack > 0.0 && stack.is_finite()) {
                return Err(SpotError::InvalidStack { position, stack });
            }
        }
        Ok(Spot::push_fold([0.0, sb_stack, bb_stack]))
    }

    fn push_fold(stacks: [f64; 3]) -> Spot {
        Spot {
            stacks,
            limp: false,
            min_raise: false,
            realization: RealizationFactors::default(),
            locks: Vec::new(),
        }
    }

    /// 3-max push/fold: the BTN pushes or folds, then the SB, then the BB,
    /// each calling or folding when facing a push.
    ///
    /// A stack of 0 is an eliminated player: the two others play heads-up,
    /// the one who acts first preflop (BTN before SB before BB) posting the
    /// small blind.
    pub fn three_max(btn_stack: f64, sb_stack: f64, bb_stack: f64) -> Result<Spot, SpotError> {
        let stacks = [btn_stack, sb_stack, bb_stack];
        for (position, stack) in Position::ALL.into_iter().zip(stacks) {
            if !(stack >= 0.0 && stack.is_finite()) {
                return Err(SpotError::InvalidStack { position, stack });
            }
        }
        match stacks
            .iter()
            .filter(|&&s| s > 0.0)
            .copied()
            .collect::<Vec<_>>()[..]
        {
            [sb, bb] => Spot::heads_up(sb, bb),
            [_, _, _] => Ok(Spot::push_fold(stacks)),
            _ => Err(SpotError::TooFewPlayers),
        }
    }

    /// Allows (or not) limping: putting in one big blind while nobody has
    /// raised, then checking from the BB.
    ///
    /// Refused when a locked node limps: the lock would no longer fit the
    /// spot's tree.
    pub fn with_limp(mut self, allowed: bool) -> Result<Spot, SpotError> {
        self.limp = allowed;
        self.check_locks()
    }

    /// Allows (or not) one min-raise per hand, to two big blinds, while
    /// nobody has raised; it can be called, folded to or pushed over.
    ///
    /// Refused when a locked node min-raises: the lock would no longer fit
    /// the spot's tree.
    pub fn with_min_raise(mut self, allowed: bool) -> Result<Spot, SpotError> {
        self.min_raise = allowed;
        self.check_locks()
    }

    /// `self` if every lock only plays actions its node has in this spot.
    fn check_locks(self) -> Result<Spot, SpotError> {
        for lock in &self.locks {
            let actions = self.actions_at(lock.node);
            for row in lock.frequencies.chunks(lock.actions.len()) {
                let mut played = lock.actions.iter().zip(row);
                if let Some((&action, _)) = played.find(|&(a, &f)| f != 0.0 && !actions.contains(a))
                {
                    return Err(SpotError::LockedActionDisallowed {
                        node: lock.node,
                        action,
                    });
                }
            }
        }
        Ok(self)
    }

    /// Sets the realization factors of the equity model that values the
    /// lines reaching the flop.
    pub fn with_realization(mut self, factors: RealizationFactors) -> Result<Spot, SpotError> {
        if let Some(&bad) = factors
            .all()
            .iter()
            .find(|f| !(**f >= 0.0 && f.is_finite()))
        {
            return Err(SpotError::InvalidRealization(bad));
        }
        self.realization = factors;
        Ok(self)
    }

    pub fn allows_limp(&self) -> bool {
        self.limp
    }

    pub fn allows_min_raise(&self) -> bool {
        self.min_raise
    }

    pub fn realization(&self) -> &RealizationFactors {
        &self.realization
    }

    /// Whether some line can reach the flop: limps or min-raises are
    /// allowed and no blind is all-in from the start. A blind all-in leaves
    /// the others nothing to do but fold or go all-in, as in push/fold.
    pub(crate) fn has_flop(&self) -> bool {
        let all_in_blind = self.positions().iter().any(|&p| self.stack(p) <= p.blind());
        (self.limp || self.min_raise) && !all_in_blind
    }

    /// The seats dealt in, in the order they act preflop.
    pub fn positions(&self) -> Vec<Position> {
        Position::ALL
            .into_iter()
            .filter(|&p| self.stack(p) > 0.0)
            .collect()
    }

    /// Whether both spots seat the same stacks, allow the same actions and
    /// value the flop alike, whatever their locks: their trees and payoffs
    /// are then the same.
    pub(crate) fn same_table(&self, other: &Spot) -> bool {
        self.stacks == other.stacks
            && self.limp == other.limp
            && self.min_raise == other.min_raise
            && self.realization == other.realization
    }

    /// The actions of `node` in this spot's tree, or its push/fold actions
    /// ([`Node::actions`]) if the node is not in the tree.
    fn actions_at(&self, node: Node) -> &'static [Action] {
        if !self.has_flop() {
            return node.actions();
        }
        BettingTree::new(self)
            .decisions
            .iter()
            .find(|d| d.node == node)
            .map_or(node.actions(), |d| d.actions)
    }

    pub(crate) fn is_heads_up(&self) -> bool {
        self.stacks[Position::Btn.index()] == 0.0
    }

    /// Stack of the player at `position`, in BB (0 if the seat is empty).
    pub fn stack(&self, position: Position) -> f64 {
        self.stacks[position.index()]
    }

    /// The smallest stack among the players dealt in; heads-up, the most that
    /// can change hands.
    pub fn effective_stack(&self) -> f64 {
        self.positions()
            .into_iter()
            .map(|p| self.stack(p))
            .fold(f64::INFINITY, f64::min)
    }

    /// Locks `node` on `strategy`, the frequencies each hand class plays
    /// there (node-locking): the solver keeps them exactly as given and
    /// re-solves every other node around them. Locking a node again replaces
    /// its strategy; a node outside the spot's tree is never played, so its
    /// lock has no effect.
    ///
    /// The strategy may only use the node's actions in this spot: allow
    /// limps and min-raises before locking the nodes they reach or open up.
    pub fn lock(
        mut self,
        node: Node,
        mut strategy: impl FnMut(HandClass) -> Strategy,
    ) -> Result<Spot, SpotError> {
        let actions = self.actions_at(node);
        let mut frequencies = Vec::with_capacity(NUM_CLASSES * actions.len());
        for hand in HandClass::all() {
            let strategy = strategy(hand);
            let legal = strategy.iter().all(|(a, _)| actions.contains(&a));
            let row: Vec<f64> = actions.iter().map(|&a| strategy.frequency(a)).collect();
            let in_range = row.iter().all(|f| (0.0..=1.0).contains(f));
            if !legal || !in_range || (row.iter().sum::<f64>() - 1.0).abs() > 1e-9 {
                return Err(SpotError::InvalidLock { node, hand });
            }
            frequencies.extend(row);
        }
        self.locks.retain(|lock| lock.node != node);
        self.locks.push(Lock {
            node,
            actions,
            frequencies,
        });
        Ok(self)
    }

    /// Whether `node` is locked on a given strategy.
    pub fn is_locked(&self, node: Node) -> bool {
        self.locks.iter().any(|lock| lock.node == node)
    }

    /// The locked frequencies of `node`, laid out class by class for the
    /// node's `actions` in the solved tree, or `None` if the node is free.
    ///
    /// A lock only plays actions of its node in the spot ([`Spot::lock`],
    /// [`Spot::with_limp`] and [`Spot::with_min_raise`] see to it): the
    /// actions the tree has beyond them get 0.
    pub(crate) fn locked(&self, node: Node, actions: &[Action]) -> Option<Vec<f64>> {
        let lock = self.locks.iter().find(|lock| lock.node == node)?;
        if lock.actions == actions {
            return Some(lock.frequencies.clone());
        }
        let width = lock.actions.len();
        let mut frequencies = Vec::with_capacity(NUM_CLASSES * actions.len());
        for row in lock.frequencies.chunks(width) {
            frequencies.extend(actions.iter().map(|a| {
                lock.actions
                    .iter()
                    .position(|b| b == a)
                    .map_or(0.0, |i| row[i])
            }));
        }
        Some(frequencies)
    }
}
