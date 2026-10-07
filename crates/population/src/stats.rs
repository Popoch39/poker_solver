use nitro_hh::Card;
use nitro_solver::{Action, HandClass, Node};

/// Number of two-card combinations in a deck.
const COMBOS: f64 = 1326.0;

/// What the players did at one node, for one stack bucket.
#[derive(Clone, Debug)]
pub struct NodeStats {
    node: Node,
    /// Indexed like [`Node::actions`].
    counts: Vec<u32>,
    /// Indexed like [`Node::actions`].
    known: Vec<KnownHands>,
}

impl NodeStats {
    pub(crate) fn new(node: Node) -> NodeStats {
        let actions = node.actions().len();
        NodeStats {
            node,
            counts: vec![0; actions],
            known: vec![KnownHands::default(); actions],
        }
    }

    /// Counts one decision, with the player's cards when they are known.
    pub(crate) fn record(&mut self, action: Action, cards: Option<[Card; 2]>) {
        let i = self.index(action).expect("a legal action");
        self.counts[i] += 1;
        if let Some(cards) = cards {
            self.known[i].add(class_of(cards));
        }
    }

    fn index(&self, action: Action) -> Option<usize> {
        self.node.actions().iter().position(|&a| a == action)
    }

    /// Number of decisions observed.
    pub fn sample(&self) -> u32 {
        self.counts.iter().sum()
    }

    /// Number of times `action` was taken (0 if it is not legal here).
    pub fn count(&self, action: Action) -> u32 {
        self.index(action).map_or(0, |i| self.counts[i])
    }

    /// Share of the decisions that took `action`.
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

fn grid_index(class: HandClass) -> usize {
    let (row, col) = class.grid_position();
    row * 13 + col
}

fn class_of(cards: [Card; 2]) -> HandClass {
    let [a, b] = cards;
    let (high, low) = if a.value >= b.value { (a, b) } else { (b, a) };
    let (high, low) = (high.value.to_char(), low.value.to_char());
    let name = if high == low {
        format!("{high}{low}")
    } else if a.suit == b.suit {
        format!("{high}{low}s")
    } else {
        format!("{high}{low}o")
    };
    name.parse().expect("two cards make a hand class")
}
