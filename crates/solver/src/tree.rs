use std::fmt;
use std::str::FromStr;

use crate::spot::Position;

/// Something a player can do at a decision node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Fold,
    /// Move all-in as the first player to put chips in voluntarily.
    Push,
    /// Call an all-in.
    Call,
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Action::Fold => "fold",
            Action::Push => "push",
            Action::Call => "call",
        })
    }
}

/// A decision node of the push/fold tree: one player to act after a given
/// sequence of actions.
///
/// Heads-up, only [`Node::SbOpen`] and [`Node::BbVsSbPush`] exist.
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
}

impl Node {
    /// Every node, in the order they are played.
    pub(crate) const ALL: [Node; 6] = [
        Node::BtnOpen,
        Node::SbVsBtnPush,
        Node::SbOpen,
        Node::BbVsBtnPush,
        Node::BbVsBtnPushSbCall,
        Node::BbVsSbPush,
    ];

    /// The player who acts at this node.
    pub fn actor(self) -> Position {
        match self {
            Node::BtnOpen => Position::Btn,
            Node::SbVsBtnPush | Node::SbOpen => Position::Sb,
            Node::BbVsBtnPush | Node::BbVsBtnPushSbCall | Node::BbVsSbPush => Position::Bb,
        }
    }

    /// The legal actions, the passive one first.
    pub fn actions(self) -> &'static [Action] {
        match self {
            Node::BtnOpen | Node::SbOpen => &[Action::Fold, Action::Push],
            _ => &[Action::Fold, Action::Call],
        }
    }

    /// Short identifier used on the command line and in exports
    /// (`btn-open`, `bb-vs-sb-push`…).
    pub fn id(self) -> &'static str {
        match self {
            Node::BtnOpen => "btn-open",
            Node::SbVsBtnPush => "sb-vs-btn-push",
            Node::SbOpen => "sb-open",
            Node::BbVsBtnPush => "bb-vs-btn-push",
            Node::BbVsBtnPushSbCall => "bb-vs-btn-push-sb-call",
            Node::BbVsSbPush => "bb-vs-sb-push",
        }
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Node::BtnOpen => "BTN open",
            Node::SbVsBtnPush => "SB vs BTN push",
            Node::SbOpen => "SB open",
            Node::BbVsBtnPush => "BB vs BTN push",
            Node::BbVsBtnPushSbCall => "BB vs BTN push and SB call",
            Node::BbVsSbPush => "BB vs SB push",
        })
    }
}

/// Error returned when a string is not a node identifier.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown node {0:?}: expected one of {ids}", ids = Node::ALL.map(Node::id).join(", "))]
pub struct ParseNodeError(String);

impl FromStr for Node {
    type Err = ParseNodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Node::ALL
            .into_iter()
            .find(|node| node.id().eq_ignore_ascii_case(s))
            .ok_or_else(|| ParseNodeError(s.to_owned()))
    }
}
