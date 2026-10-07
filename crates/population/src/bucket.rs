use std::fmt;

/// Upper bounds of the buckets, in BB; the last bucket has none.
const UPPER: [f64; 7] = [2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 15.0];

/// A range of effective stacks, in BB, over which frequencies are pooled.
///
/// Buckets include their upper bound: `8–10 BB` holds stacks above 8 BB up
/// to 10 BB. They are 2 BB wide where push/fold play changes quickly, then
/// `12–15 BB` up to the starting stack of an Expresso Nitro and one bucket
/// above it (reached only heads-up or at the first level).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StackBucket(u8);

impl StackBucket {
    /// Every bucket, from the shortest stacks to the deepest.
    pub fn all() -> impl Iterator<Item = StackBucket> {
        (0..=UPPER.len() as u8).map(StackBucket)
    }

    /// The bucket of an effective stack of `bb` big blinds.
    pub fn of(bb: f64) -> StackBucket {
        StackBucket(UPPER.iter().take_while(|&&upper| bb > upper).count() as u8)
    }

    /// Lowest stack of the bucket, in BB (excluded, except for 0).
    pub fn lower(self) -> f64 {
        match self.0 {
            0 => 0.0,
            i => UPPER[i as usize - 1],
        }
    }

    /// Highest stack of the bucket, in BB (included), if it has one.
    pub fn upper(self) -> Option<f64> {
        UPPER.get(self.0 as usize).copied()
    }
}

impl fmt::Display for StackBucket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.upper() {
            Some(upper) => write!(f, "{}–{upper} BB", self.lower()),
            None => write!(f, ">{} BB", self.lower()),
        }
    }
}
