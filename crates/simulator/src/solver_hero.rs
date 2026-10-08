//! The hero that plays the solver's strategy: the equilibrium, or the
//! exploit of a population (node-locking).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use nitro_population::{PopulationModel, StackBucket};
use nitro_solver::{Action, Position as TreePosition, Solution, SolveOptions, Spot, solve};
use rand::{Rng, RngExt};

use crate::apart;
use crate::push_fold::{self, Reading};
use crate::seat::{Decision, Position, SeatStrategy, SeatView};

/// A seat that plays the solver's push/fold strategy for the stacks at
/// hand, solved the first time they are met and cached.
///
/// - **Stacks** are rounded before solving, so that nearby stacks share a
///   solve: to 0.25 BB below 2 BB, 0.5 BB below 5 BB, 1 BB below 16 BB,
///   2 BB below 20 BB and 5 BB above, but never out of the population's
///   stack bucket ([`StackBucket`]). Chips nobody can win are left out: a
///   3-max stack is capped at the second largest, a heads-up one at the
///   effective stack.
/// - **Exploit**: the spot is locked on the population before it is solved
///   ([`PopulationModel::lock`]), for the hero's position, and solved again
///   for each position the hero meets it from.
/// - **Off the tree**, the hero follows the fallback policy of the
///   push/fold strategies (ADR 0007): check or call postflop, a limp or a
///   raise short of all-in answered as a push.
///
/// Solves run one at a time in a thread pool of their own, every core
/// helping, apart from the games that wait for them.
pub struct SolverHero {
    name: &'static str,
    exploit: Option<(Arc<PopulationModel>, u32)>,
    options: SolveOptions,
    cache: Mutex<HashMap<Key, Arc<OnceLock<Arc<Solution>>>>>,
    solving: Mutex<Duration>,
}

/// A rounded spot, in quarters of a BB per position, and the hero's
/// position for an exploit.
type Key = ([u32; 3], Option<TreePosition>);

impl SolverHero {
    /// Plays the equilibrium strategy of each spot.
    pub fn equilibrium(options: SolveOptions) -> SolverHero {
        SolverHero::new("solver-equilibrium", None, options)
    }

    /// Plays the exploit of `population`: every opponent node it observed at
    /// least `min_sample` times is locked on its frequencies.
    pub fn exploit(
        population: Arc<PopulationModel>,
        min_sample: u32,
        options: SolveOptions,
    ) -> SolverHero {
        SolverHero::new("solver-exploit", Some((population, min_sample)), options)
    }

    fn new(
        name: &'static str,
        exploit: Option<(Arc<PopulationModel>, u32)>,
        options: SolveOptions,
    ) -> SolverHero {
        SolverHero {
            name,
            exploit,
            options,
            cache: Mutex::default(),
            solving: Mutex::default(),
        }
    }

    /// Number of distinct (rounded) spots solved so far.
    pub fn spots_solved(&self) -> usize {
        lock(&self.cache)
            .values()
            .filter(|s| s.get().is_some())
            .count()
    }

    /// Time spent solving so far, summed over the solves.
    pub fn solving_time(&self) -> Duration {
        *lock(&self.solving)
    }

    fn solution(&self, spot: &Spot, hero: TreePosition) -> Arc<Solution> {
        let spot = rounded(spot);
        let position = self.exploit.as_ref().map(|_| hero);
        let key = (
            [TreePosition::Btn, TreePosition::Sb, TreePosition::Bb]
                .map(|p| (spot.stack(p) * 4.0).round() as u32),
            position,
        );
        let cell = Arc::clone(lock(&self.cache).entry(key).or_default());
        Arc::clone(cell.get_or_init(|| {
            // Locking ranks hands by an equity table the solver builds
            // lazily: it runs apart from the games, like the solve.
            let (solution, took) = apart::run(|| {
                let spot = match &self.exploit {
                    Some((population, min_sample)) => population.lock(spot, hero, *min_sample),
                    None => spot,
                };
                solve(&spot, &self.options)
            });
            *lock(&self.solving) += took;
            Arc::new(solution)
        }))
    }
}

impl SeatStrategy for SolverHero {
    fn name(&self) -> &str {
        self.name
    }

    fn decide(&self, view: &SeatView, rng: &mut dyn Rng) -> Decision {
        let decision = match push_fold::read(view) {
            Reading::CallDown => return Decision::Call,
            Reading::Tree(decision) => decision,
        };
        let solution = self.solution(&decision.spot, tree_position(view.position));
        // The rounding can leave the player without a decision (all-in from
        // the blind): it has nothing left to decide.
        let Some(strategy) = solution.strategy(decision.node, decision.hand) else {
            return Decision::Call;
        };
        let aggressive = *decision.node.actions().last().expect("a node has actions");
        let action = if rng.random::<f64>() < strategy.frequency(aggressive) {
            aggressive
        } else {
            Action::Fold
        };
        decision.decision(action)
    }
}

fn tree_position(position: Position) -> TreePosition {
    match position {
        Position::Button => TreePosition::Btn,
        Position::SmallBlind => TreePosition::Sb,
        Position::BigBlind => TreePosition::Bb,
    }
}

/// `spot` with its stacks rounded, and capped where chips cannot be won.
fn rounded(spot: &Spot) -> Spot {
    let positions = spot.positions();
    let mut stacks: Vec<f64> = positions.iter().map(|&p| round(spot.stack(p))).collect();
    let mut sorted = stacks.clone();
    sorted.sort_by(|a, b| b.total_cmp(a));
    let cap = sorted[1];
    for stack in &mut stacks {
        *stack = stack.min(cap);
    }
    match stacks[..] {
        [sb, bb] => Spot::heads_up(sb, bb),
        [btn, sb, bb] => Spot::three_max(btn, sb, bb),
        _ => unreachable!("a spot seats two or three players"),
    }
    .expect("rounded stacks stay positive")
}

/// The stack grid: finer where push/fold play changes quickly, and never
/// across a population stack bucket, so that an exploit is locked on the
/// population met at the stacks really played.
fn round(bb: f64) -> f64 {
    let step = match bb {
        _ if bb < 2.0 => 0.25,
        _ if bb < 5.0 => 0.5,
        _ if bb < 16.0 => 1.0,
        _ if bb < 20.0 => 2.0,
        _ => 5.0,
    };
    let rounded = ((bb / step).round() * step).max(0.25);
    let bucket = StackBucket::of(bb);
    match StackBucket::of(rounded) {
        // Buckets are wider than a step: one step back lands inside.
        b if b < bucket => rounded + step,
        b if b > bucket => rounded - step,
        _ => rounded,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .expect("no thread panics while holding the hero's locks")
}
