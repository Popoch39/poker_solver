//! Reading the table from the local client's frames, rendered offscreen by
//! the client's own renderer.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use nitro_bot::{Cards, ReadError, TableReader};
use nitro_local_client::{ClientConfig, HERO, LAYOUT, LocalClient, Next, Rect};
use nitro_simulator::{SeatStrategy, Structure, TrivialBot};

fn client(seed: u64, bots: [TrivialBot; 2]) -> LocalClient {
    LocalClient::new(ClientConfig {
        structure: Structure::expresso_nitro(),
        seed,
        bots: bots.map(|b| Arc::new(b) as Arc<dyn SeatStrategy>),
        bot_delay: Duration::ZERO,
        hand_over_delay: Duration::ZERO,
    })
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
fn reads_the_heros_cards() {
    let reader = TableReader::new();
    let mut client = client(1, [TrivialBot::Random; 2]);
    until_hero_acts(&mut client);
    let state = reader.read(&client.frame()).unwrap();
    assert_eq!(state.hero_cards(), client.view().seats[HERO].hole_cards);
    assert!(state.hero_cards().is_some());
}

#[test]
fn refuses_a_frame_that_is_not_the_whole_window() {
    // What a capture at another scale, or of a resized window, gives.
    let reader = TableReader::new();
    let mut client = client(1, [TrivialBot::Random; 2]);
    until_hero_acts(&mut client);
    let frame = client.frame().crop(Rect::new(0, 0, 900, 600));
    assert_eq!(
        reader.read(&frame),
        Err(ReadError::Size {
            width: 900,
            height: 600
        })
    );
}

#[test]
fn reads_the_stacks_and_the_pot() {
    let reader = TableReader::new();
    let mut client = client(4, [TrivialBot::Random; 2]);
    until_hero_acts(&mut client);
    let view = client.view();
    let state = reader.read(&client.frame()).unwrap();
    assert!(view.pot > 0.0);
    assert_eq!(state.pot, view.pot);
    for (seat, read) in state.seats.iter().enumerate() {
        assert_eq!(read.stack, view.seats[seat].stack, "seat {seat}");
    }
}

#[test]
fn reads_the_blinds_the_bets_and_the_heros_options() {
    let reader = TableReader::new();
    let mut client = client(4, [TrivialBot::Random; 2]);
    until_hero_acts(&mut client);
    let view = client.view();
    let state = reader.read(&client.frame()).unwrap();
    assert_eq!(
        (state.hand_number, state.level),
        (view.hand_number, view.level)
    );
    assert_eq!(
        (state.small_blind, state.big_blind),
        (view.small_blind, view.big_blind)
    );
    for (seat, read) in state.seats.iter().enumerate() {
        assert_eq!(read.street_bet, view.seats[seat].street_bet, "seat {seat}");
    }
    assert!(view.to_call > 0.0);
    assert_eq!(state.legal, view.legal);
    assert_eq!(state.to_call, view.to_call);
}

#[test]
fn reads_what_each_seat_won() {
    let reader = TableReader::new();
    let mut client = client(9, [TrivialBot::AlwaysAllIn; 2]);
    until_hero_acts(&mut client);
    let (x, y) = LAYOUT.call.center();
    client.click(x, y).unwrap();
    let view = client.view();
    assert!(view.hand_over);
    let state = reader.read(&client.frame()).unwrap();
    for (seat, read) in state.seats.iter().enumerate() {
        assert_eq!(read.won, view.seats[seat].won, "seat {seat}");
    }
    assert!(state.seats.iter().any(|s| s.won > 0.0));
    assert!(state.legal.is_empty());
}

#[test]
fn reads_positions_the_dealer_button_and_who_is_to_act() {
    let reader = TableReader::new();
    let mut client = client(2, [TrivialBot::AlwaysFold; 2]);
    let mut buttons = HashSet::new();
    let mut bots_to_act = HashSet::new();
    let mut hero_to_act = false;
    // The hero calls (or checks) through a few hands.
    while buttons.len() < 3 || bots_to_act.len() < 2 || !hero_to_act {
        let next = client.advance();
        let view = client.view();
        let state = reader.read(&client.frame()).unwrap();
        assert_eq!(state.to_act, view.to_act);
        assert_eq!(state.button, view.button);
        for (seat, read) in state.seats.iter().enumerate() {
            assert_eq!(read.position, view.seats[seat].position, "seat {seat}");
        }
        assert_eq!(state.hero_position(), view.seats[HERO].position);
        buttons.insert(view.button);
        bots_to_act.extend(view.to_act.filter(|&seat| seat != HERO));
        match next {
            Next::WaitForClick => {
                assert_eq!(state.to_act, Some(HERO));
                hero_to_act = true;
                let (x, y) = LAYOUT.call.center();
                client.click(x, y).unwrap();
            }
            Next::GameOver => panic!("the game ended too early"),
            Next::After(_) => {}
        }
    }
}

#[test]
fn reads_the_board_and_the_cards_shown_down() {
    let reader = TableReader::new();
    let mut client = client(9, [TrivialBot::AlwaysAllIn; 2]);
    until_hero_acts(&mut client);
    // The bots are all-in: calling plays the hand out to the river.
    let (x, y) = LAYOUT.call.center();
    client.click(x, y).unwrap();
    let view = client.view();
    let state = reader.read(&client.frame()).unwrap();
    assert_eq!(state.board, view.board);
    assert_eq!(state.board.len(), 5);
    for (seat, read) in state.seats.iter().enumerate() {
        let shown = view.seats[seat].hole_cards.expect("shown down");
        assert_eq!(read.cards, Cards::Shown(shown), "seat {seat}");
    }
}
