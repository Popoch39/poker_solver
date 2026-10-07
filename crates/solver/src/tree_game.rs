//! Games on a [`BettingTree`]: spots with limps or min-raises, where a player
//! can act several times on one line and some lines end at the flop.
//!
//! Every payoff is affine in the five showdown terms of the chance model, and
//! a player's reach only depends on its own class. One evaluation therefore
//! splits each player's lines into *segments*: the part of the tree after one
//! of its actions (or the root) and before its next decision. For every
//! (BTN class, SB class) pair, with the BB's classes as a vector, it sums
//! each terminal's value into the segment it belongs to, for each player.
//! Counterfactual action values and best responses then follow from these
//! segment sums by walking each player's decisions bottom-up: a player's
//! decision weighs its actions by its strategy, a best response takes the
//! best one. No player needs to act only once for this to hold.

use rayon::prelude::*;

use crate::betting_tree::{BettingTree, Child};
use crate::cfr::{Game, Table};
use crate::equity::HeadsUpEquity;
use crate::hand::NUM_CLASSES;
use crate::push_fold::{NUM_TERMS, SB_BB};
use crate::spot::{Position, Spot};
use crate::three_way_equity::{BB_STRIDE, SAMPLES, ThreeWayEquity};
use crate::tree::{Action, Node};

/// The constant of a payoff, then its showdown terms.
const COEFFICIENTS: usize = 1 + NUM_TERMS;

const BTN: usize = 0;
const SB: usize = 1;
const BB: usize = 2;

/// Rows of the chance model for one (BTN class, SB class) pair, over the BB
/// classes: the deal weight, then the deal weight times each showdown term.
type Rows = [[f32; BB_STRIDE]; COEFFICIENTS];

enum Chance {
    ThreeMax(&'static ThreeWayEquity),
    /// No BTN: one dummy BTN class, and only the SB-BB term.
    HeadsUp(&'static HeadsUpEquity),
}

impl Chance {
    fn btn_classes(&self) -> usize {
        match self {
            Chance::ThreeMax(_) => NUM_CLASSES,
            Chance::HeadsUp(_) => 1,
        }
    }

    fn total_weight(&self) -> f64 {
        match self {
            Chance::ThreeMax(eq) => eq.total_deals,
            Chance::HeadsUp(eq) => eq.weight.iter().sum(),
        }
    }

    fn rows(&self, btn: usize, sb: usize, rows: &mut Rows) {
        match self {
            Chance::ThreeMax(eq) => {
                let start = (btn * NUM_CLASSES + sb) * BB_STRIDE;
                let half = 1.0 / (2.0 * f32::from(SAMPLES));
                let sixth = 1.0 / (6.0 * f32::from(SAMPLES));
                let [w, terms @ ..] = rows;
                for (w, &d) in w.iter_mut().zip(&eq.deals[start..start + BB_STRIDE]) {
                    *w = f32::from(d);
                }
                for (t, (row, results)) in terms.iter_mut().zip(&eq.results).enumerate() {
                    let unit = if t < 3 { half } else { sixth };
                    let results = &results[start..start + BB_STRIDE];
                    for ((x, &r), &w) in row.iter_mut().zip(results).zip(w.iter()) {
                        *x = w * f32::from(r) * unit;
                    }
                }
            }
            Chance::HeadsUp(eq) => {
                *rows = [[0.0; BB_STRIDE]; COEFFICIENTS];
                let pairs = sb * NUM_CLASSES..(sb + 1) * NUM_CLASSES;
                let (weight, equity) = (&eq.weight[pairs.clone()], &eq.equity[pairs]);
                for (x, &w) in rows[0].iter_mut().zip(weight) {
                    *x = w as f32;
                }
                for ((x, &w), &e) in rows[1 + SB_BB].iter_mut().zip(weight).zip(equity) {
                    *x = (w * e) as f32;
                }
            }
        }
    }
}

/// How one player's lines split into segments: segment 0 is the root, and
/// every action of the player starts a new one.
#[derive(Default)]
struct Segments {
    /// For each segment but the root: the decision and action starting it.
    origin: Vec<Option<(usize, usize)>>,
    /// The player's decisions reached from each segment, before any other
    /// decision of the player.
    next: Vec<Vec<usize>>,
}

impl Segments {
    fn add(&mut self, origin: Option<(usize, usize)>) -> usize {
        self.origin.push(origin);
        self.next.push(Vec::new());
        self.origin.len() - 1
    }
}

struct Terminal {
    /// Each player's segment.
    segment: [usize; 3],
    /// Each player's payoff: the constant, then the coefficient of each
    /// showdown term, in BB.
    coefficients: [[f32; COEFFICIENTS]; 3],
    /// Each player's non-zero coefficients, as a bit mask.
    used: [u8; 3],
}

pub(crate) struct TreeGame {
    chance: Chance,
    nodes: Vec<Node>,
    actions: Vec<&'static [Action]>,
    actors: Vec<usize>,
    /// Each player's segment on arriving at each decision.
    arrival: Vec<[usize; 3]>,
    /// The actor's segment after each action of each decision.
    after: Vec<Vec<usize>>,
    segments: [Segments; 3],
    terminals: Vec<Terminal>,
}

/// Sums of terminal values by segment over the whole chance model, in BB
/// times the total deal weight: `[segment][class]` per player.
struct SegmentValues([Vec<Vec<f64>>; 3]);

impl TreeGame {
    pub(crate) fn new(spot: &Spot) -> TreeGame {
        let tree = BettingTree::new(spot);
        let chance = if spot.is_heads_up() {
            Chance::HeadsUp(HeadsUpEquity::get())
        } else {
            Chance::ThreeMax(ThreeWayEquity::get())
        };
        let mut game = TreeGame {
            chance,
            nodes: tree.decisions.iter().map(|d| d.node).collect(),
            actions: tree.decisions.iter().map(|d| d.actions).collect(),
            actors: tree
                .decisions
                .iter()
                .map(|d| d.node.actor().index())
                .collect(),
            arrival: vec![[0; 3]; tree.decisions.len()],
            after: tree.decisions.iter().map(|_| Vec::new()).collect(),
            segments: Default::default(),
            terminals: Vec::new(),
        };
        for segments in &mut game.segments {
            segments.add(None);
        }
        let root = match tree.decisions.first() {
            Some(_) => Child::Decision(0),
            None => Child::Terminal(0),
        };
        game.walk(&tree, root, [0; 3]);
        game
    }

    /// Records the segments of the subtree at `child`, reached with each
    /// player in `segment`.
    fn walk(&mut self, tree: &BettingTree, child: Child, segment: [usize; 3]) {
        match child {
            Child::Terminal(z) => {
                let payoff = &tree.terminals[z];
                let coefficients: [[f32; COEFFICIENTS]; 3] = std::array::from_fn(|p| {
                    let mut c = [payoff.constant[p] as f32; COEFFICIENTS];
                    for (c, &t) in c[1..].iter_mut().zip(&payoff.terms[p]) {
                        *c = t as f32;
                    }
                    c
                });
                self.terminals.push(Terminal {
                    segment,
                    used: coefficients.map(|c| {
                        (0..COEFFICIENTS)
                            .filter(|&t| c[t] != 0.0)
                            .fold(0, |mask, t| mask | 1 << t)
                    }),
                    coefficients,
                });
            }
            Child::Decision(d) => {
                let actor = self.actors[d];
                self.arrival[d] = segment;
                self.segments[actor].next[segment[actor]].push(d);
                for (a, &child) in tree.decisions[d].children.iter().enumerate() {
                    let s = self.segments[actor].add(Some((d, a)));
                    self.after[d].push(s);
                    let mut segment = segment;
                    segment[actor] = s;
                    self.walk(tree, child, segment);
                }
            }
        }
    }

    /// Probability that each class of `player` plays to each of its
    /// segments, `[segment][class]`.
    fn reaches(&self, player: usize, profile: &Table) -> Vec<[f32; BB_STRIDE]> {
        let segments = &self.segments[player];
        let mut reach = vec![[0.0f32; BB_STRIDE]; segments.origin.len()];
        reach[0][..NUM_CLASSES].fill(1.0);
        for (s, origin) in segments.origin.iter().enumerate().skip(1) {
            let (d, a) = origin.expect("only the root has no origin");
            let from = reach[self.arrival[d][player]];
            for (class, r) in reach[s].iter_mut().take(NUM_CLASSES).enumerate() {
                *r = from[class] * profile.infoset(d, class)[a] as f32;
            }
        }
        reach
    }

    /// The one pass over the chance model.
    fn segment_values(&self, profile: &Table) -> SegmentValues {
        let reach = [BTN, SB, BB].map(|p| self.reaches(p, profile));
        let sb_reach: Vec<f32> = (0..NUM_CLASSES)
            .flat_map(|sb| self.terminals.iter().map(move |z| (sb, z)))
            .map(|(sb, z)| reach[SB][z.segment[SB]][sb])
            .collect();
        let partials: Vec<Partial> = (0..self.chance.btn_classes())
            .into_par_iter()
            .map(|btn| self.btn_partial(btn, &reach, &sb_reach))
            .collect();

        let count = |p: usize| self.segments[p].origin.len();
        let mut values = [BTN, SB, BB].map(|p| vec![vec![0.0f64; NUM_CLASSES]; count(p)]);
        let [btn_values, sb_values, bb_values] = &mut values;
        for (btn, partial) in partials.iter().enumerate() {
            for (s, &v) in partial.btn.iter().enumerate() {
                btn_values[s][btn] = v;
            }
            for (s, row) in partial.sb.chunks(NUM_CLASSES).enumerate() {
                for (x, &v) in sb_values[s].iter_mut().zip(row) {
                    *x += v;
                }
            }
            for (s, row) in partial.bb.iter().enumerate() {
                for (x, &v) in bb_values[s].iter_mut().zip(row) {
                    *x += f64::from(v);
                }
            }
        }
        SegmentValues(values)
    }

    /// Segment sums over every SB class and BB class for one BTN class.
    ///
    /// `sb_reach` is the SB's reach of each terminal, `[sb class][terminal]`.
    fn btn_partial(
        &self,
        btn: usize,
        reach: &[Vec<[f32; BB_STRIDE]>; 3],
        sb_reach: &[f32],
    ) -> Partial {
        let [btn_reach, sb_segment_reach, bb_reach] = reach;
        let bb_segments = bb_reach.len();
        let mut out = Partial {
            btn: vec![0.0; btn_reach.len()],
            sb: vec![0.0; sb_segment_reach.len() * NUM_CLASSES],
            bb: vec![[0.0; BB_STRIDE]; bb_segments],
        };
        let btn_reach: Vec<f32> = self
            .terminals
            .iter()
            .map(|z| btn_reach[z.segment[BTN]][btn])
            .collect();
        let mut rows: Rows = [[0.0; BB_STRIDE]; COEFFICIENTS];
        // Per BB segment: each coefficient's row summed against the BB's
        // reach, computed only when a terminal needs it (`summed` says which
        // are up to date for this pair), and the weight each coefficient
        // gets in the BB's values. A sum not up to date is only ever used
        // with a zero coefficient or a zero reach, and stays finite.
        let mut sums = vec![[0.0f32; COEFFICIENTS]; bb_segments];
        let mut summed = vec![0u8; bb_segments];
        let mut weights = vec![[0.0f32; COEFFICIENTS]; bb_segments];
        for sb in 0..NUM_CLASSES {
            self.chance.rows(btn, sb, &mut rows);
            summed.fill(0);
            weights.fill([0.0; COEFFICIENTS]);
            let sb_reach = &sb_reach[sb * self.terminals.len()..][..self.terminals.len()];
            for ((terminal, &r_btn), &r_sb) in self.terminals.iter().zip(&btn_reach).zip(sb_reach) {
                if r_btn == 0.0 && r_sb == 0.0 {
                    continue;
                }
                let [s_btn, s_sb, s_bb] = terminal.segment;
                // The BTN's value weighs by the SB's reach and the SB's by
                // the BTN's: a player out of reach needs no sums.
                let [used_btn, used_sb, _] = terminal.used;
                let needed =
                    if r_sb != 0.0 { used_btn } else { 0 } | if r_btn != 0.0 { used_sb } else { 0 };
                let missing = needed & !summed[s_bb];
                if missing != 0 {
                    sum_rows(&rows, &bb_reach[s_bb], missing, &mut sums[s_bb]);
                    summed[s_bb] |= missing;
                }
                let [c_btn, c_sb, c_bb] = &terminal.coefficients;
                let sum = &sums[s_bb];
                let value = |c: &[f32; COEFFICIENTS]| -> f32 {
                    c.iter().zip(sum).map(|(c, s)| c * s).sum()
                };
                out.btn[s_btn] += f64::from(r_sb * value(c_btn));
                out.sb[s_sb * NUM_CLASSES + sb] += f64::from(r_btn * value(c_sb));
                let r = r_btn * r_sb;
                for (w, c) in weights[s_bb].iter_mut().zip(c_bb) {
                    *w += r * c;
                }
            }
            for (acc, weights) in out.bb.iter_mut().zip(&weights) {
                add_rows(&rows, weights, acc);
            }
        }
        out
    }

    /// Each player's value of each action at each of its decisions, and its
    /// value at the root, `[class]`, in BB times the total deal weight.
    /// With `best_response` (the locks), the player best responds at its
    /// free decisions instead of playing the profile.
    fn player_values(
        &self,
        player: usize,
        sums: &[Vec<f64>],
        profile: &Table,
        best_response: Option<&[Option<&[f64]>]>,
    ) -> (Vec<Vec<Vec<f64>>>, Vec<f64>) {
        let segments = &self.segments[player];
        let mut action_values: Vec<Vec<Vec<f64>>> = vec![Vec::new(); self.nodes.len()];
        let mut node_values: Vec<Vec<f64>> = vec![Vec::new(); self.nodes.len()];
        let sum_from = |s: usize, node_values: &[Vec<f64>]| -> Vec<f64> {
            let mut v = sums[s].clone();
            for &d in &segments.next[s] {
                for (x, y) in v.iter_mut().zip(&node_values[d]) {
                    *x += y;
                }
            }
            v
        };
        // Decisions come in breadth-first order, so going backwards visits
        // a player's later decisions before the ones leading to them.
        for d in (0..self.nodes.len()).rev() {
            if self.actors[d] != player {
                continue;
            }
            let values: Vec<Vec<f64>> = self.after[d]
                .iter()
                .map(|&s| sum_from(s, &node_values))
                .collect();
            let respond = best_response.is_some_and(|locks| locks[d].is_none());
            node_values[d] = (0..NUM_CLASSES)
                .map(|class| {
                    let action = values.iter().map(|v| v[class]);
                    if respond {
                        action.fold(f64::NEG_INFINITY, f64::max)
                    } else {
                        action
                            .zip(profile.infoset(d, class))
                            .map(|(v, s)| v * s)
                            .sum()
                    }
                })
                .collect();
            action_values[d] = values;
        }
        let root = sum_from(0, &node_values);
        (action_values, root)
    }
}

/// [`TreeGame::btn_partial`]'s output: the BTN's segment sums for its class,
/// the SB's `[segment * 169 + class]`, the BB's `[segment][class]`.
struct Partial {
    btn: Vec<f64>,
    sb: Vec<f64>,
    bb: Vec<[f32; BB_STRIDE]>,
}

// The two kernels below are the hot loop. Each handles all the rows it
// needs in one pass over the BB classes, so that the reach (or the
// accumulator) is loaded once for all of them, with the number of rows a
// compile-time constant so that the loop body is fully unrolled.

/// Independent partial sums per row, which the compiler keeps in SIMD
/// registers.
const LANES: usize = 8;

/// The coefficients in `mask`, in order, and how many there are.
fn pick(mask: u8) -> ([usize; COEFFICIENTS], usize) {
    let mut picked = [0; COEFFICIENTS];
    let mut n = 0;
    for t in (0..COEFFICIENTS).filter(|t| mask & 1 << t != 0) {
        picked[n] = t;
        n += 1;
    }
    (picked, n)
}

/// Sets `sums[t]`, for each coefficient `t` in `mask`, to row `t` summed
/// against `reach`.
fn sum_rows(rows: &Rows, reach: &[f32; BB_STRIDE], mask: u8, sums: &mut [f32; COEFFICIENTS]) {
    let (picked, n) = pick(mask);
    match n {
        1 => sum_picked::<1>(rows, &picked, reach, sums),
        2 => sum_picked::<2>(rows, &picked, reach, sums),
        3 => sum_picked::<3>(rows, &picked, reach, sums),
        4 => sum_picked::<4>(rows, &picked, reach, sums),
        5 => sum_picked::<5>(rows, &picked, reach, sums),
        _ => sum_picked::<6>(rows, &picked, reach, sums),
    }
}

fn sum_picked<const N: usize>(
    rows: &Rows,
    picked: &[usize; COEFFICIENTS],
    reach: &[f32; BB_STRIDE],
    sums: &mut [f32; COEFFICIENTS],
) {
    let rows: [&[[f32; LANES]]; N] = std::array::from_fn(|i| rows[picked[i]].as_chunks().0);
    let mut acc = [[0.0f32; LANES]; N];
    for (k, r) in reach.as_chunks::<LANES>().0.iter().enumerate() {
        for (acc, row) in acc.iter_mut().zip(rows) {
            for ((a, x), r) in acc.iter_mut().zip(&row[k]).zip(r) {
                *a += x * r;
            }
        }
    }
    for (i, acc) in acc.iter().enumerate() {
        sums[picked[i]] = acc.iter().sum();
    }
}

/// Adds each row times its weight to `acc`, skipping zero weights.
fn add_rows(rows: &Rows, weights: &[f32; COEFFICIENTS], acc: &mut [f32; BB_STRIDE]) {
    let mask = (0..COEFFICIENTS)
        .filter(|&t| weights[t] != 0.0)
        .fold(0u8, |mask, t| mask | 1 << t);
    let (picked, n) = pick(mask);
    match n {
        0 => {}
        1 => add_picked::<1>(rows, &picked, weights, acc),
        2 => add_picked::<2>(rows, &picked, weights, acc),
        3 => add_picked::<3>(rows, &picked, weights, acc),
        4 => add_picked::<4>(rows, &picked, weights, acc),
        5 => add_picked::<5>(rows, &picked, weights, acc),
        _ => add_picked::<6>(rows, &picked, weights, acc),
    }
}

fn add_picked<const N: usize>(
    rows: &Rows,
    picked: &[usize; COEFFICIENTS],
    weights: &[f32; COEFFICIENTS],
    acc: &mut [f32; BB_STRIDE],
) {
    let rows: [&[f32; BB_STRIDE]; N] = std::array::from_fn(|i| &rows[picked[i]]);
    let weights: [f32; N] = std::array::from_fn(|i| weights[picked[i]]);
    for (k, a) in acc.iter_mut().enumerate() {
        let mut x = *a;
        for (row, w) in rows.iter().zip(weights) {
            x += w * row[k];
        }
        *a = x;
    }
}

impl Game for TreeGame {
    fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    fn actions(&self) -> &[&'static [Action]] {
        &self.actions
    }

    fn action_values(&self, profile: &Table) -> Table {
        let SegmentValues(sums) = self.segment_values(profile);
        let scale = 1.0 / self.chance.total_weight();
        let mut values = profile.zeros_like();
        for (player, sums) in sums.iter().enumerate() {
            let (action_values, _) = self.player_values(player, sums, profile, None);
            for (d, per_action) in action_values.iter().enumerate() {
                for (a, v) in per_action.iter().enumerate() {
                    for (class, v) in v.iter().enumerate() {
                        values.infoset_mut(d, class)[a] = v * scale;
                    }
                }
            }
        }
        values
    }

    fn exploitability(&self, profile: &Table, locks: &[Option<&[f64]>]) -> Vec<(Position, f64)> {
        let SegmentValues(sums) = self.segment_values(profile);
        let scale = 1.0 / self.chance.total_weight();
        let free =
            |p: usize| (0..self.nodes.len()).any(|d| self.actors[d] == p && locks[d].is_none());
        Position::ALL
            .into_iter()
            .filter(|p| free(p.index()))
            .map(|position| {
                let p = position.index();
                let (_, played) = self.player_values(p, &sums[p], profile, None);
                let (_, best) = self.player_values(p, &sums[p], profile, Some(locks));
                let gain: f64 = best.iter().zip(&played).map(|(b, v)| b - v).sum();
                (position, gain * scale)
            })
            .collect()
    }
}

impl TreeGame {
    /// What `position` wins per hand under `profile`, in BB.
    pub(crate) fn value(&self, profile: &Table, position: Position) -> f64 {
        let SegmentValues(sums) = self.segment_values(profile);
        let p = position.index();
        let (_, root) = self.player_values(p, &sums[p], profile, None);
        root.iter().sum::<f64>() / self.chance.total_weight()
    }
}
