//! Reference captures: frames of the local client, recorded as PNG under
//! `tests/captures/`, and the state each must read as.
//!
//! They are drawn offscreen by the client's renderer, which draws the same
//! pixels as its window. `NITRO_BLESS_CAPTURES=1 cargo test -p nitro-bot
//! --test reference` records them again after a change to the client.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nitro_bot::{Cards, SeatState, TableReader, TableState};
use nitro_local_client::{ClientConfig, Frame, HERO, LAYOUT, LocalClient, Next};
use nitro_simulator::{
    Card, Decision, Position, SeatStrategy, Structure, Suit, TableView, TrivialBot, Value,
};

/// Plays a game, the hero always checking or calling, until `stop` holds
/// for the table.
fn play_until(seed: u64, bots: [TrivialBot; 2], stop: impl Fn(&TableView) -> bool) -> Frame {
    let mut client = LocalClient::new(ClientConfig {
        structure: Structure::expresso_nitro(),
        seed,
        bots: bots.map(|b| Arc::new(b) as Arc<dyn SeatStrategy>),
        bot_delay: Duration::ZERO,
        hand_over_delay: Duration::ZERO,
    });
    for _ in 0..10_000 {
        if stop(&client.view()) {
            return client.frame();
        }
        match client.advance() {
            Next::After(_) => {}
            Next::WaitForClick => {
                if stop(&client.view()) {
                    return client.frame();
                }
                let (x, y) = LAYOUT.call.center();
                client.click(x, y).expect("the call button");
            }
            Next::GameOver => break,
        }
    }
    panic!("seed {seed}: the table never got there");
}

/// The recorded captures: name and how to get there.
fn scenarios() -> Vec<(&'static str, Frame)> {
    use TrivialBot::{AlwaysAllIn, AlwaysFold, Random};
    vec![
        ("before-first-hand", play_until(5, [Random; 2], |_| true)),
        (
            "bot-to-act-at-a-later-level",
            play_until(2, [AlwaysFold; 2], |v| {
                v.hand_number >= 14 && v.to_act.is_some_and(|seat| seat != HERO)
            }),
        ),
        (
            "facing-two-all-ins",
            play_until(9, [AlwaysAllIn; 2], |v| v.to_act == Some(HERO)),
        ),
        (
            "shown-down",
            play_until(9, [AlwaysAllIn; 2], |v| v.hand_over),
        ),
        (
            "hero-on-the-flop",
            play_until(1, [Random; 2], |v| {
                v.board.len() == 3 && v.to_act == Some(HERO)
            }),
        ),
    ]
}

fn path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/captures")
        .join(format!("{name}.png"))
}

fn recorded(name: &str) -> Frame {
    let bytes = std::fs::read(path(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    Frame::decode_png(&bytes).unwrap()
}

#[test]
fn the_captures_are_what_the_client_draws() {
    let bless = std::env::var_os("NITRO_BLESS_CAPTURES").is_some();
    for (name, frame) in scenarios() {
        if bless {
            std::fs::create_dir_all(path(name).parent().unwrap()).unwrap();
            frame.save_png(path(name)).unwrap();
        }
        assert!(
            recorded(name) == frame,
            "{name} is not what the client draws"
        );
    }
}

/// `"Qc"` → the queen of clubs.
fn card(text: &str) -> Card {
    let mut chars = text.chars();
    let value = Value::from_char(chars.next().unwrap()).unwrap();
    let suit = Suit::from_char(chars.next().unwrap()).unwrap();
    Card::new(value, suit)
}

fn shown(first: &str, second: &str) -> Cards {
    Cards::Shown([card(first), card(second)])
}

fn seat(position: Option<Position>, stack: f32, cards: Cards) -> SeatState {
    SeatState {
        position,
        stack,
        all_in: false,
        place: None,
        street_bet: 0.0,
        won: 0.0,
        cards,
    }
}

fn hand_one() -> TableState {
    TableState {
        hand_number: 1,
        level: 1,
        small_blind: 10.0,
        big_blind: 20.0,
        button: 0,
        board: Vec::new(),
        pot: 0.0,
        to_act: None,
        legal: Vec::new(),
        to_call: 0.0,
        seats: [seat(None, 300.0, Cards::None); 3],
    }
}

/// What each capture shows, read off the PNG by eye.
fn expected(name: &str) -> TableState {
    use Position::{BigBlind, Button, SmallBlind};
    match name {
        "before-first-hand" => TableState {
            hand_number: 0,
            ..hand_one()
        },
        "bot-to-act-at-a-later-level" => TableState {
            hand_number: 14,
            level: 4,
            small_blind: 30.0,
            big_blind: 60.0,
            button: 1,
            pot: 90.0,
            to_act: Some(1),
            seats: [
                SeatState {
                    street_bet: 60.0,
                    ..seat(Some(BigBlind), 465.0, shown("6s", "2d"))
                },
                seat(Some(Button), 205.0, Cards::Hidden),
                SeatState {
                    street_bet: 30.0,
                    ..seat(Some(SmallBlind), 140.0, Cards::Hidden)
                },
            ],
            ..hand_one()
        },
        "facing-two-all-ins" => TableState {
            button: 1,
            pot: 620.0,
            to_act: Some(HERO),
            legal: vec![Decision::Fold, Decision::Call],
            to_call: 280.0,
            seats: [
                SeatState {
                    street_bet: 20.0,
                    ..seat(Some(BigBlind), 280.0, shown("2s", "6c"))
                },
                SeatState {
                    all_in: true,
                    street_bet: 300.0,
                    ..seat(Some(Button), 0.0, Cards::Hidden)
                },
                SeatState {
                    all_in: true,
                    street_bet: 300.0,
                    ..seat(Some(SmallBlind), 0.0, Cards::Hidden)
                },
            ],
            ..hand_one()
        },
        "shown-down" => TableState {
            button: 1,
            board: ["4c", "Qc", "Kc", "Jd", "7d"].map(card).to_vec(),
            pot: 900.0,
            seats: [
                SeatState {
                    place: Some(3),
                    ..seat(Some(BigBlind), 0.0, shown("2s", "6c"))
                },
                SeatState {
                    place: Some(2),
                    ..seat(Some(Button), 0.0, shown("6s", "9s"))
                },
                SeatState {
                    won: 900.0,
                    ..seat(Some(SmallBlind), 900.0, shown("7h", "Jh"))
                },
            ],
            ..hand_one()
        },
        "hero-on-the-flop" => TableState {
            button: 2,
            board: ["4s", "5d", "7c"].map(card).to_vec(),
            pot: 40.0,
            to_act: Some(HERO),
            legal: vec![Decision::Call, Decision::AllIn],
            to_call: 0.0,
            seats: [
                seat(Some(SmallBlind), 280.0, shown("2c", "Ad")),
                seat(Some(BigBlind), 280.0, Cards::Hidden),
                // Folded: no cards, name dimmed.
                seat(Some(Button), 300.0, Cards::None),
            ],
            ..hand_one()
        },
        _ => panic!("no expected state for {name}"),
    }
}

#[test]
fn the_captures_are_read_exactly() {
    let reader = TableReader::new();
    for (name, _) in scenarios() {
        assert_eq!(reader.read(&recorded(name)), Ok(expected(name)), "{name}");
    }
}

/// `frame` with every colour channel moved by `shift`, as a capture
/// through another colour pipeline might be.
fn shifted(frame: &Frame, shift: i16) -> Frame {
    let mut rgba = frame.rgba().to_vec();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        for channel in &mut pixel[..3] {
            *channel = (i16::from(*channel) + shift).clamp(0, 255) as u8;
        }
    }
    let size = tiny_skia::IntSize::from_wh(frame.width(), frame.height()).unwrap();
    let pixmap = tiny_skia::Pixmap::from_vec(rgba, size).unwrap();
    Frame::decode_png(&pixmap.encode_png().unwrap()).unwrap()
}

#[test]
fn the_captures_are_read_with_slightly_off_colours() {
    let reader = TableReader::new();
    for (name, _) in scenarios() {
        for shift in [-12, 12] {
            let frame = shifted(&recorded(name), shift);
            assert_eq!(reader.read(&frame), Ok(expected(name)), "{name} {shift}");
        }
    }
}
