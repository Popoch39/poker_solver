//! The bot on whole tournaments, offscreen: the local client renders each
//! frame, the bot reads it, decides with the hero's strategy and clicks
//! through an injector that forwards the click to the client's own click
//! handler. Nothing reaches the screen or the desktop's input.
//!
//! The same games are played by the hero directly in the simulator
//! ([`compare_with`]), and each of the bot's decisions is checked against
//! the hero's decision from the simulator's own view of the table.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nitro_local_client::{APP_ID, ClientConfig, Frame, HERO, LAYOUT, LocalClient, Next, TITLE};
use nitro_simulator::{Comparison, Decision, SimulationConfig, compare_with};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use crate::clicker::{ClickerBot, Turn, button};
use crate::inject::{InjectError, Injector, ScreenPoint};
use crate::read::ReadError;
use crate::state::TableState;
use crate::stop::EmergencyStop;
use crate::window::{ActiveWindow, Focus, TargetError, Window, WindowList};

/// Where the offscreen client pretends to be on the screen.
const AT: (i32, i32) = (240, 120);

/// The bot's tournaments against the hero's, on the same games.
#[derive(Debug, Clone, PartialEq)]
pub struct OffscreenReport {
    /// The hero simulated directly (baseline) and the bot playing it on
    /// the client (challenger), at the hero's seat.
    pub comparison: Comparison,
    /// The hero's decisions the bot was asked for.
    pub decisions: u64,
    /// Decisions where the bot clicked what the hero decides from the
    /// simulator's own view, with the same random draw.
    pub concordant: u64,
    /// Decisions the bot could not take: its frame was unreadable, or its
    /// click did not land. The hero's decision is then played for it.
    pub unplayed: u64,
    /// The first decision, by game, where the bot clicked something else.
    pub first_discordance: Option<Discordance>,
}

impl OffscreenReport {
    /// The share of decisions where the bot clicked the hero's decision.
    pub fn concordance(&self) -> f64 {
        if self.decisions == 0 {
            1.0
        } else {
            self.concordant as f64 / self.decisions as f64
        }
    }
}

/// A decision of the bot that is not the hero's.
#[derive(Debug, Clone, PartialEq)]
pub struct Discordance {
    /// Game index in the simulation.
    pub game: u64,
    pub hand_number: u32,
    /// The hero's decision, as a button.
    pub expected: Decision,
    /// What the bot clicked; `None` when it clicked nothing.
    pub clicked: Option<Decision>,
}

/// Plays the games of `config` with the bot at seat 0, playing the
/// strategy `config` seats there, against the strategies of seats 1 and 2
/// on the local client, every frame read with `read`; and the same games
/// directly.
pub fn play_offscreen(
    config: &SimulationConfig,
    read: impl Fn(&Frame) -> Result<TableState, ReadError> + Sync,
) -> OffscreenReport {
    let decisions = AtomicU64::new(0);
    let concordant = AtomicU64::new(0);
    let unplayed = AtomicU64::new(0);
    let first_discordance = Mutex::new(None::<Discordance>);
    let comparison = compare_with(config, HERO, |game, seed| {
        let mut tally = Tally::default();
        let places = play_game(config, &read, game, seed, &mut tally);
        decisions.fetch_add(tally.decisions, Ordering::Relaxed);
        concordant.fetch_add(tally.concordant, Ordering::Relaxed);
        unplayed.fetch_add(tally.unplayed, Ordering::Relaxed);
        if let Some(discordance) = tally.first_discordance {
            let mut first = first_discordance.lock().expect("no tally panics");
            if first.as_ref().is_none_or(|f| f.game > discordance.game) {
                *first = Some(discordance);
            }
        }
        places
    });
    OffscreenReport {
        comparison,
        decisions: decisions.into_inner(),
        concordant: concordant.into_inner(),
        unplayed: unplayed.into_inner(),
        first_discordance: first_discordance.into_inner().expect("no tally panics"),
    }
}

#[derive(Default)]
struct Tally {
    decisions: u64,
    concordant: u64,
    unplayed: u64,
    first_discordance: Option<Discordance>,
}

/// Plays game `game`, dealt from `seed`, to its end and returns its places.
fn play_game(
    config: &SimulationConfig,
    read: &impl Fn(&Frame) -> Result<TableState, ReadError>,
    game: u64,
    seed: u64,
    tally: &mut Tally,
) -> [u8; 3] {
    let [hero, first, second] = config.seats.clone();
    let client = Rc::new(RefCell::new(LocalClient::new(ClientConfig {
        structure: config.structure.clone(),
        seed,
        bots: [first, second],
        bot_delay: Duration::ZERO,
        hand_over_delay: Duration::ZERO,
    })));
    let mut bot = ClickerBot::new(
        Arc::clone(&hero),
        Box::new(Desktop),
        Box::new(Desktop),
        Box::new(Forward(Rc::clone(&client))),
        EmergencyStop::new(),
    );
    let mut rng = StdRng::seed_from_u64(seed);
    loop {
        let next = client.borrow_mut().advance();
        match next {
            Next::After(_) => {}
            Next::GameOver => break,
            Next::WaitForClick => {
                let (frame, truth) = {
                    let client = client.borrow();
                    let truth = client.hero_decision().expect("the hero is asked").clone();
                    (client.frame(), truth)
                };
                // The hero and the bot get the same random draws.
                let draws: u64 = rng.random();
                let expected = button(
                    hero.decide(&truth, &mut StdRng::seed_from_u64(draws)),
                    &truth.legal_decisions(),
                );
                let clicked = match read(&frame) {
                    Ok(state) => match bot.turn(&state, &mut StdRng::seed_from_u64(draws)) {
                        Ok(Turn::Clicked(decision)) => Some(decision),
                        _ => None,
                    },
                    Err(_) => None,
                };
                tally.decisions += 1;
                if clicked == Some(expected) {
                    tally.concordant += 1;
                } else if tally.first_discordance.is_none() {
                    tally.first_discordance = Some(Discordance {
                        game,
                        hand_number: truth.hand_number,
                        expected,
                        clicked,
                    });
                }
                if clicked.is_none() {
                    tally.unplayed += 1;
                    let (x, y) = LAYOUT.button(expected).center();
                    client.borrow_mut().click(x, y).expect("a legal button");
                }
            }
        }
    }
    let view = client.borrow().view();
    view.seats.map(|s| s.place.expect("the game is over"))
}

/// A desktop with the local client alone, focused, at [`AT`].
struct Desktop;

fn client_window() -> Window {
    Window {
        address: "0xoffscreen".to_owned(),
        title: TITLE.to_owned(),
        class: APP_ID.to_owned(),
        stable_id: Some("offscreen".to_owned()),
    }
}

impl WindowList for Desktop {
    fn windows(&self) -> Result<Vec<Window>, TargetError> {
        Ok(vec![client_window()])
    }
}

impl Focus for Desktop {
    fn active_window(&self) -> Result<Option<ActiveWindow>, TargetError> {
        Ok(Some(ActiveWindow {
            window: client_window(),
            at: AT,
            size: (LAYOUT.window.width, LAYOUT.window.height),
        }))
    }
}

/// Hands each click to the client, at the window point under it.
struct Forward(Rc<RefCell<LocalClient>>);

impl Injector for Forward {
    fn click(&mut self, at: ScreenPoint) -> Result<(), InjectError> {
        let (x, y) = (at.x - AT.0, at.y - AT.1);
        let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
            return Err(InjectError(format!("({x}, {y}) is off the client")));
        };
        match self.0.borrow_mut().click(x, y) {
            Some(_) => Ok(()),
            None => Err(InjectError(format!("({x}, {y}) is on no legal button"))),
        }
    }
}
