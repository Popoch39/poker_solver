//! The bot's turn: from the state read to a click on the local client's
//! button, and only there.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use nitro_bot::{
    ActiveWindow, BotError, ClickerBot, EmergencyStop, Focus, InjectError, Injector, ScreenPoint,
    TableState, TargetError, Turn, Window, WindowList,
};
use nitro_local_client::{APP_ID, ClientConfig, HERO, LAYOUT, LocalClient, Next, TITLE};
use nitro_simulator::{Decision, SeatStrategy, Structure, TrivialBot};
use rand::SeedableRng;
use rand::rngs::StdRng;

fn window(title: &str, class: &str, id: &str) -> Window {
    Window {
        address: format!("0x{id}"),
        title: title.to_owned(),
        class: class.to_owned(),
        stable_id: Some(id.to_owned()),
    }
}

fn client_window() -> Window {
    window(TITLE, APP_ID, "18000008")
}

fn terminal() -> Window {
    window("~/poker_solver", "Alacritty", "18000001")
}

/// The local client, unscaled, at (1200, 300) on the screen.
fn client_on_screen() -> ActiveWindow {
    ActiveWindow {
        window: client_window(),
        at: (1200, 300),
        size: (LAYOUT.window.width, LAYOUT.window.height),
    }
}

struct Desktop(Vec<Window>);

impl WindowList for Desktop {
    fn windows(&self) -> Result<Vec<Window>, TargetError> {
        Ok(self.0.clone())
    }
}

/// The focused window; `on_check` runs at every check.
struct FakeFocus {
    active: Option<ActiveWindow>,
    on_check: Box<dyn Fn()>,
}

impl Focus for FakeFocus {
    fn active_window(&self) -> Result<Option<ActiveWindow>, TargetError> {
        (self.on_check)();
        Ok(self.active.clone())
    }
}

#[derive(Clone, Default)]
struct Clicks(Rc<RefCell<Vec<ScreenPoint>>>);

impl Injector for Clicks {
    fn click(&mut self, at: ScreenPoint) -> Result<(), InjectError> {
        self.0.borrow_mut().push(at);
        Ok(())
    }
}

impl Clicks {
    fn taken(&self) -> Vec<ScreenPoint> {
        self.0.borrow().clone()
    }
}

struct Setup {
    windows: Vec<Window>,
    active: Option<ActiveWindow>,
    on_check: Box<dyn Fn()>,
    stop: EmergencyStop,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            windows: vec![terminal(), client_window()],
            active: Some(client_on_screen()),
            on_check: Box::new(|| {}),
            stop: EmergencyStop::new(),
        }
    }
}

fn bot(hero: TrivialBot, setup: Setup) -> (ClickerBot, Clicks) {
    let clicks = Clicks::default();
    let bot = ClickerBot::new(
        Arc::new(hero),
        Box::new(Desktop(setup.windows)),
        Box::new(FakeFocus {
            active: setup.active,
            on_check: setup.on_check,
        }),
        Box::new(clicks.clone()),
        setup.stop,
    );
    (bot, clicks)
}

/// The table at the hero's first decision for which `wanted` holds.
fn state_where(wanted: impl Fn(&TableState) -> bool) -> TableState {
    for seed in 0.. {
        let mut client = LocalClient::new(ClientConfig {
            structure: Structure::expresso_nitro(),
            seed,
            bots: [TrivialBot::AlwaysFold; 2].map(|b| Arc::new(b) as Arc<dyn SeatStrategy>),
            bot_delay: Duration::ZERO,
            hand_over_delay: Duration::ZERO,
        });
        for _ in 0..200 {
            match client.advance() {
                Next::After(_) => {}
                Next::GameOver => break,
                Next::WaitForClick => {
                    let state = TableState::drawn(&client.view());
                    if wanted(&state) {
                        return state;
                    }
                    let (x, y) = LAYOUT.call.center();
                    client.click(x, y).unwrap();
                }
            }
        }
    }
    unreachable!()
}

fn hero_may_go_all_in() -> TableState {
    state_where(|s| s.legal.contains(&Decision::AllIn))
}

fn on_screen(decision: Decision) -> ScreenPoint {
    let (x, y) = LAYOUT.button(decision).center();
    ScreenPoint {
        x: 1200 + x as i32,
        y: 300 + y as i32,
    }
}

fn rng() -> StdRng {
    StdRng::seed_from_u64(1)
}

#[test]
fn clicks_the_hero_s_decision_on_the_client_s_button_where_it_is_on_screen() {
    let (mut bot, clicks) = bot(TrivialBot::AlwaysAllIn, Setup::default());
    let turn = bot.turn(&hero_may_go_all_in(), &mut rng());
    assert_eq!(turn, Ok(Turn::Clicked(Decision::AllIn)));
    assert_eq!(clicks.taken(), [on_screen(Decision::AllIn)]);
}

#[test]
fn a_fold_with_nothing_to_call_clicks_check() {
    let state = state_where(|s| s.to_call == 0.0);
    assert!(!state.legal.contains(&Decision::Fold));
    let (mut bot, clicks) = bot(TrivialBot::AlwaysFold, Setup::default());
    assert_eq!(
        bot.turn(&state, &mut rng()),
        Ok(Turn::Clicked(Decision::Call))
    );
    assert_eq!(clicks.taken(), [on_screen(Decision::Call)]);
}

#[test]
fn clicks_nothing_while_the_hero_is_not_to_act() {
    let mut state = hero_may_go_all_in();
    state.to_act = Some(1);
    state.legal.clear();
    let (mut bot, clicks) = bot(TrivialBot::AlwaysAllIn, Setup::default());
    assert_eq!(bot.turn(&state, &mut rng()), Ok(Turn::NotToAct));
    assert!(clicks.taken().is_empty());
}

#[test]
fn clicks_a_state_once_while_the_client_has_not_moved_on() {
    let state = hero_may_go_all_in();
    let (mut bot, clicks) = bot(TrivialBot::AlwaysAllIn, Setup::default());
    bot.turn(&state, &mut rng()).unwrap();
    assert_eq!(bot.turn(&state, &mut rng()), Ok(Turn::AlreadyClicked));
    assert_eq!(clicks.taken().len(), 1);
}

#[test]
fn the_game_is_over_for_the_bot_once_the_hero_is_out() {
    let mut state = hero_may_go_all_in();
    state.to_act = None;
    state.legal.clear();
    state.seats[HERO].place = Some(3);
    let (mut bot, clicks) = bot(TrivialBot::AlwaysAllIn, Setup::default());
    assert_eq!(bot.turn(&state, &mut rng()), Ok(Turn::GameOver));
    assert!(clicks.taken().is_empty());
}

#[test]
fn stops_without_clicking_when_another_window_is_focused() {
    let (mut bot, clicks) = bot(
        TrivialBot::AlwaysAllIn,
        Setup {
            active: Some(ActiveWindow {
                window: terminal(),
                ..client_on_screen()
            }),
            ..Setup::default()
        },
    );
    let turn = bot.turn(&hero_may_go_all_in(), &mut rng());
    assert_eq!(
        turn,
        Err(BotError::NotFocused {
            active: Some(terminal())
        })
    );
    assert!(clicks.taken().is_empty());
}

#[test]
fn stops_without_clicking_when_no_window_is_focused() {
    let (mut bot, clicks) = bot(
        TrivialBot::AlwaysAllIn,
        Setup {
            active: None,
            ..Setup::default()
        },
    );
    let turn = bot.turn(&hero_may_go_all_in(), &mut rng());
    assert_eq!(turn, Err(BotError::NotFocused { active: None }));
    assert!(clicks.taken().is_empty());
}

#[test]
fn stops_without_clicking_when_a_look_alike_of_the_client_is_open() {
    let look_alike = window(TITLE, "firefox", "18000004");
    let (mut bot, clicks) = bot(
        TrivialBot::AlwaysAllIn,
        Setup {
            windows: vec![client_window(), look_alike],
            ..Setup::default()
        },
    );
    let turn = bot.turn(&hero_may_go_all_in(), &mut rng());
    assert_eq!(
        turn,
        Err(BotError::Target(TargetError::WrongApplication {
            class: "firefox".to_owned()
        }))
    );
    assert!(clicks.taken().is_empty());
}

#[test]
fn stops_without_clicking_when_the_focused_client_is_not_at_its_own_size() {
    // Tiled or scaled: the buttons are no longer where the layout says.
    let (mut bot, clicks) = bot(
        TrivialBot::AlwaysAllIn,
        Setup {
            active: Some(ActiveWindow {
                size: (1280, 720),
                ..client_on_screen()
            }),
            ..Setup::default()
        },
    );
    let turn = bot.turn(&hero_may_go_all_in(), &mut rng());
    assert_eq!(
        turn,
        Err(BotError::WrongSize {
            width: 1280,
            height: 720
        })
    );
    assert!(clicks.taken().is_empty());
}

#[test]
fn an_emergency_stop_halts_every_click_that_follows() {
    let stop = EmergencyStop::new();
    let (mut bot, clicks) = bot(
        TrivialBot::AlwaysAllIn,
        Setup {
            stop: stop.clone(),
            ..Setup::default()
        },
    );
    let state = hero_may_go_all_in();
    stop.trigger();
    assert_eq!(bot.turn(&state, &mut rng()), Err(BotError::Stopped));
    assert_eq!(bot.turn(&state, &mut rng()), Err(BotError::Stopped));
    assert!(clicks.taken().is_empty());
}

#[test]
fn an_emergency_stop_during_the_focus_check_still_halts_the_click() {
    let stop = EmergencyStop::new();
    let raised = stop.clone();
    let (mut bot, clicks) = bot(
        TrivialBot::AlwaysAllIn,
        Setup {
            stop,
            on_check: Box::new(move || raised.trigger()),
            ..Setup::default()
        },
    );
    let turn = bot.turn(&hero_may_go_all_in(), &mut rng());
    assert_eq!(turn, Err(BotError::Stopped));
    assert!(clicks.taken().is_empty());
}
