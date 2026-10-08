//! The live loop: capture the local client, read it, play the bot's turn,
//! until the game is over or anything goes wrong.

use std::thread::sleep;
use std::time::{Duration, Instant};

use nitro_local_client::Frame;
use rand::Rng;

use crate::capture::CaptureError;
use crate::clicker::{BotError, ClickerBot, Turn};
use crate::read::ReadError;
use crate::state::TableState;

/// How fast the loop goes, and how long it waits for the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pace {
    /// Between two captures.
    pub poll: Duration,
    /// How long the client may take to show a click's effect, or to become
    /// readable again, before the bot gives up.
    pub patience: Duration,
}

/// Why the live loop stopped before the end of the game. Nothing more is
/// clicked.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LiveError {
    #[error(transparent)]
    Bot(#[from] BotError),
    #[error(transparent)]
    Capture(#[from] CaptureError),
    #[error("the table stays unreadable: {0}")]
    Unreadable(ReadError),
    #[error("the client does not show the last click's effect")]
    ClickNotTaken,
}

/// Plays the game on the local client to its end: `capture` takes its
/// window, `read` reads it and the bot plays each state once. Returns the
/// number of clicks.
pub fn play_live(
    bot: &mut ClickerBot,
    capture: &mut dyn FnMut() -> Result<Frame, CaptureError>,
    read: &dyn Fn(&Frame) -> Result<TableState, ReadError>,
    rng: &mut dyn Rng,
    pace: Pace,
) -> Result<u32, LiveError> {
    let mut clicks = 0;
    // Since when the client has shown nothing new to play.
    let mut waiting: Option<(Instant, Waiting)> = None;
    loop {
        let frame = capture()?;
        let turn = read(&frame).map(|state| bot.turn(&state, rng));
        let now = Instant::now();
        let waited = match turn {
            Ok(Ok(Turn::GameOver)) => return Ok(clicks),
            Ok(Ok(Turn::Clicked(_))) => {
                clicks += 1;
                None
            }
            Ok(Ok(Turn::NotToAct)) => None,
            Ok(Ok(Turn::AlreadyClicked)) => Some(Waiting::Click),
            Ok(Err(err)) => return Err(err.into()),
            Err(err) => Some(Waiting::Read(err)),
        };
        waiting = match (waiting, waited) {
            (_, None) => None,
            (Some((since, _)), Some(now_waiting)) if now - since > pace.patience => {
                return Err(match now_waiting {
                    Waiting::Click => LiveError::ClickNotTaken,
                    Waiting::Read(err) => LiveError::Unreadable(err),
                });
            }
            (Some((since, _)), Some(now_waiting)) => Some((since, now_waiting)),
            (None, Some(now_waiting)) => Some((now, now_waiting)),
        };
        if bot.is_stopped() {
            return Err(BotError::Stopped.into());
        }
        sleep(pace.poll);
    }
}

/// What the loop waits for.
enum Waiting {
    Click,
    Read(ReadError),
}
