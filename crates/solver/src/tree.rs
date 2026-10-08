use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;
use std::sync::{Mutex, OnceLock};

use crate::spot::Position;

/// Something a player can do at a decision node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Fold,
    /// Move all-in: first in, or as a raise over limps or a min-raise.
    Push,
    /// Call an all-in or a min-raise. Facing an all-in, the caller commits
    /// its whole stack, as in push/fold: what nobody matches comes back.
    Call,
    /// Put in one big blind while nobody has raised: call the big blind, or
    /// complete from the small blind.
    Limp,
    /// Min-raise: raise to two big blinds while nobody has raised yet.
    Raise,
    /// Stay in for free: the BB when the others only limped or folded.
    Check,
}

impl Action {
    const ALL: [Action; 6] = [
        Action::Fold,
        Action::Push,
        Action::Call,
        Action::Limp,
        Action::Raise,
        Action::Check,
    ];
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Action::Fold => "fold",
            Action::Push => "push",
            Action::Call => "call",
            Action::Limp => "limp",
            Action::Raise => "raise",
            Action::Check => "check",
        })
    }
}

/// A decision node of the preflop tree: one player to act after a given
/// sequence of actions.
///
/// The six named nodes make up the push/fold tree; heads-up, only
/// [`Node::SbOpen`] and [`Node::BbVsSbPush`] exist. A spot that allows
/// limps or min-raises has more nodes, all of them [`Node::Line`]s.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Node {
    /// The BTN acts first: push or fold.
    BtnOpen,
    /// The SB faces the BTN's push: call or fold.
    SbVsBtnPush,
    /// The SB acts first (the BTN folded, or heads-up): push or fold.
    SbOpen,
    /// The BB faces the BTN's push, the SB having folded: call or fold.
    BbVsBtnPush,
    /// The BB faces the BTN's push and the SB's call: call or fold.
    BbVsBtnPushSbCall,
    /// The BB faces the SB's push (the BTN folded, or heads-up): call or fold.
    BbVsSbPush,
    /// Any other node, reached through a limp or a min-raise
    /// (`bb-vs-sb-limp`, `btn-vs-btn-limp-bb-raise`…).
    Line(Line),
}

/// Most voluntary actions before a decision: two limps, a min-raise, two
/// calls of it and a push leave at most one more call before the last one.
const MAX_STEPS: usize = 8;

/// Who acts at a node, after which voluntary actions.
///
/// Folds are left out: the order of play (BTN, SB, BB, then round again)
/// implies them, since a player still in who is skipped must have folded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Line {
    actor: Position,
    len: u8,
    /// The first `len` entries; the rest stay at a fixed filler so that
    /// equal lines compare and hash equal.
    steps: [(Position, Action); MAX_STEPS],
}

const FILLER: (Position, Action) = (Position::Btn, Action::Fold);

impl Line {
    fn new(actor: Position, steps: &[(Position, Action)]) -> Line {
        assert!(
            steps.len() <= MAX_STEPS,
            "a line has at most {MAX_STEPS} steps"
        );
        let mut line = Line {
            actor,
            len: steps.len() as u8,
            steps: [FILLER; MAX_STEPS],
        };
        line.steps[..steps.len()].copy_from_slice(steps);
        line
    }

    fn steps(&self) -> &[(Position, Action)] {
        &self.steps[..self.len as usize]
    }
}

impl Node {
    /// The push/fold nodes, in the order they are played.
    pub(crate) const ALL: [Node; 6] = [
        Node::BtnOpen,
        Node::SbVsBtnPush,
        Node::SbOpen,
        Node::BbVsBtnPush,
        Node::BbVsBtnPushSbCall,
        Node::BbVsSbPush,
    ];

    /// The node where `actor` acts after the voluntary actions `steps`
    /// (folds left out), as one of the named push/fold nodes when it is one.
    pub(crate) fn new(actor: Position, steps: &[(Position, Action)]) -> Node {
        let line = Line::new(actor, steps);
        Node::ALL
            .into_iter()
            .find(|n| n.line() == line)
            .unwrap_or(Node::Line(line))
    }

    fn line(self) -> Line {
        use Action::{Call, Push};
        use Position::{Bb, Btn, Sb};
        match self {
            Node::BtnOpen => Line::new(Btn, &[]),
            Node::SbVsBtnPush => Line::new(Sb, &[(Btn, Push)]),
            Node::SbOpen => Line::new(Sb, &[]),
            Node::BbVsBtnPush => Line::new(Bb, &[(Btn, Push)]),
            Node::BbVsBtnPushSbCall => Line::new(Bb, &[(Btn, Push), (Sb, Call)]),
            Node::BbVsSbPush => Line::new(Bb, &[(Sb, Push)]),
            Node::Line(line) => line,
        }
    }

    /// The player who acts at this node.
    pub fn actor(self) -> Position {
        self.line().actor
    }

    /// The legal actions in a push/fold tree, the passive one first.
    ///
    /// A [`Node::Line`] gives its actions when limps and min-raises are both
    /// allowed and every stack is deep. A spot can have fewer: limp or
    /// min-raise not allowed, or a stack too short for them, and then the
    /// opening nodes have more than in push/fold. The actions a solved spot
    /// really has are given by [`crate::Solution::actions`].
    pub fn actions(self) -> &'static [Action] {
        use Action::{Call, Check, Fold, Limp, Push, Raise};
        let line = match self {
            Node::BtnOpen | Node::SbOpen => return &[Fold, Push],
            Node::Line(line) => line,
            _ => return &[Fold, Call],
        };
        let steps = line.steps();
        if steps.iter().any(|&(_, a)| a == Push) {
            &[Fold, Call]
        } else if steps.iter().any(|&(_, a)| a == Raise) {
            &[Fold, Call, Push]
        } else if line.actor == Position::Bb {
            &[Check, Raise, Push]
        } else {
            &[Fold, Limp, Raise, Push]
        }
    }

    /// The last and most aggressive of [`Node::actions`]: the push, or the
    /// call when facing one. The push/fold frequency of a node is its share.
    pub fn aggressive_action(self) -> Action {
        *self.actions().last().expect("every node has actions")
    }

    /// Short identifier used on the command line and in exports
    /// (`btn-open`, `bb-vs-sb-push`, `bb-vs-btn-limp-sb-limp`…): the actor,
    /// then `open` or `vs` and each voluntary action before the node.
    pub fn id(self) -> &'static str {
        match self {
            Node::BtnOpen => "btn-open",
            Node::SbVsBtnPush => "sb-vs-btn-push",
            Node::SbOpen => "sb-open",
            Node::BbVsBtnPush => "bb-vs-btn-push",
            Node::BbVsBtnPushSbCall => "bb-vs-btn-push-sb-call",
            Node::BbVsSbPush => "bb-vs-sb-push",
            Node::Line(line) => intern(line_id(&line)),
        }
    }
}

/// A position as it appears in node identifiers.
fn token(position: Position) -> &'static str {
    match position {
        Position::Btn => "btn",
        Position::Sb => "sb",
        Position::Bb => "bb",
    }
}

fn line_id(line: &Line) -> String {
    let mut id = token(line.actor).to_owned();
    if line.steps().is_empty() {
        id.push_str("-open");
    } else {
        id.push_str("-vs");
        for &(position, action) in line.steps() {
            id.push_str(&format!("-{}-{action}", token(position)));
        }
    }
    id
}

/// Keeps [`Node::id`] a `&'static str` for every node. Spots have finitely
/// many distinct lines (a few hundred), so the set stays small.
fn intern(id: String) -> &'static str {
    static IDS: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut ids = IDS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match ids.get(id.as_str()) {
        Some(&known) => known,
        None => {
            let leaked: &'static str = Box::leak(id.into_boxed_str());
            ids.insert(leaked);
            leaked
        }
    }
}

impl fmt::Display for Node {
    /// `BTN open`, `BB vs BTN push and SB call`, `BTN vs BTN limp, SB limp
    /// and BB raise`…
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let line = self.line();
        let steps = line.steps();
        if steps.is_empty() {
            return write!(f, "{} open", line.actor);
        }
        write!(f, "{} vs ", line.actor)?;
        for (i, (position, action)) in steps.iter().enumerate() {
            let separator = match i {
                0 => "",
                _ if i + 1 == steps.len() => " and ",
                _ => ", ",
            };
            write!(f, "{separator}{position} {action}")?;
        }
        Ok(())
    }
}

/// Error returned when a string is not a node identifier.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "unknown node {0:?}: expected the actor then `open`, or `vs` and the actions before it \
     (btn-open, bb-vs-btn-push-sb-call, bb-vs-btn-limp-sb-limp…)"
)]
pub struct ParseNodeError(String);

impl FromStr for Node {
    type Err = ParseNodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseNodeError(s.to_owned());
        let lower = s.to_ascii_lowercase();
        let position = |t: &str| {
            Position::ALL
                .into_iter()
                .find(|&p| token(p) == t)
                .ok_or_else(err)
        };
        let tokens: Vec<&str> = lower.split('-').collect();
        let (actor, rest) = match tokens[..] {
            [actor, "open"] => return Ok(Node::new(position(actor)?, &[])),
            [actor, "vs", ref rest @ ..] if !rest.is_empty() && rest.len() % 2 == 0 => {
                (position(actor)?, rest)
            }
            _ => return Err(err()),
        };
        let steps = rest
            .chunks(2)
            .map(|pair| {
                let action = Action::ALL
                    .into_iter()
                    .find(|a| a.to_string() == pair[1])
                    // Folds and checks never appear in a line.
                    .filter(|a| !matches!(a, Action::Fold | Action::Check))
                    .ok_or_else(err)?;
                Ok((position(pair[0])?, action))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if steps.len() > MAX_STEPS {
            return Err(err());
        }
        Ok(Node::new(actor, &steps))
    }
}
