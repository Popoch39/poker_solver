//! The bot's turn: the state read → the hero's decision → a click on the
//! local client's button.

use std::sync::Arc;

use nitro_local_client::{HERO, LAYOUT};
use nitro_simulator::{Decision, SeatStrategy};
use rand::Rng;

use crate::inject::{InjectError, Injector, ScreenPoint};
use crate::state::TableState;
use crate::stop::EmergencyStop;
use crate::window::{Focus, TargetError, Window, WindowList, find_client_window};

/// What a turn did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    /// Clicked the button of this decision.
    Clicked(Decision),
    /// The hero is not to act.
    NotToAct,
    /// The state is the one last clicked: the client has not drawn the
    /// click's effect yet.
    AlreadyClicked,
    /// The hero is out, or has won.
    GameOver,
}

/// Why the bot stopped: nothing was clicked.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BotError {
    #[error("emergency stop")]
    Stopped,
    #[error(transparent)]
    Target(#[from] TargetError),
    #[error("the local client does not have the focus (focused: {active:?})")]
    NotFocused { active: Option<Window> },
    #[error(
        "the local client is {width}×{height}, not {}×{}: it must float unscaled",
        LAYOUT.window.width,
        LAYOUT.window.height
    )]
    WrongSize { width: u32, height: u32 },
    #[error(transparent)]
    Inject(#[from] InjectError),
}

/// Plays the hero's seat of the local client with a seat strategy, by
/// clicking its buttons.
///
/// Before every click, the local client must be the one window titled like
/// it ([`find_client_window`]), must have the focus, at its own size, and
/// the emergency stop must be down; otherwise nothing is clicked and the
/// turn fails.
pub struct ClickerBot {
    hero: Arc<dyn SeatStrategy>,
    windows: Box<dyn WindowList>,
    focus: Box<dyn Focus>,
    injector: Box<dyn Injector>,
    stop: EmergencyStop,
    last_clicked: Option<TableState>,
}

impl ClickerBot {
    pub fn new(
        hero: Arc<dyn SeatStrategy>,
        windows: Box<dyn WindowList>,
        focus: Box<dyn Focus>,
        injector: Box<dyn Injector>,
        stop: EmergencyStop,
    ) -> Self {
        Self {
            hero,
            windows,
            focus,
            injector,
            stop,
            last_clicked: None,
        }
    }

    /// Plays `state`, read from the client: the hero's decision, drawn from
    /// `rng`, clicked on its button.
    pub fn turn(&mut self, state: &TableState, rng: &mut dyn Rng) -> Result<Turn, BotError> {
        if self.stop.is_triggered() {
            return Err(BotError::Stopped);
        }
        if game_over(state) {
            return Ok(Turn::GameOver);
        }
        let Some(view) = state.seat_view() else {
            return Ok(Turn::NotToAct);
        };
        if self.last_clicked.as_ref() == Some(state) {
            return Ok(Turn::AlreadyClicked);
        }
        let button = button(self.hero.decide(&view, rng), &state.legal);
        let at = self.aim(button)?;
        // The focus check takes a while: the stop may have come meanwhile.
        if self.stop.is_triggered() {
            return Err(BotError::Stopped);
        }
        self.injector.click(at)?;
        self.last_clicked = Some(state.clone());
        Ok(Turn::Clicked(button))
    }

    /// Where `button` is on the screen, provided the local client has the
    /// focus.
    fn aim(&self, button: Decision) -> Result<ScreenPoint, BotError> {
        let client = find_client_window(self.windows.as_ref())?;
        let active = match self.focus.active_window()? {
            Some(active) if same_window(&active.window, client.window()) => active,
            other => {
                return Err(BotError::NotFocused {
                    active: other.map(|a| a.window),
                });
            }
        };
        let (width, height) = active.size;
        if (width, height) != (LAYOUT.window.width, LAYOUT.window.height) {
            return Err(BotError::WrongSize { width, height });
        }
        let (x, y) = LAYOUT.button(button).center();
        Ok(ScreenPoint {
            x: active.at.0 + x as i32,
            y: active.at.1 + y as i32,
        })
    }
}

/// The address names the window; title and class must still be the
/// client's, in case the address was reused.
fn same_window(a: &Window, b: &Window) -> bool {
    a.address == b.address && a.title == b.title && a.class == b.class
}

/// The button that plays `decision` among the `legal` ones, as the
/// simulator plays a strategy's decision: a fold with nothing to call is a
/// check, and an all-in that is not a button only calls.
pub(crate) fn button(decision: Decision, legal: &[Decision]) -> Decision {
    if legal.contains(&decision) {
        decision
    } else {
        Decision::Call
    }
}

/// The hero is out, or the two others are.
fn game_over(state: &TableState) -> bool {
    state.seats[HERO].place.is_some()
        || state.seats.iter().filter(|s| s.place.is_some()).count() == 2
}
