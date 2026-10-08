//! Chip EV of the actions at a push/fold node for one hand class, when every
//! other player follows a solved strategy.
//!
//! The solver only exposes strategies, so the EVs are estimated by Monte
//! Carlo from them: deal the hero a combo of the class, the players who acted
//! before the node a hand from the range of what they did (their push, call
//! or fold frequencies), the players still to act a random hand, and a
//! board; then settle every line the players still to act can take, each
//! weighted by its frequency at equilibrium.

use nitro_solver::{Action, HandClass, Node, Position, Solution};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use rs_poker::core::{Card, Rank, SevenCardAccum};

use crate::model::TREE_ORDER;
use crate::stats::grid_index;

const POSITIONS: [Position; 3] = [Position::Btn, Position::Sb, Position::Bb];

/// Blinds in BB, indexed like [`POSITIONS`].
const BLINDS: [f64; 3] = [0.0, 0.5, 1.0];

/// Draws beyond which a class is given up on, per sample asked: a hand
/// that blocks a whole range would otherwise never be dealt.
const MAX_DRAWS_PER_SAMPLE: u32 = 20;

/// The equilibrium of one spot, read once for the many deals.
pub(crate) struct Evaluator {
    /// Stacks in BB, indexed like [`POSITIONS`] (0 for an empty seat).
    stacks: [f64; 3],
    /// Push or call frequency per node (indexed like [`TREE_ORDER`]) and hand
    /// class (by grid index); `None` for a node the spot does not reach.
    aggressive: [Option<Vec<f64>>; 6],
    combos: Vec<Combo>,
}

/// Two hole cards, with the bit of each card in a 52-bit mask.
#[derive(Clone, Copy)]
struct Combo {
    cards: [Card; 2],
    mask: u64,
    class: usize,
}

impl Evaluator {
    pub(crate) fn new(solution: &Solution) -> Evaluator {
        let spot = solution.spot();
        let aggressive = TREE_ORDER.map(|node| {
            let action = node.aggressive_action();
            HandClass::all()
                .map(|class| Some(solution.strategy(node, class)?.frequency(action)))
                .collect::<Option<Vec<f64>>>()
        });
        Evaluator {
            stacks: POSITIONS.map(|p| spot.stack(p)),
            aggressive,
            combos: all_combos(),
        }
    }

    /// EVs in BB of the actions at `node` (in the order of
    /// [`Node::actions`]) for a hero holding `class`, from `samples` deals;
    /// `None` if the spot never reaches the node, or never with this class.
    pub(crate) fn action_evs(
        &self,
        node: Node,
        class: HandClass,
        samples: u32,
        seed: u64,
    ) -> Option<[f64; 2]> {
        self.frequencies(node)?;
        let hero = index(node.actor());
        let fold = -BLINDS[hero].min(self.stacks[hero]);
        let class = grid_index(class);
        let mine: Vec<Combo> = self
            .combos
            .iter()
            .copied()
            .filter(|c| c.class == class)
            .collect();
        let before = self.ranges_before(node)?;
        let after: Vec<usize> = (hero + 1..3).filter(|&i| self.stacks[i] > 0.0).collect();
        let live = before
            .iter()
            .filter(|(_, action, _)| *action != Action::Fold)
            .fold(1 << hero, |live, (i, _, _)| live | 1 << i);

        let mut rng = StdRng::seed_from_u64(seed);
        let (mut total, mut dealt) = (0.0, 0);
        for _ in 0..samples * MAX_DRAWS_PER_SAMPLE {
            if dealt == samples {
                break;
            }
            let Some(deal) = self.deal(hero, &mine, &before, &mut rng) else {
                continue;
            };
            dealt += 1;
            total += self.settle(&after, live, &deal)[hero];
        }
        (dealt > 0).then(|| [fold, total / f64::from(dealt)])
    }

    /// The players who acted before `node`, each with what they did and the
    /// cumulative weights of the combos they do it with.
    fn ranges_before(&self, node: Node) -> Option<Vec<(usize, Action, Vec<f64>)>> {
        let path: &[(Node, Action)] = match node {
            Node::BtnOpen => &[],
            Node::SbVsBtnPush => &[(Node::BtnOpen, Action::Push)],
            Node::SbOpen => &[(Node::BtnOpen, Action::Fold)],
            Node::BbVsBtnPush => &[
                (Node::BtnOpen, Action::Push),
                (Node::SbVsBtnPush, Action::Fold),
            ],
            Node::BbVsBtnPushSbCall => &[
                (Node::BtnOpen, Action::Push),
                (Node::SbVsBtnPush, Action::Call),
            ],
            Node::BbVsSbPush => &[(Node::BtnOpen, Action::Fold), (Node::SbOpen, Action::Push)],
            // Reached through a limp or a min-raise: not a push/fold line.
            Node::Line(_) => return None,
        };
        path.iter()
            .filter(|(node, _)| self.stacks[index(node.actor())] > 0.0)
            .map(|&(node, action)| {
                let frequencies = self.frequencies(node)?;
                let mut sum = 0.0;
                let cumulative = self
                    .combos
                    .iter()
                    .map(|combo| {
                        let push = frequencies[combo.class];
                        sum += if action == Action::Fold {
                            1.0 - push
                        } else {
                            push
                        };
                        sum
                    })
                    .collect();
                (sum > 0.0).then_some((index(node.actor()), action, cumulative))
            })
            .collect()
    }

    fn frequencies(&self, node: Node) -> Option<&[f64]> {
        self.aggressive[TREE_ORDER.iter().position(|&n| n == node)?].as_deref()
    }

    /// Deals the hero one of `mine`, the players who acted a hand from their
    /// range, the others a random hand, and a board; `None` when the hands
    /// drawn from ranges collide, so that the deal is drawn from the joint
    /// ranges without any bias from card removal.
    fn deal(
        &self,
        hero: usize,
        mine: &[Combo],
        before: &[(usize, Action, Vec<f64>)],
        rng: &mut StdRng,
    ) -> Option<Deal> {
        let mut hands = [None; 3];
        hands[hero] = Some(mine[rng.random_range(0..mine.len())]);
        let mut used = hands[hero].map_or(0, |c| c.mask);
        for (seat, _, cumulative) in before {
            let total = *cumulative.last().expect("1326 combos");
            let draw = rng.random::<f64>() * total;
            let combo = self.combos[cumulative.partition_point(|&w| w <= draw)];
            if combo.mask & used != 0 {
                return None;
            }
            used |= combo.mask;
            hands[*seat] = Some(combo);
        }
        let mut cards = [[Card::from(0); 2]; 3];
        let mut classes = [0; 3];
        for i in 0..3 {
            if self.stacks[i] == 0.0 {
                continue;
            }
            let combo = hands[i].unwrap_or_else(|| {
                let a = draw_card(&mut used, rng);
                let b = draw_card(&mut used, rng);
                combo_of(a, b)
            });
            cards[i] = combo.cards;
            classes[i] = combo.class;
        }
        let board: [Card; 5] = std::array::from_fn(|_| draw_card(&mut used, rng));
        let ranks = cards.map(|hole| {
            let mut hand = SevenCardAccum::new();
            for card in hole.into_iter().chain(board) {
                hand.add(card);
            }
            hand.rank()
        });
        Some(Deal { classes, ranks })
    }

    /// Each player's net chips in BB once the players in `after` have
    /// acted, averaged over what they do at equilibrium; `live` is the mask
    /// of the players already all-in.
    fn settle(&self, after: &[usize], live: u8, deal: &Deal) -> [f64; 3] {
        let Some((&seat, rest)) = after.split_first() else {
            return self.showdown(live, &deal.ranks);
        };
        // A player with no decision at the node (all-in from the blind)
        // stays in.
        let call = self
            .frequencies(facing_push(seat, live))
            .map_or(1.0, |f| f[deal.classes[seat]]);
        let mut net = [0.0; 3];
        for (weight, live) in [(call, live | 1 << seat), (1.0 - call, live)] {
            if weight > 0.0 {
                let line = self.settle(rest, live, deal);
                for i in 0..3 {
                    net[i] += weight * line[i];
                }
            }
        }
        net
    }

    /// Net chips in BB when the players in `live` go to showdown and the
    /// others have folded their blinds.
    ///
    /// The pot is cut into layers at each player's commitment; each layer
    /// goes to the best hand among the live players who reached it, or back
    /// to those who put it in when none did.
    fn showdown(&self, live: u8, ranks: &[Rank; 3]) -> [f64; 3] {
        let committed: [f64; 3] = std::array::from_fn(|i| {
            if live & 1 << i != 0 {
                self.stacks[i]
            } else {
                BLINDS[i].min(self.stacks[i])
            }
        });
        let mut levels: Vec<f64> = committed.iter().copied().filter(|&c| c > 0.0).collect();
        levels.sort_by(f64::total_cmp);
        levels.dedup();
        let mut won = [0.0; 3];
        let mut floor = 0.0;
        for level in levels {
            let layer = level - floor;
            let reached: Vec<usize> = (0..3).filter(|&i| committed[i] >= level).collect();
            let contenders: Vec<usize> = reached
                .iter()
                .copied()
                .filter(|&i| live & 1 << i != 0)
                .collect();
            match contenders.iter().map(|&i| ranks[i]).max() {
                Some(best) => {
                    let winners: Vec<usize> = contenders
                        .into_iter()
                        .filter(|&i| ranks[i] == best)
                        .collect();
                    let share = layer * reached.len() as f64 / winners.len() as f64;
                    for i in winners {
                        won[i] += share;
                    }
                }
                None => {
                    for i in reached {
                        won[i] += layer;
                    }
                }
            }
            floor = level;
        }
        std::array::from_fn(|i| won[i] - committed[i])
    }
}

struct Deal {
    /// Grid index of each player's hand class.
    classes: [usize; 3],
    ranks: [Rank; 3],
}

/// The node of `seat` when the players in `live` have gone all-in before it.
fn facing_push(seat: usize, live: u8) -> Node {
    match (seat, live & 1 != 0, live & 2 != 0) {
        (1, true, _) => Node::SbVsBtnPush,
        (_, true, true) => Node::BbVsBtnPushSbCall,
        (_, true, false) => Node::BbVsBtnPush,
        _ => Node::BbVsSbPush,
    }
}

fn index(position: Position) -> usize {
    POSITIONS
        .iter()
        .position(|&p| p == position)
        .expect("three positions")
}

fn draw_card(used: &mut u64, rng: &mut StdRng) -> Card {
    loop {
        let card = rng.random_range(0..52u8);
        if *used & 1 << card == 0 {
            *used |= 1 << card;
            return Card::from(card);
        }
    }
}

fn all_combos() -> Vec<Combo> {
    (0..52u8)
        .flat_map(|a| (a + 1..52).map(move |b| combo_of(Card::from(a), Card::from(b))))
        .collect()
}

fn combo_of(a: Card, b: Card) -> Combo {
    // Grid rows and columns go from the ace down; rs_poker values go up.
    let row = |card: Card| 12 - usize::from(u8::from(card.value));
    let (high, low) = (row(a).min(row(b)), row(a).max(row(b)));
    let class = if high == low || a.suit != b.suit {
        // Pairs on the diagonal, offsuit hands below it.
        low * 13 + high
    } else {
        high * 13 + low
    };
    Combo {
        cards: [a, b],
        mask: 1 << u8::from(a) | 1 << u8::from(b),
        class,
    }
}
