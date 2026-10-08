//! 3-max push/fold: the BTN pushes or folds, then the SB and the BB, each
//! calling or folding when facing a push.
//!
//! One evaluation walks every (BTN, SB, BB) class triple, about 4.8 million,
//! so this loop is the solver's hot spot. It runs in `f32` over contiguous BB
//! classes (vectorised by the compiler), with one parallel task per BTN
//! class whose partial sums are added up in `f64`.

use rayon::prelude::*;

use crate::cfr::{self, Game, Table};
use crate::hand::NUM_CLASSES;
use crate::push_fold::{BTN_BB, BTN_SB, NUM_TERMS, Payoff, PushFold, SB_BB, Slot};
use crate::spot::{Position, Spot};
use crate::three_way_equity::{BB_STRIDE, SAMPLES, ThreeWayEquity};
use crate::tree::{Action, Node};

const FOLD: usize = 0;
const ALL_IN: usize = 1;

pub(crate) struct ThreeMaxPushFold {
    equity: &'static ThreeWayEquity,
    tree: PushFold,
    nodes: Vec<Node>,
    actions: Vec<&'static [Action]>,
    lines: Lines,
}

impl ThreeMaxPushFold {
    pub(crate) fn new(spot: &Spot) -> ThreeMaxPushFold {
        let tree = PushFold::new(spot);
        let nodes = tree.decisions();
        ThreeMaxPushFold {
            equity: ThreeWayEquity::get(),
            actions: nodes.iter().map(|n| n.actions()).collect(),
            nodes,
            lines: Lines::new(&tree),
            tree,
        }
    }

    /// Probability that each class goes all-in at every node, indexed like
    /// [`Node::ALL`]: from the profile at a decision, 1 elsewhere (a forced
    /// player is in; an unreachable node only meets zero reach).
    fn all_in(&self, profile: &Table) -> [Vec<f32>; 6] {
        Node::ALL.map(|node| {
            let mut x = vec![0.0f32; BB_STRIDE];
            match self.nodes.iter().position(|&n| n == node) {
                Some(n) => {
                    for (class, x) in x.iter_mut().take(NUM_CLASSES).enumerate() {
                        *x = profile.infoset(n, class)[ALL_IN] as f32;
                    }
                }
                None => x[..NUM_CLASSES].fill(1.0),
            }
            x
        })
    }
}

/// The seven ways a hand can end, by who stays in, with their payoffs in
/// `f32`. Lines with one player in have no showdown; lines with two depend
/// on one pair term only.
struct Lines {
    bb_walk: [f32; 3],
    sb_steal: [f32; 3],
    btn_steal: [f32; 3],
    sb_bb: Pair,
    btn_bb: Pair,
    btn_sb: Pair,
    all: Three,
}

#[derive(Clone, Copy)]
struct Pair {
    constant: [f32; 3],
    slope: [f32; 3],
}

#[derive(Clone, Copy)]
struct Three {
    constant: [f32; 2],
    terms: [[f32; NUM_TERMS]; 2],
}

impl Lines {
    fn new(tree: &PushFold) -> Lines {
        use Position::{Bb, Btn, Sb};
        let single = |p: &Payoff| p.constant.map(|c| c as f32);
        let pair = |p: &Payoff, term: usize| Pair {
            constant: p.constant.map(|c| c as f32),
            slope: std::array::from_fn(|q| p.terms[q][term] as f32),
        };
        let all = tree.payoff(&[Btn, Sb, Bb]);
        // The player out of a two-way showdown only loses what it put in.
        for (line, out) in [(&[Sb, Bb], Btn), (&[Btn, Bb], Sb), (&[Btn, Sb], Bb)] {
            debug_assert!(
                tree.payoff(line).terms[out.index()]
                    .iter()
                    .all(|&t| t == 0.0)
            );
        }
        Lines {
            bb_walk: single(tree.payoff(&[Bb])),
            sb_steal: single(tree.payoff(&[Sb])),
            btn_steal: single(tree.payoff(&[Btn])),
            sb_bb: pair(tree.payoff(&[Sb, Bb]), SB_BB),
            btn_bb: pair(tree.payoff(&[Btn, Bb]), BTN_BB),
            btn_sb: pair(tree.payoff(&[Btn, Sb]), BTN_SB),
            all: Three {
                constant: [all.constant[0] as f32, all.constant[1] as f32],
                terms: [0, 1].map(|p| all.terms[p].map(|t| t as f32)),
            },
        }
    }
}

/// Counterfactual values summed over the SB and BB classes for one BTN
/// class: `[fold, all-in]` per node class.
struct Partial {
    btn: [f64; 2],
    /// `[SbVsBtnPush, SbOpen]`, per SB class.
    sb: Vec<[[f64; 2]; 2]>,
    /// `[BbVsBtnPush, BbVsBtnPushSbCall, BbVsSbPush]`, per BB class.
    bb: [[Vec<f32>; 2]; 3],
}

impl Game for ThreeMaxPushFold {
    fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    fn actions(&self) -> &[&'static [Action]] {
        &self.actions
    }

    fn exploitability(&self, profile: &Table, locks: &[Option<&[f64]>]) -> Vec<(Position, f64)> {
        cfr::one_shot_exploitability(self, profile, locks)
    }

    fn action_values(&self, profile: &Table) -> Table {
        let x = self.all_in(profile);
        let partials: Vec<Partial> = (0..NUM_CLASSES)
            .into_par_iter()
            .map(|btn| self.btn_class_values(btn, &x))
            .collect();

        let scale = 1.0 / self.equity.total_deals;
        let mut values = profile.zeros_like();
        for (n, &node) in self.nodes.iter().enumerate() {
            debug_assert_eq!(self.tree.slot(node), Slot::Decision);
            for class in 0..NUM_CLASSES {
                let v: [f64; 2] = match node {
                    Node::BtnOpen => partials[class].btn,
                    Node::SbVsBtnPush => sum(&partials, |p| p.sb[class][0]),
                    Node::SbOpen => sum(&partials, |p| p.sb[class][1]),
                    Node::BbVsBtnPush => sum(&partials, |p| bb_pair(&p.bb[0], class)),
                    Node::BbVsBtnPushSbCall => sum(&partials, |p| bb_pair(&p.bb[1], class)),
                    Node::BbVsSbPush => sum(&partials, |p| bb_pair(&p.bb[2], class)),
                    Node::Line(_) => unreachable!("a push/fold tree has only the named nodes"),
                };
                values
                    .infoset_mut(n, class)
                    .copy_from_slice(&v.map(|v| v * scale));
            }
        }
        values
    }
}

fn bb_pair(values: &[Vec<f32>; 2], class: usize) -> [f64; 2] {
    values.each_ref().map(|v| f64::from(v[class]))
}

fn sum(partials: &[Partial], f: impl Fn(&Partial) -> [f64; 2]) -> [f64; 2] {
    partials
        .iter()
        .map(f)
        .fold([0.0; 2], |a, b| [a[0] + b[0], a[1] + b[1]])
}

impl ThreeMaxPushFold {
    fn btn_class_values(&self, btn: usize, x: &[Vec<f32>; 6]) -> Partial {
        let [
            btn_push,
            sb_call,
            sb_push,
            bb_call_btn,
            bb_call_both,
            bb_call_sb,
        ] = x;
        let l = &self.lines;
        let eq = self.equity;
        let half = 1.0 / (2.0 * f32::from(SAMPLES));
        let sixth = 1.0 / (6.0 * f32::from(SAMPLES));
        let x0 = btn_push[btn];

        let mut out = Partial {
            btn: [0.0; 2],
            sb: vec![[[0.0; 2]; 2]; NUM_CLASSES],
            bb: std::array::from_fn(|_| [vec![0.0; NUM_CLASSES], vec![0.0; NUM_CLASSES]]),
        };
        // Per BB class: the deal weight reaching each BB node (its fold
        // payoffs are constant), and the weighted payoff of its calls.
        let mut bb_reach = [[0.0f32; BB_STRIDE]; 3];
        let mut bb_call = [[0.0f32; BB_STRIDE]; 3];
        let y3: &[f32; BB_STRIDE] = as_row(bb_call_btn, 0);
        let y4: &[f32; BB_STRIDE] = as_row(bb_call_both, 0);
        let y5: &[f32; BB_STRIDE] = as_row(bb_call_sb, 0);
        let (sb_bb, btn_bb, btn_sb, all) = (l.sb_bb, l.btn_bb, l.btn_sb, l.all);
        let (btn_steal, sb_steal) = (l.btn_steal, l.sb_steal);

        for sb in 0..NUM_CLASSES {
            let (x1, x2) = (sb_call[sb], sb_push[sb]);
            // Reach of the BB's three nodes through the BTN and the SB.
            let (r_btn, r_both, r_sb) = (x0 * (1.0 - x1), x0 * x1, (1.0 - x0) * x2);
            let start = (btn * NUM_CLASSES + sb) * BB_STRIDE;
            let deals = as_row(&eq.deals, start);
            let [e_btn_sb, e_btn_bb, e_sb_bb, s_btn, s_sb] =
                eq.results.each_ref().map(|r| as_row(r, start));

            // Weighted sums over the BB's class of: the deal weight; the SB's
            // value after its fold, after its call, after its push; the
            // BTN's value after its push, and after the SB pushes behind its
            // fold. Algebraic adds let the compiler reorder these sums into
            // SIMD lanes.
            let mut acc = [0.0f32; 6];
            for k in 0..BB_STRIDE {
                let w = f32::from(deals[k]);
                let e01 = f32::from(e_btn_sb[k]) * half;
                let e02 = f32::from(e_btn_bb[k]) * half;
                let e12 = f32::from(e_sb_bb[k]) * half;
                let s0 = f32::from(s_btn[k]) * sixth;
                let s1 = f32::from(s_sb[k]) * sixth;
                let terms = [e01, e02, e12, s0, s1];

                let sb_bb1 = sb_bb.constant[1] + sb_bb.slope[1] * e12;
                let sb_bb2 = sb_bb.constant[2] + sb_bb.slope[2] * e12;
                let btn_bb0 = btn_bb.constant[0] + btn_bb.slope[0] * e02;
                let btn_bb2 = btn_bb.constant[2] + btn_bb.slope[2] * e02;
                let btn_sb0 = btn_sb.constant[0] + btn_sb.slope[0] * e01;
                let btn_sb1 = btn_sb.constant[1] + btn_sb.slope[1] * e01;
                let mut all0 = all.constant[0];
                let mut all1 = all.constant[1];
                for ((a0, a1), e) in all.terms[0].iter().zip(&all.terms[1]).zip(terms) {
                    all0 += a0 * e;
                    all1 += a1 * e;
                }

                bb_reach[0][k] += w * r_btn;
                bb_call[0][k] += w * r_btn * btn_bb2;
                bb_reach[1][k] += w * r_both;
                bb_call[1][k] -= w * r_both * (all0 + all1);
                bb_reach[2][k] += w * r_sb;
                bb_call[2][k] += w * r_sb * sb_bb2;

                let after = |y: f32, fold: f32, call: f32| fold + y * (call - fold);
                let btn_bb1 = btn_bb.constant[1];
                let sb_bb0 = sb_bb.constant[0];
                let btn_after_push = (1.0 - x1) * after(y3[k], btn_steal[0], btn_bb0)
                    + x1 * after(y4[k], btn_sb0, all0);
                let sums = [
                    w,
                    w * after(y3[k], btn_steal[1], btn_bb1),
                    w * after(y4[k], btn_sb1, all1),
                    w * after(y5[k], sb_steal[1], sb_bb1),
                    w * btn_after_push,
                    w * after(y5[k], sb_steal[0], sb_bb0),
                ];
                for (a, s) in acc.iter_mut().zip(sums) {
                    *a = a.algebraic_add(s);
                }
            }
            let [w, sb_fold, sb_call_v, sb_push_v, btn_push, btn_sb_push] = acc.map(f64::from);
            let (x0, x2) = (f64::from(x0), f64::from(x2));
            out.sb[sb][0][FOLD] += x0 * sb_fold;
            out.sb[sb][0][ALL_IN] += x0 * sb_call_v;
            out.sb[sb][1][FOLD] += (1.0 - x0) * w * f64::from(l.bb_walk[1]);
            out.sb[sb][1][ALL_IN] += (1.0 - x0) * sb_push_v;
            out.btn[ALL_IN] += btn_push;
            out.btn[FOLD] += (1.0 - x2) * w * f64::from(l.bb_walk[0]) + x2 * btn_sb_push;
        }
        let bb_fold = [btn_steal[2], btn_sb.constant[2], sb_steal[2]];
        for (node, values) in out.bb.iter_mut().enumerate() {
            let [fold, call] = values;
            for k in 0..NUM_CLASSES {
                fold[k] = bb_reach[node][k] * bb_fold[node];
                call[k] = bb_call[node][k];
            }
        }
        out
    }
}

/// The BB-class row of a table starting at `start`.
fn as_row<T>(table: &[T], start: usize) -> &[T; BB_STRIDE] {
    table[start..start + BB_STRIDE]
        .try_into()
        .expect("the slice has BB_STRIDE elements")
}
