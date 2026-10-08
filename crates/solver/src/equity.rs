//! Exact heads-up all-in equities between the 169 hand classes.
//!
//! Every five-card board is enumerated (up to suit isomorphism), so the table
//! is exact and carries the real card removal: a class pair is weighted by the
//! number of disjoint combo pairs, and its equity averages over all of them.

use std::sync::OnceLock;

use rayon::prelude::*;
use rs_poker::core::{Card, SevenCardAccum};

use crate::hand::{HandClass, NUM_CLASSES};

pub(crate) const NUM_CARDS: u8 = 52;
const NUM_COMBOS: usize = 1326;
const NUM_PAIRS: usize = NUM_CLASSES * NUM_CLASSES;

/// Class-vs-class chance model of a heads-up all-in, indexed `[i * 169 + j]`
/// for the first player holding class `i` and the second class `j`.
pub(crate) struct HeadsUpEquity {
    /// Probability that the deal gives this class pair; sums to 1.
    pub(crate) weight: Vec<f64>,
    /// Share of the pot won by the first player at showdown (ties split).
    pub(crate) equity: Vec<f64>,
}

impl HeadsUpEquity {
    /// The table is spot-independent and costs a few seconds to build, so it
    /// is computed once per process.
    pub(crate) fn get() -> &'static HeadsUpEquity {
        static TABLE: OnceLock<HeadsUpEquity> = OnceLock::new();
        TABLE.get_or_init(HeadsUpEquity::compute)
    }

    fn compute() -> HeadsUpEquity {
        let combos = Combos::new();

        // Every disjoint combo pair sees the same number of boards, so the
        // deal weight of a class pair is its count of disjoint combo pairs.
        let mut pairs = vec![0u64; NUM_PAIRS];
        for a in 0..NUM_COMBOS {
            for b in 0..NUM_COMBOS {
                if combos.mask[a] & combos.mask[b] == 0 {
                    pairs[combos.class[a] * NUM_CLASSES + combos.class[b]] += 1;
                }
            }
        }
        let total_pairs: u64 = pairs.iter().sum();

        let half_wins = canonical_boards()
            .par_iter()
            .fold(
                || Showdowns::new(&combos),
                |mut showdowns, board| {
                    showdowns.add_board(board);
                    showdowns
                },
            )
            .map(Showdowns::into_totals)
            .reduce(
                || vec![0; NUM_PAIRS],
                |mut a, b| {
                    a.iter_mut().zip(b).for_each(|(x, y)| *x += y);
                    a
                },
            );
        let boards_per_pair = binomial(NUM_CARDS as u64 - 4, 5);

        HeadsUpEquity {
            weight: pairs
                .iter()
                .map(|&n| n as f64 / total_pairs as f64)
                .collect(),
            equity: pairs
                .iter()
                .zip(&half_wins)
                .map(|(&n, &h)| {
                    if n == 0 {
                        0.5
                    } else {
                        h as f64 / (2 * n * boards_per_pair) as f64
                    }
                })
                .collect(),
        }
    }
}

fn binomial(n: u64, k: u64) -> u64 {
    (0..k).fold(1, |acc, i| acc * (n - i) / (i + 1))
}

/// The 1326 two-card combos with their class.
pub(crate) struct Combos {
    pub(crate) cards: Vec<[u8; 2]>,
    pub(crate) mask: Vec<u64>,
    pub(crate) class: Vec<usize>,
}

impl Combos {
    pub(crate) fn new() -> Combos {
        let mut cards = Vec::with_capacity(NUM_COMBOS);
        for a in 0..NUM_CARDS {
            for b in a + 1..NUM_CARDS {
                cards.push([a, b]);
            }
        }
        let mask = cards.iter().map(|&[a, b]| 1 << a | 1 << b).collect();
        let class = cards
            .iter()
            .map(|&[a, b]| HandClass::of_cards((a % 13, a / 13), (b % 13, b / 13)).index())
            .collect();
        Combos { cards, mask, class }
    }
}

struct Board {
    cards: [u8; 5],
    mask: u64,
    /// Number of boards in this board's suit-permutation orbit.
    multiplicity: u64,
}

/// One representative per suit-isomorphism class of five-card boards.
///
/// Relabelling suits permutes the four per-suit rank masks, so the boards
/// whose masks are already in non-increasing order are exactly one per orbit.
fn canonical_boards() -> Vec<Board> {
    let suit_masks =
        |mask: u64| -> [u64; 4] { std::array::from_fn(|s| (mask >> (13 * s)) & 0x1fff) };
    let mut boards = Vec::new();
    for c0 in 0..NUM_CARDS {
        for c1 in c0 + 1..NUM_CARDS {
            for c2 in c1 + 1..NUM_CARDS {
                for c3 in c2 + 1..NUM_CARDS {
                    for c4 in c3 + 1..NUM_CARDS {
                        let cards = [c0, c1, c2, c3, c4];
                        let mask = cards.iter().fold(0u64, |m, &c| m | 1 << c);
                        let s = suit_masks(mask);
                        if s.windows(2).all(|w| w[0] >= w[1]) {
                            boards.push(Board {
                                cards,
                                mask,
                                multiplicity: distinct_permutations(&s),
                            });
                        }
                    }
                }
            }
        }
    }
    boards
}

/// Number of distinct orderings of a sorted 4-tuple: 4! over the factorials
/// of the run lengths of equal values.
fn distinct_permutations(sorted: &[u64; 4]) -> u64 {
    let factorial = |n: u64| (1..=n).product::<u64>();
    let mut result = factorial(4);
    let mut run = 1;
    for k in 1..=4 {
        if k < 4 && sorted[k] == sorted[k - 1] {
            run += 1;
        } else {
            result /= factorial(run);
            run = 1;
        }
    }
    result
}

/// Showdown results summed over boards, per class pair, counted in half pots
/// (2 per win, 1 per tie).
///
/// Boards are tallied in `u32` tables, one per board multiplicity, and only
/// weighted by their multiplicity at the end: this keeps the hot loop narrow.
struct Showdowns<'a> {
    combos: &'a Combos,
    by_multiplicity: Vec<(u64, Vec<u32>)>,
    rank: Vec<u16>,
    live: Vec<u16>,
    /// Class histograms of the live combos ranked below the current group:
    /// `[ALL]` over every combo, `[1 + card]` over the combos holding `card`.
    below: Vec<[u32; NUM_CLASSES]>,
    /// Same histograms for the combos of the current rank group.
    equal: Vec<[u32; NUM_CLASSES]>,
}

const ALL: usize = 0;

impl<'a> Showdowns<'a> {
    fn new(combos: &'a Combos) -> Showdowns<'a> {
        Showdowns {
            combos,
            by_multiplicity: Vec::new(),
            rank: vec![0; NUM_COMBOS],
            live: Vec::with_capacity(NUM_COMBOS),
            below: vec![[0; NUM_CLASSES]; 1 + NUM_CARDS as usize],
            equal: vec![[0; NUM_CLASSES]; 1 + NUM_CARDS as usize],
        }
    }

    fn into_totals(self) -> Vec<u64> {
        let mut totals = vec![0u64; NUM_PAIRS];
        for (multiplicity, table) in self.by_multiplicity {
            for (total, n) in totals.iter_mut().zip(table) {
                *total += multiplicity * n as u64;
            }
        }
        totals
    }

    fn add_board(&mut self, board: &Board) {
        let combos = self.combos;
        let mut accum = SevenCardAccum::new();
        for &c in &board.cards {
            accum.add(Card::from(c));
        }
        let rank = &mut self.rank;
        let live = &mut self.live;
        live.clear();
        let combo_cards = combos.mask.iter().zip(&combos.cards);
        for (k, ((&mask, &[a, b]), r)) in combo_cards.zip(rank.iter_mut()).enumerate() {
            if mask & board.mask == 0 {
                *r = (accum + Card::from(a) + Card::from(b)).rank().to_raw();
                live.push(k as u16);
            }
        }
        live.sort_unstable_by_key(|&k| rank[k as usize]);

        let table = match self
            .by_multiplicity
            .iter()
            .position(|(m, _)| *m == board.multiplicity)
        {
            Some(i) => &mut self.by_multiplicity[i].1,
            None => {
                self.by_multiplicity
                    .push((board.multiplicity, vec![0; NUM_PAIRS]));
                &mut self.by_multiplicity.last_mut().expect("just pushed").1
            }
        };

        // Walk the live combos from the weakest up, one rank group at a time.
        // Each combo beats the combos below its group and ties its group; the
        // combos sharing one of its cards are not real opponents, so their
        // per-card histograms are subtracted.
        let (below, equal) = (&mut self.below, &mut self.equal);
        for histogram in below.iter_mut() {
            histogram.fill(0);
        }
        let mut start = 0;
        while start < live.len() {
            let group_rank = rank[live[start] as usize];
            let end = start
                + live[start..]
                    .iter()
                    .take_while(|&&k| rank[k as usize] == group_rank)
                    .count();
            let group = &live[start..end];
            for &k in group {
                let class = combos.class[k as usize];
                let [c1, c2] = combos.cards[k as usize];
                for h in [ALL, 1 + c1 as usize, 1 + c2 as usize] {
                    equal[h][class] += 1;
                }
            }
            for &k in group {
                let class = combos.class[k as usize];
                let [c1, c2] = combos.cards[k as usize];
                let (c1, c2) = (1 + c1 as usize, 1 + c2 as usize);
                let row: &mut [u32; NUM_CLASSES] = (&mut table[class * NUM_CLASSES..]
                    [..NUM_CLASSES])
                    .try_into()
                    .expect("a table row has one cell per class");
                let (b0, b1, b2) = (&below[ALL], &below[c1], &below[c2]);
                let (e0, e1, e2) = (&equal[ALL], &equal[c1], &equal[c2]);
                // Wrapping arithmetic: a cell can dip below zero in the middle
                // of this sum, but every final count is non-negative.
                for j in 0..NUM_CLASSES {
                    let wins = b0[j].wrapping_sub(b1[j]).wrapping_sub(b2[j]);
                    let ties = e0[j].wrapping_sub(e1[j]).wrapping_sub(e2[j]);
                    row[j] = row[j].wrapping_add(wins.wrapping_mul(2)).wrapping_add(ties);
                }
                // The combo holds both of its cards, so the two per-card
                // histograms removed it twice from its own tie count.
                row[class] = row[class].wrapping_add(1);
            }
            for &k in group {
                let class = combos.class[k as usize];
                let [c1, c2] = combos.cards[k as usize];
                for h in [ALL, 1 + c1 as usize, 1 + c2 as usize] {
                    below[h][class] += equal[h][class];
                    equal[h][class] = 0;
                }
            }
            start = end;
        }
    }
}
