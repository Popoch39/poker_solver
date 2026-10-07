use std::fmt;

/// A seat at the table, named by its preflop position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Position {
    /// Small blind: posts 0.5 BB.
    Sb,
    /// Big blind: posts 1 BB.
    Bb,
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Position::Sb => "SB",
            Position::Bb => "BB",
        })
    }
}

/// A decision situation to solve: who is at the table and with which stacks.
///
/// Stacks are in big blinds, measured before the blinds are posted. Payoffs
/// are in chips (no ICM), which matches a winner-takes-all format.
#[derive(Clone, Debug, PartialEq)]
pub struct Spot {
    sb_stack: f64,
    bb_stack: f64,
}

/// Error returned when a spot cannot be built from the given stacks.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SpotError {
    #[error("the {position} stack must be a number of at least 1 BB, got {stack}")]
    StackTooShort { position: Position, stack: f64 },
}

impl Spot {
    /// Heads-up push/fold: the SB moves all-in or folds, then the BB calls or
    /// folds.
    ///
    /// Stacks below 1 BB leave a player without a real decision; they are
    /// rejected rather than modelled.
    pub fn heads_up(sb_stack: f64, bb_stack: f64) -> Result<Spot, SpotError> {
        for (position, stack) in [(Position::Sb, sb_stack), (Position::Bb, bb_stack)] {
            // Written so that NaN is rejected too.
            if !(stack >= 1.0 && stack.is_finite()) {
                return Err(SpotError::StackTooShort { position, stack });
            }
        }
        Ok(Spot { sb_stack, bb_stack })
    }

    /// Stack of the player at `position`, in BB.
    pub fn stack(&self, position: Position) -> f64 {
        match position {
            Position::Sb => self.sb_stack,
            Position::Bb => self.bb_stack,
        }
    }

    /// The smaller of the two stacks: the most that can change hands.
    pub fn effective_stack(&self) -> f64 {
        self.sb_stack.min(self.bb_stack)
    }
}
