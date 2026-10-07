//! Chance model of a 3-max deal: for every ordered triple of hand classes
//! (BTN, SB, BB), how likely the deal is and how the showdowns between those
//! hands go.
//!
//! Deal weights are exact: the number of disjoint combo triples, so card
//! removal is fully modelled. Showdowns are estimated: each class triple gets
//! the same number of sampled deals (a combo triple drawn uniformly among the
//! disjoint ones, then a board from the 46 remaining cards). Exact
//! enumeration of three-way showdowns is out of reach, and stratifying by
//! class triple spreads the sampling error evenly instead of leaving rare
//! triples nearly unsampled. A triple and its permutations share one set of
//! samples, which divides the work by up to six.

use std::sync::OnceLock;

use rayon::prelude::*;
use rs_poker::core::{Card, SevenCardAccum};

use crate::equity::{Combos, NUM_CARDS};
use crate::hand::NUM_CLASSES;
use crate::push_fold::NUM_TERMS;

/// Sampled deals per unordered class triple.
pub(crate) const SAMPLES: u16 = 64;

/// Stride of the BB class in the tables: 169 rounded up to a multiple of 8,
/// so that the solver's inner loop runs over whole SIMD lanes with no scalar
/// tail. Padding cells have weight 0.
pub(crate) const BB_STRIDE: usize = 176;

/// Fixed so that every run solves the same game.
const SEED: u64 = 0x005e_ed0f_d3a1;

/// Indexed `[(btn * 169 + sb) * BB_STRIDE + bb]`.
pub(crate) struct ThreeWayEquity {
    /// Number of disjoint combo triples dealing these classes.
    pub(crate) deals: Vec<u16>,
    /// Sum of `deals`.
    pub(crate) total_deals: f64,
    /// Sampled showdown results, in the order of the payoff terms
    /// ([`crate::push_fold::BTN_SB`]…): half pots won out of
    /// `2 * SAMPLES` for the pair terms, sixths of the pot out of
    /// `6 * SAMPLES` for the three-way shares.
    pub(crate) results: [Vec<u16>; NUM_TERMS],
}

/// Outcome of the samples of one class triple, for its three hands in
/// order: half pots for the pairs (0,1), (0,2), (1,2), then sixths of the
/// three-way pot for hands 0, 1 and 2.
#[derive(Clone, Copy, Default)]
struct Sampled {
    deals: u16,
    halves: [u16; 3],
    sixths: [u16; 3],
}

impl ThreeWayEquity {
    /// The table is spot-independent and costs a fraction of a second to
    /// build, so it is computed once per process.
    pub(crate) fn get() -> &'static ThreeWayEquity {
        static TABLE: OnceLock<ThreeWayEquity> = OnceLock::new();
        TABLE.get_or_init(ThreeWayEquity::compute)
    }

    fn compute() -> ThreeWayEquity {
        let combos = Combos::new();
        let mut by_class = vec![Vec::new(); NUM_CLASSES];
        for (k, &class) in combos.class.iter().enumerate() {
            by_class[class].push(k);
        }

        // Unordered triples a <= b <= c, sampled once each.
        let sorted: Vec<[usize; 3]> = (0..NUM_CLASSES)
            .flat_map(|a| {
                (a..NUM_CLASSES).flat_map(move |b| (b..NUM_CLASSES).map(move |c| [a, b, c]))
            })
            .collect();
        let sampled: Vec<Sampled> = sorted
            .par_iter()
            .enumerate()
            .map(|(n, &triple)| sample(&combos, &by_class, triple, n as u64))
            .collect();
        let mut first_of = vec![0usize; NUM_CLASSES * NUM_CLASSES];
        let mut n = 0;
        for a in 0..NUM_CLASSES {
            for b in a..NUM_CLASSES {
                first_of[a * NUM_CLASSES + b] = n;
                n += NUM_CLASSES - b;
            }
        }
        let lookup = |[a, b, c]: [usize; 3]| &sampled[first_of[a * NUM_CLASSES + b] + c - b];

        let len = NUM_CLASSES * NUM_CLASSES * BB_STRIDE;
        let mut deals = vec![0u16; len];
        let mut results: [Vec<u16>; NUM_TERMS] = std::array::from_fn(|_| vec![0u16; len]);
        let [r0, r1, r2, r3, r4] = &mut results;
        let rows = deals
            .par_chunks_mut(BB_STRIDE)
            .zip(r0.par_chunks_mut(BB_STRIDE))
            .zip(r1.par_chunks_mut(BB_STRIDE))
            .zip(r2.par_chunks_mut(BB_STRIDE))
            .zip(r3.par_chunks_mut(BB_STRIDE))
            .zip(r4.par_chunks_mut(BB_STRIDE));
        rows.enumerate()
            .for_each(|(row, (((((d, btn_sb), btn_bb), sb_bb), btn3), sb3))| {
                let (btn, sb) = (row / NUM_CLASSES, row % NUM_CLASSES);
                for bb in 0..NUM_CLASSES {
                    let seats = [btn, sb, bb];
                    // Hand number of each seat in the sorted triple.
                    let mut order = [0, 1, 2];
                    order.sort_by_key(|&s| seats[s]);
                    let mut hand_of = [0; 3];
                    for (h, &s) in order.iter().enumerate() {
                        hand_of[s] = h;
                    }
                    let s = lookup(order.map(|s| seats[s]));
                    let half = |p: usize, q: usize| {
                        let (hp, hq) = (hand_of[p], hand_of[q]);
                        let pair = |x: usize, y: usize| s.halves[x + y - 1];
                        if hp < hq {
                            pair(hp, hq)
                        } else {
                            2 * SAMPLES - pair(hq, hp)
                        }
                    };
                    d[bb] = s.deals;
                    btn_sb[bb] = half(0, 1);
                    btn_bb[bb] = half(0, 2);
                    sb_bb[bb] = half(1, 2);
                    btn3[bb] = s.sixths[hand_of[0]];
                    sb3[bb] = s.sixths[hand_of[1]];
                }
            });
        ThreeWayEquity {
            total_deals: deals.iter().map(|&d| f64::from(d)).sum(),
            deals,
            results,
        }
    }
}

/// Counts the deals of the sorted class triple and samples its showdowns.
fn sample(combos: &Combos, by_class: &[Vec<usize>], [a, b, c]: [usize; 3], stream: u64) -> Sampled {
    let mut deals: Vec<[usize; 3]> = Vec::new();
    for &x in &by_class[a] {
        for &y in &by_class[b] {
            if combos.mask[x] & combos.mask[y] != 0 {
                continue;
            }
            let xy = combos.mask[x] | combos.mask[y];
            for &z in &by_class[c] {
                if xy & combos.mask[z] == 0 {
                    deals.push([x, y, z]);
                }
            }
        }
    }
    let mut out = Sampled {
        deals: deals.len() as u16,
        ..Sampled::default()
    };
    if deals.is_empty() {
        return out;
    }
    let mut rng = SplitMix64(SEED ^ stream.wrapping_mul(0x9e37_79b9_7f4a_7c15));
    for _ in 0..SAMPLES {
        let hands = deals[rng.below(deals.len() as u64) as usize];
        let mut used = hands.iter().fold(0u64, |m, &h| m | combos.mask[h]);
        let mut board = SevenCardAccum::new();
        for _ in 0..5 {
            let card = loop {
                let card = rng.below(u64::from(NUM_CARDS)) as u8;
                if used & 1 << card == 0 {
                    break card;
                }
            };
            used |= 1 << card;
            board.add(Card::from(card));
        }
        let rank = hands.map(|h| {
            let [x, y] = combos.cards[h];
            (board + Card::from(x) + Card::from(y)).rank()
        });
        for (slot, (p, q)) in [(0, 1), (0, 2), (1, 2)].into_iter().enumerate() {
            out.halves[slot] += match rank[p].cmp(&rank[q]) {
                std::cmp::Ordering::Greater => 2,
                std::cmp::Ordering::Equal => 1,
                std::cmp::Ordering::Less => 0,
            };
        }
        let best = rank.iter().max().expect("three hands");
        let winners = rank.iter().filter(|&r| r == best).count() as u16;
        for (p, r) in rank.iter().enumerate() {
            if r == best {
                out.sixths[p] += 6 / winners;
            }
        }
    }
    out
}

/// SplitMix64 (Steele, Lea and Flood, 2014): small, fast and good enough for
/// Monte Carlo sampling, and seedable per class triple so that the table does
/// not depend on how the work is split between threads.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A uniform integer below `n` (Lemire's multiply-shift, bias < 2^-50
    /// for the small `n` used here).
    fn below(&mut self, n: u64) -> u64 {
        ((u128::from(self.next()) * u128::from(n)) >> 64) as u64
    }
}
