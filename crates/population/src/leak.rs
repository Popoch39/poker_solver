use std::collections::{BTreeMap, HashMap, HashSet};

use nitro_hh::Hand;
use nitro_solver::{HandClass, Node, Solution, SolveOptions, Spot, solve};
use rayon::prelude::*;

use crate::bucket::StackBucket;
use crate::ev::Evaluator;
use crate::model::{Players, PopulationModel, TREE_ORDER};
use crate::stats::grid_index;

/// Settings of a [`LeakReport`].
#[derive(Clone, Debug, PartialEq)]
pub struct LeakOptions {
    /// Number of decisions below which a node is flagged inconclusive.
    pub min_sample: u32,
    /// Monte Carlo deals per hand class and node to estimate the EVs.
    pub samples: u32,
    /// How the representative spot of each stack bucket is solved.
    pub solve: SolveOptions,
}

impl Default for LeakOptions {
    /// 50 decisions put the standard error of a frequency near 7 points at
    /// worst; 20 000 deals put that of an EV near 0.1 BB.
    fn default() -> Self {
        LeakOptions {
            min_sample: 50,
            samples: 20_000,
            solve: SolveOptions::default(),
        }
    }
}

/// How the hero plays one node, for one stack bucket, next to the
/// equilibrium.
///
/// Frequencies are those of the node's push (or call):
/// push / (push + fold).
#[derive(Clone, Debug, PartialEq)]
pub struct Leak {
    pub node: Node,
    pub bucket: StackBucket,
    /// Number of the hero's decisions at the node.
    pub sample: u32,
    pub hero_frequency: f64,
    /// Effective stack, in BB, of the spot solved for the bucket: every
    /// player has it.
    pub equilibrium_stack: f64,
    /// Share of the 1 326 combos that push (or call) at equilibrium; `None`
    /// when the solved spot has no decision at the node.
    pub equilibrium_frequency: Option<f64>,
    /// Estimated chips the hero lost at the node by not playing the
    /// equilibrium, summed over their decisions; `None` with no equilibrium.
    ///
    /// For each decision, with the hero's cards known, it is the EV of the
    /// equilibrium's mix of actions for that hand class minus the EV of the
    /// action taken, both against opponents who play the equilibrium, in BB
    /// times the hand's big blind. The EVs come from the bucket's solved
    /// spot (see [`Leak::equilibrium_stack`]) by Monte Carlo: the hero is
    /// dealt a combo of the class, the players who acted before a hand drawn
    /// from what they did at equilibrium, the players still to act a random
    /// hand that then pushes or calls at its equilibrium frequency, and a
    /// board settles the pot. A hero who plays the equilibrium mix of every
    /// class loses nothing whatever the sampling error; a deviation can only
    /// cost (up to that error), since nobody gains by leaving an
    /// equilibrium alone.
    ///
    /// It holds the opponents at equilibrium, not at what the population
    /// does, and prices every hand at the bucket's representative stacks.
    pub chips_lost: Option<f64>,
    /// The sample is below [`LeakOptions::min_sample`].
    pub inconclusive: bool,
}

/// The hero's frequencies node by node, compared to the equilibrium and
/// sorted by estimated impact.
///
/// The hero is named when the report is built and only their seat is used
/// afterwards: nothing in the report names a player.
#[derive(Clone, Debug, PartialEq)]
pub struct LeakReport {
    hands: usize,
    leaks: Vec<Leak>,
}

impl LeakReport {
    /// Compares the decisions of `hero` to the equilibrium.
    ///
    /// Only the hands whose history belongs to `hero` (the account owner,
    /// whose cards are always known) are read; the others are skipped. Each
    /// stack bucket met is compared to one spot, solved once, where every
    /// player has the bucket's representative stack: its middle (1.5 BB for
    /// the shortest bucket, so that the BB still decides, and 20 BB above
    /// 15 BB).
    pub fn build<'a>(
        hands: impl IntoIterator<Item = &'a Hand>,
        hero: &str,
        options: &LeakOptions,
    ) -> LeakReport {
        // Losses are counted in chips, so the hands are modelled per big
        // blind and priced at their own.
        let mut by_big_blind: BTreeMap<u32, Vec<&Hand>> = BTreeMap::new();
        for hand in hands.into_iter().filter(|hand| owned_by(hand, hero)) {
            by_big_blind.entry(hand.big_blind).or_default().push(hand);
        }
        let hands = by_big_blind.values().map(Vec::len).sum();

        let mut tallies: HashMap<(Node, StackBucket), Tally> = HashMap::new();
        for (big_blind, group) in by_big_blind {
            let model = PopulationModel::build(group, Players::Hero);
            for (node, bucket, stats) in model.entries() {
                let tally = tallies.entry((node, bucket)).or_default();
                tally.sample += stats.sample();
                tally.aggressive += stats.count(aggressive(node));
                for (i, &action) in node.actions().iter().enumerate() {
                    let known = stats.known_hands(action);
                    for class in HandClass::all().filter(|&c| known.count(c) > 0) {
                        tally.decisions.push(Decision {
                            class,
                            action: i,
                            count: known.count(class),
                            big_blind,
                        });
                    }
                }
            }
        }

        let buckets: HashSet<StackBucket> = tallies.keys().map(|&(_, bucket)| bucket).collect();
        let solutions: HashMap<StackBucket, Solution> = buckets
            .into_iter()
            .map(|bucket| {
                let stack = equilibrium_stack(bucket);
                let spot = Spot::three_max(stack, stack, stack).expect("a positive stack");
                (bucket, solve(&spot, &options.solve))
            })
            .collect();
        let evaluators: HashMap<StackBucket, Evaluator> = solutions
            .iter()
            .map(|(&bucket, solution)| (bucket, Evaluator::new(solution)))
            .collect();
        let needed: HashSet<(StackBucket, Node, HandClass)> = tallies
            .iter()
            .flat_map(|(&(node, bucket), tally)| {
                tally.decisions.iter().map(move |d| (bucket, node, d.class))
            })
            .collect();
        let evs: HashMap<(StackBucket, Node, HandClass), Option<[f64; 2]>> = needed
            .into_par_iter()
            .map(|key @ (bucket, node, class)| {
                let seed = seed(bucket, node, class);
                let evs = evaluators[&bucket].action_evs(node, class, options.samples, seed);
                (key, evs)
            })
            .collect();

        let mut leaks: Vec<Leak> = tallies
            .into_iter()
            .map(|((node, bucket), tally)| {
                let solution = &solutions[&bucket];
                let equilibrium_frequency = solution.action_share(node, aggressive(node));
                let chips_lost = equilibrium_frequency.map(|_| {
                    tally
                        .decisions
                        .iter()
                        .filter_map(|d| {
                            let evs = evs[&(bucket, node, d.class)]?;
                            let strategy = solution.strategy(node, d.class)?;
                            let mix: f64 = node
                                .actions()
                                .iter()
                                .zip(evs)
                                .map(|(&action, ev)| strategy.frequency(action) * ev)
                                .sum();
                            let regret = mix - evs[d.action];
                            Some(regret * f64::from(d.count) * f64::from(d.big_blind))
                        })
                        .sum()
                });
                Leak {
                    node,
                    bucket,
                    sample: tally.sample,
                    hero_frequency: f64::from(tally.aggressive) / f64::from(tally.sample),
                    equilibrium_stack: equilibrium_stack(bucket),
                    equilibrium_frequency,
                    chips_lost,
                    inconclusive: tally.sample < options.min_sample,
                }
            })
            .collect();
        leaks.sort_by(|a, b| {
            let lost = |leak: &Leak| leak.chips_lost.unwrap_or(f64::NEG_INFINITY);
            a.inconclusive
                .cmp(&b.inconclusive)
                .then(lost(b).total_cmp(&lost(a)))
                .then(tree_order(a.node).cmp(&tree_order(b.node)))
                .then(a.bucket.cmp(&b.bucket))
        });
        LeakReport { hands, leaks }
    }

    /// Number of the hero's hands read.
    pub fn hands(&self) -> usize {
        self.hands
    }

    /// Every (node, bucket) where the hero decided at least once: the
    /// conclusive ones first, each group from the largest estimated loss to
    /// the smallest, nodes without an equilibrium last.
    pub fn leaks(&self) -> &[Leak] {
        &self.leaks
    }
}

/// The hero's decisions at one (node, bucket), over every big blind.
#[derive(Default)]
struct Tally {
    sample: u32,
    aggressive: u32,
    decisions: Vec<Decision>,
}

/// Identical decisions: the same action with the same hand class at the
/// same big blind.
struct Decision {
    class: HandClass,
    /// Index in [`Node::actions`].
    action: usize,
    count: u32,
    big_blind: u32,
}

fn owned_by(hand: &Hand, hero: &str) -> bool {
    hand.hero.is_some_and(|seat| {
        hand.players
            .iter()
            .any(|player| player.seat == seat && player.name == hero)
    })
}

fn equilibrium_stack(bucket: StackBucket) -> f64 {
    match bucket.upper() {
        Some(upper) => (bucket.lower().max(1.0) + upper) / 2.0,
        None => bucket.lower() + 5.0,
    }
}

fn aggressive(node: Node) -> nitro_solver::Action {
    *node.actions().last().expect("a node has actions")
}

fn tree_order(node: Node) -> usize {
    TREE_ORDER
        .iter()
        .position(|&n| n == node)
        .expect("every node")
}

/// A seed per evaluation, so that a report does not depend on the order the
/// threads run in.
fn seed(bucket: StackBucket, node: Node, class: HandClass) -> u64 {
    bucket.lower().to_bits() ^ ((tree_order(node) as u64) << 8 | grid_index(class) as u64)
}
