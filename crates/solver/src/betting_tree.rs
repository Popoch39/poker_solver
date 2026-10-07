//! The preflop tree of a spot that allows limps or min-raises, and what each
//! player wins at the end of every line.
//!
//! Rules, in the order of play (BTN, SB, BB, then round again):
//!
//! - While nobody has raised, a player who has not yet put in a big blind
//!   folds, limps, min-raises (to 2 BB) or pushes; the BB, already in for
//!   one, checks, min-raises or pushes. A limp needs more than 1 BB of
//!   stack and a min-raise more than 2 BB, or they would be the push.
//! - There is at most one min-raise per hand. Facing it, a player folds,
//!   calls or pushes; a call that takes the whole stack is all-in.
//! - Once a player is all-in, the hand is push/fold from there: the others
//!   fold or call with their whole stack, what nobody matches coming back
//!   through side-pot layers (the push/fold tree's convention).
//! - A line where only one player is left ends there; one with an all-in
//!   player goes to showdown at all-in equity; one where every player still
//!   in has chips behind ends at the flop, valued by the equity model.
//!
//! A spot with a blind all-in from the start never gets here: it is solved
//! as push/fold.

use crate::push_fold::{
    BTN_BB, BTN_SB, BTN_SHARE3, NUM_TERMS, Payoff, SB_BB, SB_SHARE3, showdown_payoff,
};
use crate::spot::{Position, RealizationFactors, Spot};
use crate::tree::{Action, Node};

/// Where an action leads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Child {
    Decision(usize),
    Terminal(usize),
}

pub(crate) struct Decision {
    pub(crate) node: Node,
    pub(crate) actions: &'static [Action],
    /// One per action.
    pub(crate) children: Vec<Child>,
}

/// The tree of one spot: decisions in breadth-first order, so that a
/// decision always comes after the ones leading to it.
pub(crate) struct BettingTree {
    pub(crate) decisions: Vec<Decision>,
    /// The payoff of each terminal, indexed by [`Child::Terminal`].
    pub(crate) terminals: Vec<Payoff>,
}

const BIG_BLIND: f64 = 1.0;
const MIN_RAISE: f64 = 2.0;

#[derive(Clone)]
struct State {
    commit: [f64; 3],
    in_hand: [bool; 3],
    /// Has acted since the last raise.
    acted: [bool; 3],
    raised: bool,
    all_in: bool,
    /// The seat to look at first for the next decision.
    next: usize,
    steps: Vec<(Position, Action)>,
}

struct Rules<'a> {
    spot: &'a Spot,
    stacks: [f64; 3],
}

impl BettingTree {
    pub(crate) fn new(spot: &Spot) -> BettingTree {
        debug_assert!(spot.has_flop());
        let rules = Rules {
            spot,
            stacks: Position::ALL.map(|p| spot.stack(p)),
        };
        let root = State {
            commit: Position::ALL.map(|p| p.blind().min(spot.stack(p))),
            in_hand: Position::ALL.map(|p| spot.stack(p) > 0.0),
            acted: [false; 3],
            raised: false,
            all_in: false,
            next: 0,
            steps: Vec::new(),
        };
        let mut tree = BettingTree {
            decisions: Vec::new(),
            terminals: Vec::new(),
        };
        // Breadth-first, each state remembering which child slot it fills.
        let mut queue = std::collections::VecDeque::from([(root, None)]);
        while let Some((state, parent)) = queue.pop_front() {
            let child = match rules.to_act(&state) {
                None => {
                    tree.terminals.push(rules.payoff(&state));
                    Child::Terminal(tree.terminals.len() - 1)
                }
                Some(seat) => {
                    let actions = rules.actions(&state, seat);
                    let index = tree.decisions.len();
                    tree.decisions.push(Decision {
                        node: Node::new(Position::ALL[seat], &state.steps),
                        actions,
                        children: Vec::with_capacity(actions.len()),
                    });
                    for (a, &action) in actions.iter().enumerate() {
                        queue.push_back((rules.apply(&state, seat, action), Some((index, a))));
                    }
                    Child::Decision(index)
                }
            };
            if let Some((index, a)) = parent {
                let children = &mut tree.decisions[index].children;
                debug_assert_eq!(children.len(), a);
                children.push(child);
            }
        }
        debug_assert!(
            tree.decisions
                .iter()
                .enumerate()
                .all(|(i, d)| { tree.decisions[..i].iter().all(|e| e.node != d.node) }),
            "every decision has its own node"
        );
        tree
    }
}

impl Rules<'_> {
    fn max_commit(state: &State) -> f64 {
        state.commit.iter().copied().fold(0.0, f64::max)
    }

    fn has_chips(&self, state: &State, seat: usize) -> bool {
        state.in_hand[seat] && state.commit[seat] < self.stacks[seat]
    }

    /// The next player to decide, if any.
    fn to_act(&self, state: &State) -> Option<usize> {
        if state.in_hand.iter().filter(|&&i| i).count() < 2 {
            return None;
        }
        let bet = Rules::max_commit(state);
        (0..3).map(|k| (state.next + k) % 3).find(|&seat| {
            self.has_chips(state, seat)
                && (state.commit[seat] < bet || !state.all_in && !state.acted[seat])
        })
    }

    fn actions(&self, state: &State, seat: usize) -> &'static [Action] {
        use Action::{Call, Check, Fold, Limp, Push, Raise};
        if state.all_in {
            return &[Fold, Call];
        }
        let (bet, stack) = (Rules::max_commit(state), self.stacks[seat]);
        let can_raise = self.spot.allows_min_raise() && !state.raised && stack > MIN_RAISE;
        if state.commit[seat] == bet {
            return if can_raise {
                &[Check, Raise, Push]
            } else {
                &[Check, Push]
            };
        }
        if state.raised {
            // A stack that cannot cover the raise only calls, all-in.
            return if stack > bet {
                &[Fold, Call, Push]
            } else {
                &[Fold, Call]
            };
        }
        let can_limp = self.spot.allows_limp() && stack > BIG_BLIND;
        match (can_limp, can_raise) {
            (true, true) => &[Fold, Limp, Raise, Push],
            (true, false) => &[Fold, Limp, Push],
            (false, true) => &[Fold, Raise, Push],
            (false, false) => &[Fold, Push],
        }
    }

    fn apply(&self, state: &State, seat: usize, action: Action) -> State {
        let mut next = state.clone();
        next.next = (seat + 1) % 3;
        next.acted[seat] = true;
        let stack = self.stacks[seat];
        match action {
            Action::Fold => {
                next.in_hand[seat] = false;
                return next;
            }
            Action::Check => return next,
            Action::Limp => next.commit[seat] = BIG_BLIND,
            Action::Raise => {
                next.commit[seat] = MIN_RAISE;
                next.raised = true;
                next.acted = [false; 3];
                next.acted[seat] = true;
            }
            Action::Push => {
                next.commit[seat] = stack;
                next.all_in = true;
            }
            Action::Call if state.all_in => next.commit[seat] = stack,
            Action::Call => {
                next.commit[seat] = Rules::max_commit(state).min(stack);
                next.all_in = next.commit[seat] == stack;
            }
        }
        next.steps.push((Position::ALL[seat], action));
        next
    }

    fn payoff(&self, state: &State) -> Payoff {
        let live = |p: usize| state.in_hand[p];
        let in_hand = (0..3).filter(|&p| live(p)).count();
        if state.all_in || in_hand == 1 {
            return showdown_payoff(&state.commit, live);
        }
        flop_payoff(state, self.spot)
    }
}

/// Every player still in has matched the bet and has chips behind: each wins
/// its realization factor times its equity times the pot.
fn flop_payoff(state: &State, spot: &Spot) -> Payoff {
    let pot: f64 = state.commit.iter().sum();
    let mut payoff = Payoff {
        constant: state.commit.map(|c| -c),
        ..Payoff::default()
    };
    let r = spot.realization();
    let (btn, sb, bb) = (0, 1, 2);
    match state.in_hand {
        [true, true, true] => {
            let RealizationFactors {
                three_way_btn,
                three_way_sb,
                three_way_bb,
                ..
            } = *r;
            payoff.terms[btn][BTN_SHARE3] += pot * three_way_btn;
            payoff.terms[sb][SB_SHARE3] += pot * three_way_sb;
            // The BB's three-way share is what the two others do not win.
            payoff.constant[bb] += pot * three_way_bb;
            payoff.terms[bb][BTN_SHARE3] -= pot * three_way_bb;
            payoff.terms[bb][SB_SHARE3] -= pot * three_way_bb;
        }
        in_hand => {
            let (first, second, term) = match in_hand {
                [true, true, false] => (btn, sb, BTN_SB),
                [true, false, true] => (btn, bb, BTN_BB),
                _ => (sb, bb, SB_BB),
            };
            // The BTN acts last after the flop, then the BB, then the SB;
            // heads-up, the SB has the button and acts last.
            let first_in_position = first == btn || spot.is_heads_up();
            let (r_first, r_second) = if first_in_position {
                (r.in_position, r.out_of_position)
            } else {
                (r.out_of_position, r.in_position)
            };
            payoff.terms[first][term] += pot * r_first;
            payoff.constant[second] += pot * r_second;
            payoff.terms[second][term] -= pot * r_second;
        }
    }
    debug_assert!(payoff.terms.iter().flatten().all(|t| t.is_finite()));
    debug_assert_eq!(payoff.terms[0].len(), NUM_TERMS);
    payoff
}
