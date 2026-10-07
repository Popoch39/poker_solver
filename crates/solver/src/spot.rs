use std::fmt;

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

/// A decision situation to solve: who is at the table and with which stacks.
///
/// Stacks are in big blinds, measured before the blinds are posted. A stack
/// may be smaller than its blind: the player then posts it all and is all-in
/// without a decision. Payoffs are in chips (no ICM), which matches a
/// winner-takes-all format.
#[derive(Clone, Debug, PartialEq)]
pub struct Spot {
    /// Indexed by [`Position::index`]; a heads-up spot has no BTN (stack 0).
    stacks: [f64; 3],
}

/// Error returned when a spot cannot be built from the given stacks.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SpotError {
    #[error("the {position} stack must be a positive number of BB, got {stack}")]
    InvalidStack { position: Position, stack: f64 },
    #[error("a spot needs at least two players with chips")]
    TooFewPlayers,
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
        Ok(Spot {
            stacks: [0.0, sb_stack, bb_stack],
        })
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
            [_, _, _] => Ok(Spot { stacks }),
            _ => Err(SpotError::TooFewPlayers),
        }
    }

    /// The seats dealt in, in the order they act preflop.
    pub fn positions(&self) -> Vec<Position> {
        Position::ALL
            .into_iter()
            .filter(|&p| self.stack(p) > 0.0)
            .collect()
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
}
