use std::collections::HashMap;
use std::fmt;

use nitro_hh::{ActionKind, Card, Hand, Street};
use nitro_solver::{Action, Node, Position, Spot};

use crate::bucket::StackBucket;
use crate::stats::NodeStats;

/// The push/fold nodes in the order they are played.
const TREE_ORDER: [Node; 6] = [
    Node::BtnOpen,
    Node::SbVsBtnPush,
    Node::SbOpen,
    Node::BbVsBtnPush,
    Node::BbVsBtnPushSbCall,
    Node::BbVsSbPush,
];

/// Whose decisions a model is built from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Players {
    /// Everyone but the account owner of each hand history: the population
    /// met at the tables.
    Opponents,
    /// The account owner alone, to compare their play with the equilibrium.
    Hero,
}

impl Players {
    fn include(self, hand: &Hand, seat: u8) -> bool {
        let hero = hand.hero == Some(seat);
        match self {
            Players::Opponents => !hero,
            Players::Hero => hero,
        }
    }
}

/// Action frequencies per push/fold node and stack bucket, built from hand
/// histories without keeping anything that identifies a player.
#[derive(Clone, Debug, Default)]
pub struct PopulationModel {
    nodes: HashMap<(Node, StackBucket), NodeStats>,
    off_tree: OffTree,
    hands: usize,
    hands_in_tree: usize,
    hands_off_tree: usize,
}

impl PopulationModel {
    pub fn build<'a>(hands: impl IntoIterator<Item = &'a Hand>, players: Players) -> Self {
        let mut model = PopulationModel::default();
        for hand in hands {
            model.add(hand, players);
        }
        model
    }

    /// What was observed at `node` for effective stacks in `bucket`, if the
    /// node was reached there at least once.
    pub fn get(&self, node: Node, bucket: StackBucket) -> Option<&NodeStats> {
        self.nodes.get(&(node, bucket))
    }

    /// Every (node, bucket) observed at least once, in the order the nodes
    /// are played, then from the shortest stacks to the deepest.
    pub fn entries(&self) -> impl Iterator<Item = (Node, StackBucket, &NodeStats)> {
        TREE_ORDER.into_iter().flat_map(move |node| {
            StackBucket::all()
                .filter_map(move |bucket| self.get(node, bucket).map(|stats| (node, bucket, stats)))
        })
    }

    /// Locks into `spot` the nodes where the population plays against
    /// `hero` (node-locking): every node of another position observed at
    /// least `min_sample` times in the bucket of the spot's effective stack,
    /// on [`NodeStats::strategy`]. The hero's nodes stay free, for the
    /// solver to find the exploit; so do the nodes sampled too little to be
    /// trusted.
    pub fn lock(&self, spot: Spot, hero: Position, min_sample: u32) -> Spot {
        let bucket = StackBucket::of(spot.effective_stack());
        TREE_ORDER
            .into_iter()
            .filter(|node| node.actor() != hero)
            .fold(spot, |spot, node| match self.get(node, bucket) {
                Some(stats) if stats.sample() >= min_sample => spot
                    .lock(node, |hand| stats.strategy(hand))
                    .expect("a population strategy is a valid lock"),
                _ => spot,
            })
    }

    /// Decisions that are not part of the push/fold tree.
    pub fn off_tree(&self) -> &OffTree {
        &self.off_tree
    }

    /// Number of hands read, including those whose seats could not be
    /// placed (neither in nor off the tree).
    pub fn hands(&self) -> usize {
        self.hands
    }

    /// Hands played preflop entirely within the push/fold tree.
    pub fn hands_in_tree(&self) -> usize {
        self.hands_in_tree
    }

    /// Hands where someone limped, raised short of all-in or did anything
    /// else the push/fold tree has no place for.
    pub fn hands_off_tree(&self) -> usize {
        self.hands_off_tree
    }

    fn add(&mut self, hand: &Hand, players: Players) {
        self.hands += 1;
        let Some(table) = Table::of(hand) else {
            return;
        };
        let bucket = StackBucket::of(table.spot.effective_stack());
        let mut walk = Walk::new(&table, hand.big_blind);
        let mut left_tree = false;
        for action in &hand.actions {
            let Some(position) = table.position(action.seat) else {
                continue;
            };
            let included = players.include(hand, action.seat);
            if action.street != Street::Preflop {
                self.off_tree.postflop += u32::from(included);
                continue;
            }
            // The blinds come first, so every action after leaving is a decision.
            if left_tree {
                self.off_tree.later_preflop += u32::from(included);
                continue;
            }
            match walk.step(position, action.kind, action.all_in) {
                None => {}
                Some(Step::Decision(node, choice)) if included => self
                    .nodes
                    .entry((node, bucket))
                    .or_insert_with(|| NodeStats::new(node))
                    .record(choice, cards(hand, action.seat)),
                Some(Step::Decision(..)) => {}
                Some(Step::LeftTree(kind)) => {
                    left_tree = true;
                    if included {
                        *self.off_tree.left.entry((position, kind)).or_default() += 1;
                    }
                }
            }
        }
        if left_tree {
            self.hands_off_tree += 1;
        } else {
            self.hands_in_tree += 1;
        }
    }
}

/// How a preflop action left the push/fold tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OffTreeAction {
    /// Called the big blind without raising.
    Limp,
    /// Raised to twice the big blind, short of all-in.
    MinRaise,
    /// Any other raise short of all-in.
    Raise,
    /// Anything else the tree has no place for.
    Other,
}

impl fmt::Display for OffTreeAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            OffTreeAction::Limp => "limp",
            OffTreeAction::MinRaise => "min-raise",
            OffTreeAction::Raise => "raise",
            OffTreeAction::Other => "other",
        })
    }
}

/// Decisions outside the push/fold tree, counted apart from its nodes.
#[derive(Clone, Debug, Default)]
pub struct OffTree {
    left: HashMap<(Position, OffTreeAction), u32>,
    later_preflop: u32,
    postflop: u32,
}

impl OffTree {
    /// Decisions from `position` that took the hand out of the tree with
    /// `action`.
    pub fn count(&self, position: Position, action: OffTreeAction) -> u32 {
        self.left.get(&(position, action)).copied().unwrap_or(0)
    }

    /// Preflop decisions taken after the hand had left the tree.
    pub fn later_preflop(&self) -> u32 {
        self.later_preflop
    }

    /// Decisions on the flop, turn and river.
    pub fn postflop(&self) -> u32 {
        self.postflop
    }
}

fn cards(hand: &Hand, seat: u8) -> Option<[Card; 2]> {
    hand.players
        .iter()
        .find(|p| p.seat == seat)
        .and_then(|p| p.cards)
}

/// Positions and stacks of the players dealt in.
struct Table {
    /// `(seat, position, stack in chips)`.
    seats: Vec<(u8, Position, u32)>,
    spot: Spot,
}

impl Table {
    /// `None` when the blinds do not tell who sits where.
    fn of(hand: &Hand) -> Option<Table> {
        let posted = |blind: fn(&ActionKind) -> bool| {
            hand.actions
                .iter()
                .find(|a| a.street == Street::Preflop && blind(&a.kind))
                .map(|a| a.seat)
        };
        let sb = posted(|k| matches!(k, ActionKind::SmallBlind(_)))?;
        let bb = posted(|k| matches!(k, ActionKind::BigBlind(_)))?;
        if sb == bb {
            return None;
        }
        let seats: Vec<(u8, Position, u32)> = hand
            .players
            .iter()
            .map(|p| {
                let position = match p.seat {
                    s if s == sb => Position::Sb,
                    s if s == bb => Position::Bb,
                    _ => Position::Btn,
                };
                (p.seat, position, p.stack)
            })
            .collect();
        let in_bb = |position: Position| {
            seats
                .iter()
                .find(|&&(_, p, _)| p == position)
                .map(|&(_, _, stack)| f64::from(stack) / f64::from(hand.big_blind))
        };
        let spot = match seats.len() {
            2 => Spot::heads_up(in_bb(Position::Sb)?, in_bb(Position::Bb)?),
            3 => Spot::three_max(
                in_bb(Position::Btn)?,
                in_bb(Position::Sb)?,
                in_bb(Position::Bb)?,
            ),
            _ => return None,
        }
        .ok()?;
        Some(Table { seats, spot })
    }

    fn position(&self, seat: u8) -> Option<Position> {
        self.seats
            .iter()
            .find(|&&(s, _, _)| s == seat)
            .map(|&(_, p, _)| p)
    }

    fn stack(&self, position: Position) -> u32 {
        self.seats
            .iter()
            .find(|&&(_, p, _)| p == position)
            .map_or(0, |&(_, _, stack)| stack)
    }
}

/// One preflop action, placed in the push/fold tree.
enum Step {
    Decision(Node, Action),
    LeftTree(OffTreeAction),
}

/// Follows a hand's preflop actions down the push/fold tree.
struct Walk<'a> {
    table: &'a Table,
    big_blind: u32,
    /// Chips put in so far, per position.
    committed: [u32; 3],
    folded: [bool; 3],
    /// Who moved all-in first, then who called, in order.
    all_in: Vec<Position>,
}

impl<'a> Walk<'a> {
    fn new(table: &'a Table, big_blind: u32) -> Walk<'a> {
        Walk {
            table,
            big_blind,
            committed: [0; 3],
            folded: [false; 3],
            all_in: Vec::new(),
        }
    }

    /// Places one action; `None` for an action that is not a decision (a
    /// blind).
    fn step(&mut self, position: Position, kind: ActionKind, all_in: bool) -> Option<Step> {
        let i = index(position);
        let total = match kind {
            ActionKind::SmallBlind(n) | ActionKind::BigBlind(n) => {
                self.committed[i] += n;
                return None;
            }
            ActionKind::Call(n) | ActionKind::Bet(n) => self.committed[i] + n,
            ActionKind::Raise { to } => to,
            ActionKind::Fold | ActionKind::Check => self.committed[i],
        };
        let Some(node) = self.node(position) else {
            return Some(Step::LeftTree(OffTreeAction::Other));
        };
        // A raise, or a call of a short blind, that puts every opponent
        // all-in is a push: the tree's all-in is for the effective stack.
        let covers = all_in || self.covers(position, total);
        let facing_push = !self.all_in.is_empty();
        let action = match kind {
            ActionKind::Fold => Action::Fold,
            // A check facing a push only happens with nothing left to add:
            // the player stays in, as if calling.
            ActionKind::Call(_) | ActionKind::Check if facing_push => Action::Call,
            ActionKind::Raise { .. } if facing_push && covers => Action::Call,
            ActionKind::Call(_) | ActionKind::Raise { .. } if covers => Action::Push,
            ActionKind::Call(_) if !facing_push => {
                return Some(Step::LeftTree(OffTreeAction::Limp));
            }
            ActionKind::Raise { to } if !facing_push && to == 2 * self.big_blind => {
                return Some(Step::LeftTree(OffTreeAction::MinRaise));
            }
            ActionKind::Raise { .. } => return Some(Step::LeftTree(OffTreeAction::Raise)),
            _ => return Some(Step::LeftTree(OffTreeAction::Other)),
        };
        self.committed[i] = total;
        if action == Action::Fold {
            self.folded[i] = true;
        } else {
            self.all_in.push(position);
        }
        Some(Step::Decision(node, action))
    }

    /// Whether `total` chips from `position` put every other player still in
    /// the hand all-in.
    fn covers(&self, position: Position, total: u32) -> bool {
        [Position::Btn, Position::Sb, Position::Bb]
            .into_iter()
            .filter(|&p| p != position && !self.folded[index(p)])
            .all(|p| self.table.stack(p) <= total)
    }

    /// The node `position` acts at, given the actions so far.
    fn node(&self, position: Position) -> Option<Node> {
        let sb_called = self.all_in.get(1) == Some(&Position::Sb);
        match (self.all_in.first(), position) {
            (None, Position::Btn) => Some(Node::BtnOpen),
            (None, Position::Sb) => Some(Node::SbOpen),
            (Some(Position::Btn), Position::Sb) => Some(Node::SbVsBtnPush),
            (Some(Position::Btn), Position::Bb) if sb_called => Some(Node::BbVsBtnPushSbCall),
            (Some(Position::Btn), Position::Bb) => Some(Node::BbVsBtnPush),
            (Some(Position::Sb), Position::Bb) => Some(Node::BbVsSbPush),
            _ => None,
        }
    }
}

fn index(position: Position) -> usize {
    match position {
        Position::Btn => 0,
        Position::Sb => 1,
        Position::Bb => 2,
    }
}
