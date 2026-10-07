//! The frame shows exactly what the simulator says, where the layout says.

use std::sync::Arc;
use std::time::Duration;

use nitro_local_client::{
    ClientConfig, Frame, HERO, LAYOUT, LocalClient, Next, Rect, card_back, card_sprite, render,
};
use nitro_simulator::{
    Card, Decision, SeatStrategy, Structure, Suit, TableView, TrivialBot, Value,
};

fn config(seed: u64, bots: [TrivialBot; 2]) -> ClientConfig {
    ClientConfig {
        structure: Structure::expresso_nitro(),
        seed,
        bots: bots.map(|b| Arc::new(b) as Arc<dyn SeatStrategy>),
        bot_delay: Duration::from_millis(500),
        hand_over_delay: Duration::from_millis(2_000),
    }
}

/// Advances until the hero must click.
fn until_hero_acts(client: &mut LocalClient) {
    for _ in 0..10_000 {
        match client.advance() {
            Next::WaitForClick => return,
            Next::GameOver => panic!("the game ended before the hero acted"),
            Next::After(_) => {}
        }
    }
    panic!("the hero was never asked to act");
}

#[test]
fn the_hero_sees_its_cards_and_the_backs_of_the_bots_cards() {
    let mut client = LocalClient::new(config(1, [TrivialBot::Random; 2]));
    until_hero_acts(&mut client);
    let view = client.view();
    let frame = client.frame();
    assert_eq!(
        (frame.width(), frame.height()),
        (LAYOUT.window.width, LAYOUT.window.height)
    );

    let hole_cards = view.seats[HERO]
        .hole_cards
        .expect("the hero sees its cards");
    for (rect, card) in LAYOUT.seats[HERO].cards.iter().zip(hole_cards) {
        assert_eq!(frame.crop(*rect), card_sprite(card), "{card:?}");
    }
    for seat in [1, 2] {
        if view.seats[seat].in_hand {
            for rect in LAYOUT.seats[seat].cards {
                assert_eq!(frame.crop(rect), card_back(), "seat {seat}");
            }
        }
    }
}

#[test]
fn the_board_shows_the_simulator_cards_in_order() {
    let mut client = LocalClient::new(config(9, [TrivialBot::AlwaysAllIn; 2]));
    // The bots go all-in; calling plays the hand out to the river.
    until_hero_acts(&mut client);
    let (x, y) = LAYOUT.call.center();
    client.click(x, y).unwrap();
    let view = client.view();
    assert_eq!(view.board.len(), 5, "{view:?}");
    let frame = client.frame();
    for (rect, card) in LAYOUT.board.iter().zip(&view.board) {
        assert_eq!(frame.crop(*rect), card_sprite(*card), "{card:?}");
    }
    // Shown down: every seat's cards are face up.
    for seat in 0..3 {
        let cards = view.seats[seat].hole_cards.expect("shown down");
        for (rect, card) in LAYOUT.seats[seat].cards.iter().zip(cards) {
            assert_eq!(frame.crop(*rect), card_sprite(card), "seat {seat}");
        }
    }
}

#[test]
fn every_card_looks_different() {
    let cards: Vec<Card> = Value::values()
        .into_iter()
        .flat_map(|v| Suit::suits().map(|s| Card::new(v, s)))
        .collect();
    let sprites: Vec<Frame> = cards.iter().map(|&c| card_sprite(c)).collect();
    for (i, sprite) in sprites.iter().enumerate() {
        assert_eq!(
            (sprite.width(), sprite.height()),
            (LAYOUT.board[0].width, LAYOUT.board[0].height)
        );
        assert_ne!(*sprite, card_back());
        for (j, other) in sprites.iter().enumerate().skip(i + 1) {
            assert_ne!(sprite, other, "{:?} and {:?}", cards[i], cards[j]);
        }
    }
}

#[test]
fn the_regions_fit_in_the_window_without_overlapping() {
    let mut regions: Vec<(String, Rect)> = vec![
        ("info".into(), LAYOUT.info),
        ("status".into(), LAYOUT.status),
        ("pot".into(), LAYOUT.pot),
        ("fold".into(), LAYOUT.fold),
        ("call".into(), LAYOUT.call),
        ("all-in".into(), LAYOUT.all_in),
    ];
    for (i, rect) in LAYOUT.board.iter().enumerate() {
        regions.push((format!("board {i}"), *rect));
    }
    for (seat, layout) in LAYOUT.seats.iter().enumerate() {
        for (i, rect) in layout.cards.iter().enumerate() {
            regions.push((format!("seat {seat} card {i}"), *rect));
        }
        for (name, rect) in [
            ("name", layout.name),
            ("stack", layout.stack),
            ("bet", layout.bet),
            ("dealer", layout.dealer),
            ("turn", layout.turn),
        ] {
            regions.push((format!("seat {seat} {name}"), rect));
        }
    }
    let window = LAYOUT.window;
    for (i, (name, rect)) in regions.iter().enumerate() {
        assert!(
            rect.x + rect.width <= window.width && rect.y + rect.height <= window.height,
            "{name} is outside the window"
        );
        for (other, other_rect) in &regions[i + 1..] {
            assert!(!rect.intersects(other_rect), "{name} overlaps {other}");
        }
    }
}

/// The pixels that differ between two frames.
fn changed_pixels(a: &Frame, b: &Frame) -> Vec<(u32, u32)> {
    (0..a.height())
        .flat_map(|y| (0..a.width()).map(move |x| (x, y)))
        .filter(|&(x, y)| a.pixel(x, y) != b.pixel(x, y))
        .collect()
}

/// Asserts that something changed, and only inside `regions`.
fn assert_changed_only_inside(changed: &[(u32, u32)], regions: &[Rect], what: &str) {
    assert!(!changed.is_empty(), "{what}: nothing was redrawn");
    let outside: Vec<_> = changed
        .iter()
        .filter(|&&(x, y)| !regions.iter().any(|r| r.contains(x, y)))
        .collect();
    assert!(
        outside.is_empty(),
        "{what}: redrawn outside at {:?}",
        &outside[..1]
    );
}

/// A frame with each piece of state changed alone changes only that piece's
/// regions: the bot can read each value at a fixed place.
#[test]
fn each_value_is_drawn_in_its_own_region_only() {
    let mut client = LocalClient::new(config(1, [TrivialBot::Random; 2]));
    until_hero_acts(&mut client);
    let view = client.view();
    assert!(view.legal.contains(&Decision::AllIn), "{view:?}");
    let names = client.names().clone();
    let base = render(&view, &names, "STATUS");
    let changed = |edit: &dyn Fn(&mut TableView)| {
        let mut other = view.clone();
        edit(&mut other);
        changed_pixels(&base, &render(&other, &names, "STATUS"))
    };

    for seat in 0..3 {
        let layout = LAYOUT.seats[seat];
        let changed_stack = changed(&|v| v.seats[seat].stack += 1.0);
        if seat == HERO {
            // The all-in button says how much the hero puts in.
            let regions = [layout.stack, LAYOUT.all_in];
            assert_changed_only_inside(&changed_stack, &regions, "hero stack");
        } else {
            assert_changed_only_inside(&changed_stack, &[layout.stack], "bot stack");
        }
        let changed_bet = changed(&|v| v.seats[seat].street_bet += 1.0);
        assert_changed_only_inside(&changed_bet, &[layout.bet], "bet");
    }
    let changed_to_call = changed(&|v| v.to_call += 1.0);
    assert_changed_only_inside(&changed_to_call, &[LAYOUT.call], "to call");
    assert_changed_only_inside(&changed(&|v| v.pot += 1.0), &[LAYOUT.pot], "pot");
    let changed_hand = changed(&|v| v.hand_number += 1);
    assert_changed_only_inside(&changed_hand, &[LAYOUT.info], "hand");
    let changed_status = changed_pixels(&base, &render(&view, &names, "OTHER"));
    assert_changed_only_inside(&changed_status, &[LAYOUT.status], "status");
    let changed_board = changed(&|v| v.board = vec![Card::new(Value::Ace, Suit::Spade)]);
    assert_changed_only_inside(&changed_board, &[LAYOUT.board[0]], "board");
    let next_button = (view.button + 1) % 3;
    let changed_button = changed(&|v| v.button = next_button);
    let dealers = [
        LAYOUT.seats[view.button].dealer,
        LAYOUT.seats[next_button].dealer,
    ];
    assert_changed_only_inside(&changed_button, &dealers, "button");
}

#[test]
fn the_same_seed_and_clicks_give_the_same_frames() {
    let mut one = LocalClient::new(config(21, [TrivialBot::Random; 2]));
    let mut two = LocalClient::new(config(21, [TrivialBot::Random; 2]));
    for _ in 0..200 {
        let next = one.advance();
        assert_eq!(two.advance(), next);
        assert_eq!(one.frame(), two.frame());
        match next {
            Next::WaitForClick => {
                let (x, y) = LAYOUT.call.center();
                one.click(x, y).unwrap();
                two.click(x, y).unwrap();
                assert_eq!(one.frame(), two.frame());
            }
            Next::GameOver => break,
            Next::After(_) => {}
        }
    }
}

#[test]
fn an_offscreen_frame_round_trips_through_png() {
    let mut client = LocalClient::new(config(1, [TrivialBot::Random; 2]));
    until_hero_acts(&mut client);
    let frame = client.frame();
    let png = frame.encode_png();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(Frame::decode_png(&png).unwrap(), frame);
}
