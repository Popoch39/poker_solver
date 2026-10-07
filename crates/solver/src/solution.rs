use std::io;

use crate::cfr::{self, Game, Table};
use crate::hand::{HandClass, NUM_CLASSES};
use crate::heads_up::HeadsUpPushFold;
use crate::spot::{Position, Spot};
use crate::three_max::ThreeMaxPushFold;
use crate::tree::{Action, Node};
use crate::tree_game::TreeGame;

/// How long to run the solver: until the exploitability target is met, or
/// for at most `iterations`.
#[derive(Clone, Debug, PartialEq)]
pub struct SolveOptions {
    /// Maximum number of iterations.
    pub iterations: u32,
    /// Stop as soon as the exploitability falls below this, in BB per hand
    /// (checked every few iterations); `None` runs every iteration.
    pub target_exploitability: Option<f64>,
}

impl Default for SolveOptions {
    /// A tenth of a milli-big-blind per hand: below the 3-max chance model's
    /// own sampling error, so iterating further would not make the strategy
    /// more accurate.
    fn default() -> Self {
        SolveOptions {
            iterations: 1000,
            target_exploitability: Some(1e-5),
        }
    }
}

/// Computes an equilibrium strategy for `spot`; with locked nodes, the
/// strategy of every free node that is an equilibrium of the game where the
/// locked nodes always play their given frequencies.
pub fn solve(spot: &Spot, options: &SolveOptions) -> Solution {
    if spot.has_flop() {
        solve_game(spot, options, &TreeGame::new(spot))
    } else if spot.is_heads_up() {
        solve_game(spot, options, &HeadsUpPushFold::new(spot))
    } else {
        solve_game(spot, options, &ThreeMaxPushFold::new(spot))
    }
}

fn solve_game(spot: &Spot, options: &SolveOptions, game: &impl Game) -> Solution {
    let locked: Vec<Option<Vec<f64>>> = game
        .nodes()
        .iter()
        .zip(game.actions())
        .map(|(&n, actions)| spot.locked(n, actions))
        .collect();
    let locks: Vec<Option<&[f64]>> = locked.iter().map(Option::as_deref).collect();
    let run = cfr::solve(
        game,
        options.iterations,
        options.target_exploitability,
        &locks,
    );
    // Players without a decision cannot deviate: they gain nothing.
    let per_player = spot
        .positions()
        .into_iter()
        .map(|position| {
            let gain = run.exploitability.iter().find(|(p, _)| *p == position);
            (position, gain.map_or(0.0, |(_, gain)| *gain))
        })
        .collect();
    Solution {
        spot: spot.clone(),
        iterations: run.iterations,
        profile: run.profile,
        exploitability: Exploitability { per_player },
    }
}

/// The solved strategy of a spot, queryable by (node, hand class).
#[derive(Clone, Debug)]
pub struct Solution {
    spot: Spot,
    iterations: u32,
    profile: Table,
    exploitability: Exploitability,
}

impl Solution {
    pub fn spot(&self) -> &Spot {
        &self.spot
    }

    /// Number of iterations actually run (fewer than asked when the target
    /// exploitability was reached first).
    pub fn iterations(&self) -> u32 {
        self.iterations
    }

    /// The decision nodes of the spot's tree, in the order they are played.
    /// A player all-in from the blind, or with nothing to call, has no node.
    pub fn nodes(&self) -> &[Node] {
        self.profile.nodes()
    }

    /// The legal actions at `node` in this spot, the passive one (fold or
    /// check) first, or `None` if the node is not in this spot's tree.
    pub fn actions(&self, node: Node) -> Option<&'static [Action]> {
        let n = self.nodes().iter().position(|&x| x == node)?;
        Some(self.profile.actions(n))
    }

    /// The action frequencies of `hand` at `node`, or `None` if the node is
    /// not in this spot's tree.
    pub fn strategy(&self, node: Node, hand: HandClass) -> Option<Strategy> {
        let n = self.nodes().iter().position(|&x| x == node)?;
        let frequencies = self.profile.infoset(n, hand.index());
        Some(Strategy {
            frequencies: self
                .profile
                .actions(n)
                .iter()
                .copied()
                .zip(frequencies.iter().copied())
                .collect(),
        })
    }

    /// Share of the 1326 starting combos that take `action` at `node`, each
    /// hand counted by its frequency; `None` if the node is not in the tree.
    pub fn action_share(&self, node: Node, action: Action) -> Option<f64> {
        let mut combos = 0.0;
        for hand in HandClass::all() {
            combos += f64::from(hand.combos()) * self.strategy(node, hand)?.frequency(action);
        }
        Some(combos / 1326.0)
    }

    pub fn exploitability(&self) -> &Exploitability {
        &self.exploitability
    }

    /// What `position` wins per hand, in BB, by playing this solution's
    /// strategy instead of `baseline`'s while the other players play
    /// `opponents`' strategy.
    ///
    /// With a node-locked solution `exploit` and the equilibrium `eq`,
    /// `exploit.gain_over(&eq, p, &exploit)` is what exploiting the locked
    /// nodes gains, and `-exploit.gain_over(&eq, p, &eq)` what it costs
    /// against opponents who play the equilibrium after all.
    ///
    /// # Panics
    /// If the three solutions are not of the same table (same stacks,
    /// allowed actions and realization factors).
    pub fn gain_over(&self, baseline: &Solution, position: Position, opponents: &Solution) -> f64 {
        assert!(
            self.spot.same_table(&baseline.spot) && self.spot.same_table(&opponents.spot),
            "the solutions must be of the same table"
        );
        if self.spot.has_flop() {
            // A player can act again on a line: compare the values of whole
            // profiles rather than of single decisions.
            let game = TreeGame::new(&self.spot);
            let value = |ours: &Solution| {
                let mut profile = opponents.profile.clone();
                for (n, node) in self.nodes().iter().enumerate() {
                    if node.actor() == position {
                        for class in 0..NUM_CLASSES {
                            profile
                                .infoset_mut(n, class)
                                .copy_from_slice(ours.profile.infoset(n, class));
                        }
                    }
                }
                game.value(&profile, position)
            };
            value(self) - value(baseline)
        } else if self.spot.is_heads_up() {
            self.gain_in(
                &HeadsUpPushFold::new(&self.spot),
                baseline,
                position,
                opponents,
            )
        } else {
            self.gain_in(
                &ThreeMaxPushFold::new(&self.spot),
                baseline,
                position,
                opponents,
            )
        }
    }

    /// A player acts at most once per line, so its action values depend on
    /// the others only, and the lines where it does not act are the same for
    /// both of its strategies: the gain is the difference of its strategies
    /// weighted by the action values under the opponents' profile.
    fn gain_in(
        &self,
        game: &impl Game,
        baseline: &Solution,
        position: Position,
        opponents: &Solution,
    ) -> f64 {
        let values = game.action_values(&opponents.profile);
        let mut gain = 0.0;
        for (n, node) in self.nodes().iter().enumerate() {
            if node.actor() != position {
                continue;
            }
            for class in 0..NUM_CLASSES {
                let ours = self.profile.infoset(n, class);
                let theirs = baseline.profile.infoset(n, class);
                for ((s, b), v) in ours.iter().zip(theirs).zip(values.infoset(n, class)) {
                    gain += (s - b) * v;
                }
            }
        }
        gain
    }

    /// Writes the strategy as CSV: a `node,position,hand,action,frequency`
    /// header, then one row per node, hand class and legal action.
    pub fn write_csv(&self, mut out: impl io::Write) -> io::Result<()> {
        writeln!(out, "node,position,hand,action,frequency")?;
        for &node in self.nodes() {
            for hand in HandClass::all() {
                let strategy = self.strategy(node, hand).expect("the node is in the tree");
                for (action, frequency) in strategy.iter() {
                    let (id, actor) = (node.id(), node.actor());
                    writeln!(out, "{id},{actor},{hand},{action},{frequency:.6}")?;
                }
            }
        }
        Ok(())
    }
}

/// A mixed strategy at one information set: a frequency for each legal action.
#[derive(Clone, Debug, PartialEq)]
pub struct Strategy {
    frequencies: Vec<(Action, f64)>,
}

impl Strategy {
    /// A strategy from the frequency of each action (an action left out has
    /// frequency 0), e.g. to lock a node with [`Spot::lock`].
    pub fn new(frequencies: impl IntoIterator<Item = (Action, f64)>) -> Strategy {
        Strategy {
            frequencies: frequencies.into_iter().collect(),
        }
    }

    /// Probability of taking `action`, between 0 and 1 (0 if it is not legal).
    pub fn frequency(&self, action: Action) -> f64 {
        self.frequencies
            .iter()
            .find(|(a, _)| *a == action)
            .map_or(0.0, |(_, f)| *f)
    }

    /// The legal actions with their frequencies.
    pub fn iter(&self) -> impl Iterator<Item = (Action, f64)> + '_ {
        self.frequencies.iter().copied()
    }
}

/// How far a solution is from an equilibrium: what each player would win per
/// hand, in BB, by switching to a best response while the others keep the
/// solution's strategy.
#[derive(Clone, Debug, PartialEq)]
pub struct Exploitability {
    per_player: Vec<(Position, f64)>,
}

impl Exploitability {
    /// Sum of the players' best-response gains (NashConv), in BB per hand.
    pub fn total(&self) -> f64 {
        self.per_player.iter().map(|(_, gain)| gain).sum()
    }

    /// Best-response gain of `position`, in BB per hand (0 for a player who
    /// is not in the spot).
    pub fn of(&self, position: Position) -> f64 {
        self.per_player
            .iter()
            .find(|(p, _)| *p == position)
            .map_or(0.0, |(_, gain)| *gain)
    }

    /// Each player's best-response gain, in BB per hand.
    pub fn per_player(&self) -> &[(Position, f64)] {
        &self.per_player
    }
}
