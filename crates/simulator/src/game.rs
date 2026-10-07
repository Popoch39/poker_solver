//! One Expresso Nitro, played hand by hand on the `rs_poker` arena.
//!
//! The arena plays each hand; this module runs the tournament around it:
//! rising blinds, button moves, eliminations and places. rs_poker's own
//! `SingleTableTournament` cannot do it (see ADR 0002).

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use async_trait::async_trait;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rs_poker::arena::action::AgentAction;
use rs_poker::arena::game_state::Round;
use rs_poker::arena::{Agent, GameState, GameStateBuilder, HoldemSimulationBuilder};
use rs_poker::core::Card;

use crate::seat::{Decision, PlayerView, Position, SeatStrategy, SeatView, Street};
use crate::structure::Structure;

const SEATS: usize = 3;

/// A three-seat Expresso Nitro in progress.
pub struct NitroGame {
    structure: Structure,
    seats: [Arc<dyn SeatStrategy>; SEATS],
    stacks: [f32; SEATS],
    button: usize,
    hands_played: u32,
    /// Elapsed time in ticks: a level lasts `hands_per_level ×
    /// hands_per_level_heads_up` ticks, so both hand lengths are whole.
    clock: u32,
    places: [Option<u8>; SEATS],
    rng: StdRng,
}

/// What happened in one hand.
#[derive(Debug, Clone, PartialEq)]
pub struct HandSummary {
    /// Hand number in the game, from 1.
    pub number: u32,
    /// Blind level, from 1.
    pub level: u32,
    pub small_blind: f32,
    pub big_blind: f32,
    /// Seat of the button.
    pub button: usize,
    /// Stacks after the hand, by seat.
    pub stacks: [f32; SEATS],
    /// Seats eliminated in this hand.
    pub eliminated: Vec<usize>,
}

impl NitroGame {
    /// A new game; `seed` decides the first button and every card dealt.
    pub fn new(structure: Structure, seats: [Arc<dyn SeatStrategy>; SEATS], seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let button = rand::RngExt::random_range(&mut rng, 0..SEATS);
        Self {
            stacks: [structure.starting_stack as f32; SEATS],
            structure,
            seats,
            button,
            hands_played: 0,
            clock: 0,
            places: [None; SEATS],
            rng,
        }
    }

    /// Chips of each seat.
    pub fn stacks(&self) -> [f32; SEATS] {
        self.stacks
    }

    /// Seat of the button for the next hand.
    pub fn button(&self) -> usize {
        self.button
    }

    pub fn hands_played(&self) -> u32 {
        self.hands_played
    }

    /// Blind level of the next hand, from 1.
    pub fn level(&self) -> u32 {
        let ticks_per_level =
            self.structure.hands_per_level * self.structure.hands_per_level_heads_up;
        let last = self.structure.levels.len() as u32;
        (self.clock / ticks_per_level + 1).min(last)
    }

    pub fn is_over(&self) -> bool {
        self.places.iter().all(Option::is_some)
    }

    /// Finishing place of each seat (1 = winner), once the game is over.
    pub fn places(&self) -> Option<[u8; SEATS]> {
        let mut places = [0; SEATS];
        for (place, seat) in places.iter_mut().zip(self.places) {
            *place = seat?;
        }
        Some(places)
    }

    /// Plays the next hand, or returns `None` if the game is over.
    pub fn play_hand(&mut self) -> Option<HandSummary> {
        if self.is_over() {
            return None;
        }
        let number = self.hands_played + 1;
        let level = self.level();
        let blinds = self.structure.levels[level as usize - 1];
        let (small_blind, big_blind) = (blinds.small_blind as f32, blinds.big_blind as f32);
        let alive: Vec<usize> = (0..SEATS).filter(|&s| self.stacks[s] > 0.0).collect();
        let stacks: Vec<f32> = alive.iter().map(|&s| self.stacks[s]).collect();
        // Heads-up must be a 2-seat table: with a busted third seat, the arena
        // makes the button post the big blind.
        let state = GameStateBuilder::new()
            .stacks(&stacks)
            .blinds(big_blind, small_blind)
            .dealer_idx(
                alive
                    .iter()
                    .position(|&s| s == self.button)
                    .expect("button seat is alive"),
            )
            .max_raises_per_round(None)
            .build()
            .expect("at least two seats have chips");
        let agents: Vec<Box<dyn Agent>> = alive
            .iter()
            .map(|&seat| {
                Box::new(ArenaSeat {
                    strategy: Arc::clone(&self.seats[seat]),
                    table: alive.clone(),
                    hand_number: number,
                    level,
                    rng: StdRng::from_rng(&mut self.rng),
                }) as Box<dyn Agent>
            })
            .collect();
        let mut sim = HoldemSimulationBuilder::default()
            .game_state(state)
            .agents(agents)
            .build_with_rng(StdRng::from_rng(&mut self.rng))
            .expect("the table has a game state and one agent per seat");
        block_on(sim.run());

        let end = &sim.game_state;
        for (i, &seat) in alive.iter().enumerate() {
            self.stacks[seat] = end.stacks[i];
        }
        // Two players busted in the same hand: the smaller stack finishes
        // lower (ties by seat order).
        let mut eliminated: Vec<usize> = alive
            .iter()
            .copied()
            .filter(|&s| self.stacks[s] == 0.0)
            .collect();
        eliminated.sort_by(|&a, &b| {
            let start = |s| stacks[alive.iter().position(|&x| x == s).unwrap()];
            start(a).total_cmp(&start(b))
        });
        let mut next_place = (alive.len()) as u8;
        for &seat in &eliminated {
            self.places[seat] = Some(next_place);
            next_place -= 1;
        }
        if next_place == 1 {
            let winner = (0..SEATS)
                .find(|&s| self.stacks[s] > 0.0)
                .expect("a winner");
            self.places[winner] = Some(1);
        }

        self.hands_played = number;
        self.clock += if alive.len() == SEATS {
            self.structure.hands_per_level_heads_up
        } else {
            self.structure.hands_per_level
        };
        let button = self.button;
        if let Some(next) = (1..=SEATS)
            .map(|k| (self.button + k) % SEATS)
            .find(|&s| self.stacks[s] > 0.0)
        {
            self.button = next;
        }
        Some(HandSummary {
            number,
            level,
            small_blind,
            big_blind,
            button,
            stacks: self.stacks,
            eliminated,
        })
    }

    /// Plays until one player has all the chips and returns the places.
    pub fn play_to_end(&mut self) -> [u8; SEATS] {
        while self.play_hand().is_some() {}
        self.places().expect("the game is over")
    }
}

/// Bridges a [`SeatStrategy`] to the arena's agent interface for one hand.
struct ArenaSeat {
    strategy: Arc<dyn SeatStrategy>,
    /// Table seat of each arena index.
    table: Vec<usize>,
    hand_number: u32,
    level: u32,
    rng: StdRng,
}

#[async_trait]
impl Agent for ArenaSeat {
    async fn act(&mut self, _id: u128, state: &GameState) -> AgentAction {
        let view = self.view(state);
        match self.strategy.decide(&view, &mut self.rng) {
            // The arena logs a fold with nothing to call as a failed action.
            Decision::Fold if view.to_call == 0.0 => AgentAction::Call,
            Decision::Fold => AgentAction::Fold,
            Decision::Call => AgentAction::Call,
            Decision::AllIn => AgentAction::AllIn,
        }
    }

    fn name(&self) -> &str {
        self.strategy.name()
    }
}

impl ArenaSeat {
    fn view(&self, state: &GameState) -> SeatView {
        let me = state.to_act_idx();
        let board: Vec<Card> = state.board.to_vec();
        // The arena adds the board to every hand.
        let hole: Vec<Card> = state.hands[me]
            .iter()
            .filter(|c| !board.contains(c))
            .collect();
        let n = state.num_players;
        let position = |i: usize| match (n, (i + n - state.dealer_idx) % n) {
            (2, 0) | (3, 1) => Position::SmallBlind,
            (3, 0) => Position::Button,
            _ => Position::BigBlind,
        };
        let players = (0..n)
            .map(|i| {
                let all_in = state.player_all_in.get(i);
                PlayerView {
                    seat: self.table[i],
                    position: position(i),
                    stack: state.stacks[i],
                    street_bet: state.current_round_player_bet(i),
                    folded: !all_in && !state.player_active.get(i),
                    all_in,
                }
            })
            .collect();
        SeatView {
            hand_number: self.hand_number,
            level: self.level,
            small_blind: state.small_blind,
            big_blind: state.big_blind,
            seat: self.table[me],
            position: position(me),
            hole_cards: [hole[0], hole[1]],
            board,
            street: match state.round {
                Round::Flop => Street::Flop,
                Round::Turn => Street::Turn,
                Round::River => Street::River,
                _ => Street::Preflop,
            },
            pot: state.total_pot,
            to_call: (state.current_round_bet() - state.current_round_player_bet(me))
                .min(state.stacks[me]),
            players,
        }
    }
}

/// Runs an arena future to completion on the current thread.
///
/// The arena is async only for agents that wait on I/O; ours decide
/// synchronously and no historian is attached, so the future never
/// suspends and one poll completes it.
fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("an arena hand suspended; seat strategies must not await"),
    }
}
