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

/// A decision node of the action tree: one player to act after a given
/// sequence of actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Node {
    /// The SB acts first: push or fold.
    SbOpen,
    /// The BB faces the SB's push: call or fold.
    BbVsSbPush,
}

impl Node {
    /// The player who acts at this node.
    pub fn actor(self) -> Position {
        match self {
            Node::SbOpen => Position::Sb,
            Node::BbVsSbPush => Position::Bb,
        }
    }

    /// The legal actions, the passive one first.
    pub fn actions(self) -> &'static [Action] {
        match self {
            Node::SbOpen => &[Action::Fold, Action::Push],
            Node::BbVsSbPush => &[Action::Fold, Action::Call],
        }
    }

    /// Short identifier used on the command line (`sb-open`, `bb-vs-sb-push`).
    pub fn id(self) -> &'static str {
        match self {
            Node::SbOpen => "sb-open",
            Node::BbVsSbPush => "bb-vs-sb-push",
        }
    }

    const ALL: [Node; 2] = [Node::SbOpen, Node::BbVsSbPush];
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Node::SbOpen => "SB open",
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
