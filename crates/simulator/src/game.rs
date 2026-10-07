//! One Expresso Nitro, played action by action on the `rs_poker` arena.
//!
//! The arena plays each hand; this module runs the tournament around it:
//! rising blinds, button moves, eliminations and places. rs_poker's own
//! `SingleTableTournament` cannot do it (see ADR 0002).
//!
//! Every arena seat suspends the hand at its decision (see ADR 0003), so the
//! game can stop there and wait for an answer: from the seat's strategy when
//! it has one, from outside (a human through a client) when it is external.
//! Whole hands and whole games are only loops over these steps.

use std::fmt;
use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll, Waker};

use async_trait::async_trait;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use rs_poker::arena::action::AgentAction;
use rs_poker::arena::{
    Agent, GameState, GameStateBuilder, HoldemSimulation, HoldemSimulationBuilder,
};
use rs_poker::core::Card;

use crate::seat::{Decision, PlayerView, Position, SeatStrategy, SeatView, Street};
use crate::structure::Structure;
use crate::table::{TableSeat, TableView};

const SEATS: usize = 3;

/// Who answers a seat's decisions.
#[derive(Clone)]
pub enum Seat {
    /// The strategy decides whenever the game is stepped.
    Strategy(Arc<dyn SeatStrategy>),
    /// The game waits for [`NitroGame::act`]: a human through a client.
    External,
}

impl From<Arc<dyn SeatStrategy>> for Seat {
    fn from(strategy: Arc<dyn SeatStrategy>) -> Self {
        Self::Strategy(strategy)
    }
}

/// What one [`NitroGame::step`] did.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// A new hand was dealt and its blinds posted.
    HandStarted { number: u32 },
    /// A seat's strategy took this decision (a fold with nothing to call is
    /// reported as the check it becomes).
    Acted { seat: usize, decision: Decision },
    /// An external seat must act; nothing moved. Answer with
    /// [`NitroGame::act`].
    AwaitingExternal { seat: usize },
    /// The hand is over: reported once, after its last action.
    HandOver(HandSummary),
    /// The game is over; stepping again changes nothing.
    GameOver,
}

/// Why [`NitroGame::act`] refused a decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActError {
    /// No seat is waiting for a decision.
    NoDecisionPending,
    /// The decision is not among [`SeatView::legal_decisions`].
    Illegal(Decision),
}

impl fmt::Display for ActError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDecisionPending => f.write_str("no seat is waiting for a decision"),
            Self::Illegal(decision) => write!(f, "{decision:?} is not legal here"),
        }
    }
}

impl std::error::Error for ActError {}

/// A three-seat Expresso Nitro in progress.
pub struct NitroGame {
    structure: Structure,
    seats: [Seat; SEATS],
    stacks: [f32; SEATS],
    button: usize,
    hands_played: u32,
    /// Elapsed time in ticks: a level lasts `hands_per_level ×
    /// hands_per_level_heads_up` ticks, so both hand lengths are whole.
    clock: u32,
    places: [Option<u8>; SEATS],
    rng: StdRng,
    /// The hand being played, or the last one played.
    hand: Option<Hand>,
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

/// The arena's future for one hand; it yields the finished simulation.
type HandRun = Pin<Box<dyn Future<Output = HoldemSimulation> + Send>>;

struct Hand {
    number: u32,
    level: u32,
    button: usize,
    /// Table seat of each arena index.
    table: Vec<usize>,
    /// Stacks before the blinds, by arena index.
    starting_stacks: Vec<f32>,
    /// The random source of each arena index's strategy.
    rngs: Vec<StdRng>,
    exchange: Arc<Mutex<Exchange>>,
    /// `None` once the hand is over.
    run: Option<HandRun>,
    /// The arena state at the pending decision, or at the end of the hand.
    state: GameState,
    pending: Option<SeatView>,
    /// Set when the hand is over, taken when it is reported.
    summary: Option<HandSummary>,
}

/// Where a suspended arena seat leaves its question and finds its answer.
#[derive(Default)]
struct Exchange {
    asked: Option<GameState>,
    answer: Option<AgentAction>,
}

impl NitroGame {
    /// A new game between strategies; `seed` decides the first button and
    /// every card dealt.
    pub fn new(structure: Structure, seats: [Arc<dyn SeatStrategy>; SEATS], seed: u64) -> Self {
        Self::with_seats(structure, seats.map(Seat::from), seed)
    }

    /// A new game where some seats may be external.
    pub fn with_seats(structure: Structure, seats: [Seat; SEATS], seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let button = rng.random_range(0..SEATS);
        Self {
            stacks: [structure.starting_stack as f32; SEATS],
            structure,
            seats,
            button,
            hands_played: 0,
            clock: 0,
            places: [None; SEATS],
            rng,
            hand: None,
        }
    }

    /// Chips of each seat after the last finished hand.
    pub fn stacks(&self) -> [f32; SEATS] {
        self.stacks
    }

    /// Seat of the button for the current hand, or the next one between
    /// hands.
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

    /// What the seat to act sees, while a decision is pending.
    pub fn pending_decision(&self) -> Option<&SeatView> {
        self.hand.as_ref()?.pending.as_ref()
    }

    /// The table as `observer` sees it: its own cards, the public state, and
    /// the cards shown down once a hand is over.
    pub fn table_view(&self, observer: usize) -> TableView {
        let mut seats = [TableSeat {
            stack: 0.0,
            position: None,
            street_bet: 0.0,
            in_hand: false,
            all_in: false,
            hole_cards: None,
            won: 0.0,
            place: None,
        }; SEATS];
        for (seat, view) in seats.iter_mut().enumerate() {
            view.stack = self.stacks[seat];
            view.place = self.places[seat];
        }
        let Some(hand) = &self.hand else {
            let level = self.level();
            let blinds = self.structure.levels[level as usize - 1];
            return TableView {
                observer,
                hand_number: 0,
                level,
                small_blind: blinds.small_blind as f32,
                big_blind: blinds.big_blind as f32,
                button: self.button,
                street: Street::Preflop,
                board: Vec::new(),
                pot: 0.0,
                to_act: None,
                legal: Vec::new(),
                to_call: 0.0,
                hand_over: false,
                seats,
            };
        };
        let state = &hand.state;
        let over = hand.run.is_none();
        let shown_down = over && (state.player_active | state.player_all_in).count() >= 2;
        for (i, &seat) in hand.table.iter().enumerate() {
            let all_in = state.player_all_in.get(i);
            let in_hand = all_in || state.player_active.get(i);
            let view = &mut seats[seat];
            view.stack = state.stacks[i];
            view.position = Some(position(state, i));
            view.all_in = all_in;
            view.in_hand = in_hand;
            if seat == observer || (shown_down && in_hand) {
                view.hole_cards = Some(hole_cards(state, i));
            }
            if over {
                view.won = state.player_winnings[i];
            } else {
                view.street_bet = state.current_round_player_bet(i);
            }
        }
        let observer_to_act = hand.pending.as_ref().filter(|v| v.seat == observer);
        TableView {
            observer,
            hand_number: hand.number,
            level: hand.level,
            small_blind: state.small_blind,
            big_blind: state.big_blind,
            button: hand.button,
            street: street(&state.board),
            board: state.board.to_vec(),
            pot: state.total_pot,
            to_act: hand.pending.as_ref().map(|v| v.seat),
            legal: observer_to_act.map_or_else(Vec::new, SeatView::legal_decisions),
            to_call: observer_to_act.map_or(0.0, |v| v.to_call),
            hand_over: over,
            seats,
        }
    }

    /// Moves the game on by one event: deals a hand, lets a strategy seat
    /// act, reports a finished hand, or reports that an external seat must
    /// act (without moving).
    pub fn step(&mut self) -> Step {
        let Some(hand) = self
            .hand
            .as_mut()
            .filter(|h| h.run.is_some() || h.summary.is_some())
        else {
            if self.is_over() {
                return Step::GameOver;
            }
            let number = self.hands_played + 1;
            self.start_hand();
            return Step::HandStarted { number };
        };
        if let Some(summary) = hand.summary.take() {
            return Step::HandOver(summary);
        }
        let view = hand
            .pending
            .clone()
            .expect("a running hand waits on a seat");
        let Seat::Strategy(strategy) = &self.seats[view.seat] else {
            return Step::AwaitingExternal { seat: view.seat };
        };
        let index = hand
            .table
            .iter()
            .position(|&s| s == view.seat)
            .expect("the seat to act is dealt in");
        let decision = match strategy.decide(&view, &mut hand.rngs[index]) {
            Decision::Fold if view.to_call == 0.0 => Decision::Call,
            decision => decision,
        };
        self.answer(decision);
        Step::Acted {
            seat: view.seat,
            decision,
        }
    }

    /// Answers the pending decision, whoever's seat it is.
    pub fn act(&mut self, decision: Decision) -> Result<(), ActError> {
        let view = self.pending_decision().ok_or(ActError::NoDecisionPending)?;
        if !view.legal_decisions().contains(&decision) {
            return Err(ActError::Illegal(decision));
        }
        self.answer(decision);
        Ok(())
    }

    /// Plays to the end of the current or next hand, or returns `None` if
    /// the game is over.
    ///
    /// # Panics
    ///
    /// If an external seat must act: answer it with [`NitroGame::act`].
    pub fn play_hand(&mut self) -> Option<HandSummary> {
        loop {
            match self.step() {
                Step::HandOver(summary) => return Some(summary),
                Step::GameOver => return None,
                Step::AwaitingExternal { seat } => {
                    panic!("seat {seat} is external: answer it with `act`")
                }
                Step::HandStarted { .. } | Step::Acted { .. } => {}
            }
        }
    }

    /// Plays until one player has all the chips and returns the places.
    pub fn play_to_end(&mut self) -> [u8; SEATS] {
        while self.play_hand().is_some() {}
        self.places().expect("the game is over")
    }

    fn start_hand(&mut self) {
        let number = self.hands_played + 1;
        let level = self.level();
        let blinds = self.structure.levels[level as usize - 1];
        let (small_blind, big_blind) = (blinds.small_blind as f32, blinds.big_blind as f32);
        let table: Vec<usize> = (0..SEATS).filter(|&s| self.stacks[s] > 0.0).collect();
        let starting_stacks: Vec<f32> = table.iter().map(|&s| self.stacks[s]).collect();
        // Heads-up must be a 2-seat table: with a busted third seat, the arena
        // makes the button post the big blind.
        let state = GameStateBuilder::new()
            .stacks(&starting_stacks)
            .blinds(big_blind, small_blind)
            .dealer_idx(
                table
                    .iter()
                    .position(|&s| s == self.button)
                    .expect("button seat is alive"),
            )
            .max_raises_per_round(None)
            .build()
            .expect("at least two seats have chips");
        let exchange = Arc::new(Mutex::new(Exchange::default()));
        let rngs: Vec<StdRng> = table
            .iter()
            .map(|_| StdRng::from_rng(&mut self.rng))
            .collect();
        let agents: Vec<Box<dyn Agent>> = table
            .iter()
            .map(|&seat| {
                Box::new(SuspendingSeat {
                    name: match &self.seats[seat] {
                        Seat::Strategy(strategy) => strategy.name().to_owned(),
                        Seat::External => "external".to_owned(),
                    },
                    exchange: Arc::clone(&exchange),
                }) as Box<dyn Agent>
            })
            .collect();
        let mut sim = HoldemSimulationBuilder::default()
            .game_state(state.clone())
            .agents(agents)
            .build_with_rng(StdRng::from_rng(&mut self.rng))
            .expect("the table has a game state and one agent per seat");
        self.hand = Some(Hand {
            number,
            level,
            button: self.button,
            table,
            starting_stacks,
            rngs,
            exchange,
            run: Some(Box::pin(async move {
                sim.run().await;
                sim
            })),
            state,
            pending: None,
            summary: None,
        });
        self.resume();
    }

    fn answer(&mut self, decision: Decision) {
        let hand = self.hand.as_mut().expect("a hand is running");
        hand.pending = None;
        lock(&hand.exchange).answer = Some(match decision {
            Decision::Fold => AgentAction::Fold,
            Decision::Call => AgentAction::Call,
            Decision::AllIn => AgentAction::AllIn,
        });
        self.resume();
    }

    /// Runs the hand until the next decision or its end.
    ///
    /// Seats decide only when stepped, so the arena never needs a runtime:
    /// each poll runs synchronously to the next suspended seat (see ADR
    /// 0003).
    fn resume(&mut self) {
        let hand = self.hand.as_mut().expect("a hand is running");
        let run = hand.run.as_mut().expect("the hand is not over");
        match run.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            Poll::Pending => {
                let state = lock(&hand.exchange)
                    .asked
                    .take()
                    .expect("the arena only suspends at a seat's decision");
                hand.pending = Some(seat_view(&state, &hand.table, hand.number, hand.level));
                hand.state = state;
            }
            Poll::Ready(sim) => {
                hand.run = None;
                hand.state = sim.game_state;
                self.finish_hand();
            }
        }
    }

    fn finish_hand(&mut self) {
        let hand = self.hand.as_mut().expect("a hand was played");
        let alive = &hand.table;
        for (i, &seat) in alive.iter().enumerate() {
            self.stacks[seat] = hand.state.stacks[i];
        }
        // Two players busted in the same hand: the smaller stack finishes
        // lower (ties by seat order).
        let mut eliminated: Vec<usize> = alive
            .iter()
            .copied()
            .filter(|&s| self.stacks[s] == 0.0)
            .collect();
        eliminated.sort_by(|&a, &b| {
            let start = |s| hand.starting_stacks[alive.iter().position(|&x| x == s).unwrap()];
            start(a).total_cmp(&start(b))
        });
        let mut next_place = alive.len() as u8;
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

        self.hands_played = hand.number;
        self.clock += if alive.len() == SEATS {
            self.structure.hands_per_level_heads_up
        } else {
            self.structure.hands_per_level
        };
        if let Some(next) = (1..=SEATS)
            .map(|k| (self.button + k) % SEATS)
            .find(|&s| self.stacks[s] > 0.0)
        {
            self.button = next;
        }
        hand.summary = Some(HandSummary {
            number: hand.number,
            level: hand.level,
            small_blind: hand.state.small_blind,
            big_blind: hand.state.big_blind,
            button: hand.button,
            stacks: self.stacks,
            eliminated,
        });
    }
}

fn lock(exchange: &Mutex<Exchange>) -> MutexGuard<'_, Exchange> {
    exchange
        .lock()
        .expect("no thread panics while holding the exchange")
}

/// An arena seat that leaves its question in the exchange and suspends the
/// hand until an answer is put there.
struct SuspendingSeat {
    name: String,
    exchange: Arc<Mutex<Exchange>>,
}

#[async_trait]
impl Agent for SuspendingSeat {
    async fn act(&mut self, _id: u128, state: &GameState) -> AgentAction {
        lock(&self.exchange).asked = Some(state.clone());
        poll_fn(|_| match lock(&self.exchange).answer.take() {
            Some(action) => Poll::Ready(action),
            None => Poll::Pending,
        })
        .await
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// What the seat to act in `state` sees.
fn seat_view(state: &GameState, table: &[usize], hand_number: u32, level: u32) -> SeatView {
    let me = state.to_act_idx();
    let board: Vec<Card> = state.board.to_vec();
    let players = (0..state.num_players)
        .map(|i| {
            let all_in = state.player_all_in.get(i);
            PlayerView {
                seat: table[i],
                position: position(state, i),
                stack: state.stacks[i],
                street_bet: state.current_round_player_bet(i),
                folded: !all_in && !state.player_active.get(i),
                all_in,
            }
        })
        .collect();
    SeatView {
        hand_number,
        level,
        small_blind: state.small_blind,
        big_blind: state.big_blind,
        seat: table[me],
        position: position(state, me),
        hole_cards: hole_cards(state, me),
        street: street(&board),
        board,
        pot: state.total_pot,
        to_call: (state.current_round_bet() - state.current_round_player_bet(me))
            .min(state.stacks[me]),
        players,
    }
}

fn position(state: &GameState, i: usize) -> Position {
    let n = state.num_players;
    match (n, (i + n - state.dealer_idx) % n) {
        (2, 0) | (3, 1) => Position::SmallBlind,
        (3, 0) => Position::Button,
        _ => Position::BigBlind,
    }
}

fn hole_cards(state: &GameState, i: usize) -> [Card; 2] {
    // The arena adds the board to every hand.
    let hole: Vec<Card> = state.hands[i]
        .iter()
        .filter(|c| !state.board.contains(c))
        .collect();
    [hole[0], hole[1]]
}

/// The last street dealt.
fn street(board: &[Card]) -> Street {
    match board.len() {
        0 => Street::Preflop,
        3 => Street::Flop,
        4 => Street::Turn,
        _ => Street::River,
    }
}
