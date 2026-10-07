//! The client's state: a simulator game where the hero's seat is external,
//! stepped at a human pace.

use std::sync::Arc;
use std::time::Duration;

use nitro_simulator::{Decision, NitroGame, Seat, SeatStrategy, Step, Structure, TableView};

use crate::layout::LAYOUT;
use crate::render::{Frame, chips, ordinal, render};

/// The human's seat, drawn at the bottom of the table.
pub const HERO: usize = 0;

/// A game to play.
#[derive(Clone)]
pub struct ClientConfig {
    pub structure: Structure,
    /// Same seed, same cards and same bot decisions.
    pub seed: u64,
    /// The strategies of seats 1 and 2.
    pub bots: [Arc<dyn SeatStrategy>; 2],
    /// Pause after each bot action, so a human can follow.
    pub bot_delay: Duration,
    /// Pause on a finished hand before the next one is dealt.
    pub hand_over_delay: Duration,
}

/// What the window should do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Redraw, wait this long, then [`LocalClient::advance`] again.
    After(Duration),
    /// Redraw and wait for the hero to click a button.
    WaitForClick,
    /// Redraw; the game is over.
    GameOver,
}

/// A game in progress, seen from the hero's seat.
pub struct LocalClient {
    game: NitroGame,
    names: [String; 3],
    status: String,
    bot_delay: Duration,
    hand_over_delay: Duration,
}

impl LocalClient {
    pub fn new(config: ClientConfig) -> Self {
        let [first, second] = config.bots;
        Self {
            game: NitroGame::with_seats(
                config.structure,
                [
                    Seat::External,
                    Seat::Strategy(first),
                    Seat::Strategy(second),
                ],
                config.seed,
            ),
            names: ["YOU", "BOT 1", "BOT 2"].map(str::to_owned),
            status: String::new(),
            bot_delay: config.bot_delay,
            hand_over_delay: config.hand_over_delay,
        }
    }

    /// Moves the game on by one event, unless the hero must act.
    pub fn advance(&mut self) -> Next {
        let asked = self.game.pending_decision().map(|v| v.to_call);
        match self.game.step() {
            Step::HandStarted { number } => {
                self.status = format!("HAND {number}");
                self.after_action()
            }
            Step::Acted { seat, decision } => {
                self.status = self.describe(seat, decision, asked.unwrap_or(0.0));
                self.after_action()
            }
            Step::AwaitingExternal { .. } => Next::WaitForClick,
            Step::HandOver(_) => Next::After(Duration::ZERO),
            Step::GameOver => {
                if let Some(places) = self.game.places() {
                    self.status = format!("GAME OVER: YOU FINISH {}", ordinal(places[HERO]));
                }
                Next::GameOver
            }
        }
    }

    /// Plays the button under (`x`, `y`) if it is one the hero may press
    /// now; otherwise does nothing and returns `None`.
    pub fn click(&mut self, x: u32, y: u32) -> Option<Next> {
        let view = self.view();
        let decision = view
            .legal
            .iter()
            .copied()
            .find(|&d| LAYOUT.button(d).contains(x, y))?;
        self.game
            .act(decision)
            .expect("the simulator accepts its own legal decisions");
        self.status = self.describe(HERO, decision, view.to_call);
        Some(self.after_action())
    }

    /// The table as the hero sees it.
    pub fn view(&self) -> TableView {
        self.game.table_view(HERO)
    }

    /// The seats' names, as drawn.
    pub fn names(&self) -> &[String; 3] {
        &self.names
    }

    /// The last thing that happened, as drawn.
    pub fn status(&self) -> &str {
        &self.status
    }

    /// The window's contents.
    pub fn frame(&self) -> Frame {
        render(&self.view(), &self.names, &self.status)
    }

    fn after_action(&mut self) -> Next {
        let view = self.view();
        if !view.hand_over {
            return Next::After(self.bot_delay);
        }
        let winners: Vec<String> = view
            .seats
            .iter()
            .enumerate()
            .filter(|(_, s)| s.won > 0.0)
            .map(|(seat, s)| {
                let verb = if seat == HERO { "WIN" } else { "WINS" };
                format!("{} {verb} {}", self.names[seat], chips(s.won))
            })
            .collect();
        self.status = winners.join("  ");
        Next::After(self.hand_over_delay)
    }

    fn describe(&self, seat: usize, decision: Decision, to_call: f32) -> String {
        // "YOU CALL", "BOT 1 CALLS".
        let s = if seat == HERO { "" } else { "S" };
        let action = match decision {
            Decision::Fold => format!("FOLD{s}"),
            Decision::Call if to_call == 0.0 => format!("CHECK{s}"),
            Decision::Call => format!("CALL{s} {}", chips(to_call)),
            Decision::AllIn if seat == HERO => "GO ALL-IN".to_owned(),
            Decision::AllIn => "GOES ALL-IN".to_owned(),
        };
        format!("{} {action}", self.names[seat])
    }
}
