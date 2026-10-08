//! The live loop, driven offscreen: it captures the client, reads it and
//! plays the bot's turn until the game is over, a check fails or the
//! emergency stop is raised.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use nitro_bot::{
    ActiveWindow, BotError, CaptureError, ClickerBot, EmergencyStop, Focus, InjectError, Injector,
    LiveError, Pace, ScreenPoint, TableReader, TargetError, Window, WindowList, play_live,
};
use nitro_local_client::{APP_ID, ClientConfig, Frame, LAYOUT, LocalClient, Next, TITLE};
use nitro_simulator::{SeatStrategy, Structure, TrivialBot};
use rand::SeedableRng;
use rand::rngs::StdRng;

type Client = Rc<RefCell<LocalClient>>;

fn client() -> Client {
    Rc::new(RefCell::new(LocalClient::new(ClientConfig {
        structure: Structure::expresso_nitro(),
        seed: 5,
        bots: [TrivialBot::Random, TrivialBot::AlwaysAllIn]
            .map(|b| Arc::new(b) as Arc<dyn SeatStrategy>),
        bot_delay: Duration::ZERO,
        hand_over_delay: Duration::ZERO,
    })))
}

fn client_window() -> Window {
    Window {
        address: "0x1".to_owned(),
        title: TITLE.to_owned(),
        class: APP_ID.to_owned(),
        stable_id: Some("1".to_owned()),
    }
}

/// The client alone on the desktop, focused, at the top-left corner.
struct Desktop;

impl WindowList for Desktop {
    fn windows(&self) -> Result<Vec<Window>, TargetError> {
        Ok(vec![client_window()])
    }
}

impl Focus for Desktop {
    fn active_window(&self) -> Result<Option<ActiveWindow>, TargetError> {
        Ok(Some(ActiveWindow {
            window: client_window(),
            at: (0, 0),
            size: (LAYOUT.window.width, LAYOUT.window.height),
        }))
    }
}

/// Hands the clicks to the client, or drops them when `lost`.
struct Forward {
    client: Client,
    lost: bool,
    clicks: Rc<Cell<u32>>,
}

impl Injector for Forward {
    fn click(&mut self, at: ScreenPoint) -> Result<(), InjectError> {
        self.clicks.set(self.clicks.get() + 1);
        if !self.lost {
            let (x, y) = (at.x as u32, at.y as u32);
            self.client.borrow_mut().click(x, y);
        }
        Ok(())
    }
}

fn bot(client: &Client, lost: bool, stop: EmergencyStop) -> (ClickerBot, Rc<Cell<u32>>) {
    let clicks = Rc::new(Cell::new(0));
    let bot = ClickerBot::new(
        Arc::new(TrivialBot::Random),
        Box::new(Desktop),
        Box::new(Desktop),
        Box::new(Forward {
            client: Rc::clone(client),
            lost,
            clicks: Rc::clone(&clicks),
        }),
        stop,
    );
    (bot, clicks)
}

/// The client's window, moving on by one step at each capture, as its
/// timer would between two captures.
fn capture(client: &Client) -> impl FnMut() -> Result<Frame, CaptureError> + '_ {
    move || {
        let frame = client.borrow().frame();
        client.borrow_mut().advance();
        Ok(frame)
    }
}

fn pace() -> Pace {
    Pace {
        poll: Duration::ZERO,
        patience: Duration::from_millis(200),
    }
}

#[test]
fn plays_until_the_game_is_over() {
    let client = client();
    let (mut bot, clicks) = bot(&client, false, EmergencyStop::new());
    let reader = TableReader::new();
    let played = play_live(
        &mut bot,
        &mut capture(&client),
        &|frame| reader.read(frame),
        &mut StdRng::seed_from_u64(1),
        pace(),
    );
    assert_eq!(played, Ok(clicks.get()));
    assert!(clicks.get() > 0);
    assert_eq!(client.borrow_mut().advance(), Next::GameOver);
}

#[test]
fn the_emergency_stop_ends_the_loop_and_its_clicks() {
    let client = client();
    let stop = EmergencyStop::new();
    let (mut bot, clicks) = bot(&client, false, stop.clone());
    let reader = TableReader::new();
    let (mut captures, mut stopped_at) = (0, None);
    let mut inner = capture(&client);
    // Raised once the bot has clicked.
    let mut capture = || {
        captures += 1;
        if clicks.get() == 1 && stopped_at.is_none() {
            stop.trigger();
            stopped_at = Some(captures);
        }
        inner()
    };
    let played = play_live(
        &mut bot,
        &mut capture,
        &|frame| reader.read(frame),
        &mut StdRng::seed_from_u64(1),
        pace(),
    );
    assert_eq!(played, Err(LiveError::Bot(BotError::Stopped)));
    assert_eq!(clicks.get(), 1, "no click after the stop");
    assert_eq!(Some(captures), stopped_at, "nothing is captured after it");
}

#[test]
fn stops_when_a_click_is_not_taken() {
    let client = client();
    let (mut bot, clicks) = bot(&client, true, EmergencyStop::new());
    let reader = TableReader::new();
    let played = play_live(
        &mut bot,
        &mut capture(&client),
        &|frame| reader.read(frame),
        &mut StdRng::seed_from_u64(1),
        pace(),
    );
    assert_eq!(played, Err(LiveError::ClickNotTaken));
    assert_eq!(clicks.get(), 1, "the bot does not click again");
}

#[test]
fn stops_when_the_client_cannot_be_captured() {
    let client = client();
    let (mut bot, clicks) = bot(&client, false, EmergencyStop::new());
    let reader = TableReader::new();
    let played = play_live(
        &mut bot,
        &mut || Err(CaptureError::Target(TargetError::NotFound)),
        &|frame| reader.read(frame),
        &mut StdRng::seed_from_u64(1),
        pace(),
    );
    assert_eq!(
        played,
        Err(LiveError::Capture(CaptureError::Target(
            TargetError::NotFound
        )))
    );
    assert_eq!(clicks.get(), 0);
}

#[test]
fn stops_when_the_table_stays_unreadable() {
    let client = client();
    let (mut bot, clicks) = bot(&client, false, EmergencyStop::new());
    let played = play_live(
        &mut bot,
        &mut capture(&client),
        &|_| Err(nitro_bot::ReadError::Unreadable("anything".to_owned())),
        &mut StdRng::seed_from_u64(1),
        pace(),
    );
    assert!(
        matches!(played, Err(LiveError::Unreadable(_))),
        "{played:?}"
    );
    assert_eq!(clicks.get(), 0);
}
