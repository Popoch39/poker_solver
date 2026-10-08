//! Tournament structure: starting stack, blind levels and level length.

/// Blinds of one level, in chips (Expresso Nitro has no ante).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlindLevel {
    pub small_blind: u32,
    pub big_blind: u32,
}

const fn level(small_blind: u32, big_blind: u32) -> BlindLevel {
    BlindLevel {
        small_blind,
        big_blind,
    }
}

/// The 33 one-minute levels of the Expresso Nitro.
///
/// Source: the `Levels : [...]` line of the Winamax tournament summaries in
/// the NitroVariance dataset (github.com/ThibaultDieudonne/NitroVariance,
/// January–February 2023, read 2026-10-07): identical in all 3 487 Nitro
/// summaries, ante 0 and 60 s at every level. The official page
/// (winamax.fr/expresso, 2026-10-07) only confirms 300 chips, 10/20 and
/// one-minute levels.
pub const NITRO_BLIND_LEVELS: [BlindLevel; 33] = [
    level(10, 20),
    level(15, 30),
    level(20, 40),
    level(30, 60),
    level(40, 80),
    level(50, 100),
    level(60, 120),
    level(80, 160),
    level(100, 200),
    level(125, 250),
    level(150, 300),
    level(200, 400),
    level(250, 500),
    level(300, 600),
    level(400, 800),
    level(500, 1_000),
    level(600, 1_200),
    level(800, 1_600),
    level(1_000, 2_000),
    level(1_250, 2_500),
    level(1_500, 3_000),
    level(2_000, 4_000),
    level(2_500, 5_000),
    level(3_000, 6_000),
    level(4_000, 8_000),
    level(5_000, 10_000),
    level(6_000, 12_000),
    level(8_000, 16_000),
    level(10_000, 20_000),
    level(12_500, 25_000),
    level(15_000, 30_000),
    level(20_000, 40_000),
    level(25_000, 50_000),
];

/// How a game is played out: stacks, blinds and how many hands fit in a
/// level.
///
/// Levels last a fixed time on Winamax; the simulator has no clock, so a
/// level is converted into a number of hands, an assumption to tune.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Structure {
    pub starting_stack: u32,
    /// Blind levels in order; the last one repeats until the game ends.
    pub levels: Vec<BlindLevel>,
    /// Hands per level while three players are left.
    pub hands_per_level: u32,
    /// Hands per level once heads-up (heads-up hands are faster).
    pub hands_per_level_heads_up: u32,
}

impl Structure {
    /// The Expresso Nitro: 300 chips, [`NITRO_BLIND_LEVELS`], 4 hands per
    /// level three-handed and 5 heads-up.
    ///
    /// The hands per level come from the NitroVariance dataset (2023, read
    /// 2026-10-07): fully played levels last 3.8 to 5.3 hands on average,
    /// median 4 for the early three-handed levels and 5 later on.
    pub fn expresso_nitro() -> Self {
        Self {
            starting_stack: 300,
            levels: NITRO_BLIND_LEVELS.to_vec(),
            hands_per_level: 4,
            hands_per_level_heads_up: 5,
        }
    }
}
