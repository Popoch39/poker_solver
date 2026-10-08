use std::sync::OnceLock;

use nitro_hh::Card;
use nitro_solver::{Action, HandClass, Node, Strategy};

/// Number of two-card combinations in a deck.
const COMBOS: f64 = 1326.0;

/// The actions counted at an opening node: every first-in decision.
const FIRST_IN: [Action; 4] = [Action::Fold, Action::Limp, Action::Raise, Action::Push];

/// What the players did at one node, for one stack bucket.
#[derive(Clone, Debug)]
pub struct NodeStats {
    node: Node,
    /// Indexed like [`NodeStats::actions`].
    counts: Vec<u32>,
    /// Indexed like [`NodeStats::actions`].
    known: Vec<KnownHands>,
    other_raises: u32,
}

impl NodeStats {
    pub(crate) fn new(node: Node) -> NodeStats {
        let actions = actions_of(node).len();
        NodeStats {
            node,
            counts: vec![0; actions],
            known: vec![KnownHands::default(); actions],
            other_raises: 0,
        }
    }

    /// Counts one decision, with the player's cards when they are known.
    pub(crate) fn record(&mut self, action: Action, cards: Option<[Card; 2]>) {
        let i = self.index(action).expect("an action counted at the node");
        self.counts[i] += 1;
        if let Some([a, b]) = cards {
            self.known[i].add(HandClass::from_cards(a, b));
        }
    }

    /// Counts one raise short of all-in, first in, that is not a min-raise.
    pub(crate) fn record_other_raise(&mut self) {
        self.other_raises += 1;
    }

    fn index(&self, action: Action) -> Option<usize> {
        self.actions().iter().position(|&a| a == action)
    }

    /// The actions counted at the node: its push/fold actions, and at an
    /// opening node (BTN or SB first in) every first-in action, `fold`,
    /// `limp`, `raise` (the min-raise) and `push`.
    pub fn actions(&self) -> &'static [Action] {
        actions_of(self.node)
    }

    /// Number of decisions observed, other raises included.
    pub fn sample(&self) -> u32 {
        self.counts.iter().sum::<u32>() + self.other_raises
    }

    /// Number of times `action` was taken (0 if it is not counted here).
    pub fn count(&self, action: Action) -> u32 {
        self.index(action).map_or(0, |i| self.counts[i])
    }

    /// Raises short of all-in, first in, to more than a min-raise: the
    /// share of the sample that no action of [`NodeStats::actions`] counts.
    pub fn other_raises(&self) -> u32 {
        self.other_raises
    }

    /// Share of all the decisions observed that took `action`.
    pub fn frequency(&self, action: Action) -> f64 {
        f64::from(self.count(action)) / f64::from(self.sample())
    }

    /// The hands whose cards were known when `action` was taken: shown at
    /// showdown for opponents, every hand for the account owner.
    pub fn known_hands(&self, action: Action) -> &KnownHands {
        static NONE: KnownHands = KnownHands { counts: Vec::new() };
        self.index(action).map_or(&NONE, |i| &self.known[i])
    }

    /// Estimated share of the `class` hands that take `action` here, by
    /// Bayes' rule: P(class | action) · P(action) / P(class), where
    /// P(class | action) is read from the known hands and P(class) is the
    /// class's share of the 1 326 combos. Capped at 1, since small samples
    /// can overshoot.
    ///
    /// It assumes the known hands are a fair draw of the action's range,
    /// which showdowns are not quite: a push is only shown when called.
    /// `None` when no hand was known for the action (folds are never shown).
    pub fn estimated_frequency(&self, action: Action, class: HandClass) -> Option<f64> {
        let known = self.known_hands(action);
        if known.total() == 0 {
            return None;
        }
        let given_action = f64::from(known.count(class)) / f64::from(known.total());
        let prior = f64::from(class.combos()) / COMBOS;
        Some((given_action * self.frequency(action) / prior).min(1.0))
    }

    /// The population's strategy for `hand` at this node, as it is locked
    /// into the solver's push/fold tree: the strongest hands by all-in
    /// equity against a random hand take the aggressive action (push or
    /// call) until their combos make up the share of the decisions that did
    /// not fold, the class at the boundary mixing; every other hand folds.
    ///
    /// First in, a limp or a raise short of all-in counts as a push: it is
    /// how the fallback policy of the push/fold strategies reads it (ADR
    /// 0007), the hero answering it as a push and the player who made it
    /// calling whatever comes next. The bots enter with these same hands.
    ///
    /// The hands shown at showdown are left out on purpose. Spread over 169
    /// classes they are too few to lock class by class (at most a few
    /// hundred per node and bucket on the NitroVariance dataset), so the
    /// exploit would chase noise; and they are biased, a push being shown
    /// only when called. [`NodeStats::estimated_frequency`] remains to
    /// compare.
    pub fn strategy(&self, hand: HandClass) -> Strategy {
        let [passive, aggressive] = self.node.actions() else {
            unreachable!("every push/fold node has two actions")
        };
        let share = 1.0 - self.frequency(*passive);
        let mut stronger = 0.0;
        for &class in strength_order() {
            let combos = f64::from(class.combos()) / COMBOS;
            if class == hand {
                let f = ((share - stronger) / combos).clamp(0.0, 1.0);
                return Strategy::new([(*passive, 1.0 - f), (*aggressive, f)]);
            }
            stronger += combos;
        }
        unreachable!("the order lists every class")
    }
}

/// The actions counted at `node`: see [`NodeStats::actions`].
fn actions_of(node: Node) -> &'static [Action] {
    match node {
        Node::BtnOpen | Node::SbOpen => &FIRST_IN,
        _ => node.actions(),
    }
}

/// Every hand class from the strongest to the weakest by all-in equity
/// against a random hand.
fn strength_order() -> &'static [HandClass] {
    static ORDER: OnceLock<Vec<HandClass>> = OnceLock::new();
    ORDER.get_or_init(|| {
        let mut order: Vec<(HandClass, f64)> = HandClass::all()
            .map(|hand| (hand, hand.equity_vs_random()))
            .collect();
        order.sort_by(|a, b| b.1.total_cmp(&a.1));
        order.into_iter().map(|(hand, _)| hand).collect()
    })
}

/// How many times each hand class was seen, among hands whose cards were
/// known.
#[derive(Clone, Debug, Default)]
pub struct KnownHands {
    /// Indexed by grid position; empty until a hand is added.
    counts: Vec<u32>,
}

impl KnownHands {
    fn add(&mut self, class: HandClass) {
        if self.counts.is_empty() {
            self.counts = vec![0; 169];
        }
        self.counts[grid_index(class)] += 1;
    }

    pub fn total(&self) -> u32 {
        self.counts.iter().sum()
    }

    pub fn count(&self, class: HandClass) -> u32 {
        self.counts.get(grid_index(class)).copied().unwrap_or(0)
    }
}

pub(crate) fn grid_index(class: HandClass) -> usize {
    let (row, col) = class.grid_position();
    row * 13 + col
}
