//! Clicks become the hero's decisions, and only legal ones.

use std::sync::Arc;
use std::time::Duration;

use nitro_local_client::{ClientConfig, HERO, LAYOUT, LocalClient, Next};
use nitro_simulator::{Decision, SeatStrategy, Structure, TrivialBot};

fn client(seed: u64, bots: [TrivialBot; 2]) -> LocalClient {
    LocalClient::new(ClientConfig {
        structure: Structure::expresso_nitro(),
        seed,
        bots: bots.map(|b| Arc::new(b) as Arc<dyn SeatStrategy>),
        bot_delay: Duration::from_millis(500),
        hand_over_delay: Duration::from_millis(2_000),
    })
}

/// Advances until the hero must click, or returns `false` at the end.
fn until_hero_acts(client: &mut LocalClient) -> bool {
    loop {
        match client.advance() {
            Next::WaitForClick => return true,
            Next::GameOver => return false,
            Next::After(_) => {}
        }
    }
}

#[test]
fn only_the_legal_buttons_are_drawn() {
    let mut client = client(2, [TrivialBot::AlwaysFold; 2]);
    let mut seen_check = false;
    let mut seen_fold = false;
    while !(seen_check && seen_fold) {
        assert!(until_hero_acts(&mut client), "the game ended first");
        let view = client.view();
        let frame = client.frame();
        for decision in Decision::ALL {
            let rect = LAYOUT.button(decision);
            let (x, y) = (rect.x + 2, rect.y + 2);
            let drawn = frame.pixel(x, y) != frame.pixel(LAYOUT.window.width - 1, y);
            assert_eq!(drawn, view.legal.contains(&decision), "{decision:?}");
        }
        seen_fold |= view.legal.contains(&Decision::Fold);
        seen_check |= !view.legal.contains(&Decision::Fold);
        // Calling every bet and checking otherwise reaches both cases.
        let (x, y) = LAYOUT.call.center();
        client.click(x, y).expect("call is always legal");
    }
}

#[test]
fn a_click_on_a_legal_button_plays_it() {
    let mut client = client(3, [TrivialBot::AlwaysFold; 2]);
    assert!(until_hero_acts(&mut client));
    let before = client.view();
    assert!(
        before.to_call > 0.0,
        "the hero opens from the BTN or the SB"
    );
    let (x, y) = LAYOUT.call.center();
    assert!(matches!(client.click(x, y), Some(Next::After(_))));
    let after = client.view();
    assert_eq!(
        after.seats[HERO].street_bet,
        before.seats[HERO].street_bet + before.to_call
    );
    assert_eq!(client.status(), format!("YOU CALL {}", before.to_call));
}

#[test]
fn a_fold_leaves_the_hand_and_shows_the_winner() {
    let mut client = client(3, [TrivialBot::AlwaysAllIn; 2]);
    assert!(until_hero_acts(&mut client));
    let (x, y) = LAYOUT.fold.center();
    assert!(matches!(client.click(x, y), Some(Next::After(_))));
    let view = client.view();
    assert!(!view.seats[HERO].in_hand, "{view:?}");
    // Both bots are all-in: the hand is played out at once.
    assert!(view.hand_over);
    let (winner, seat) = view
        .seats
        .iter()
        .enumerate()
        .find(|(_, s)| s.won > 0.0)
        .unwrap();
    assert!(
        client
            .status()
            .starts_with(&format!("BOT {winner} WINS {}", seat.won))
    );
}

#[test]
fn clicks_elsewhere_or_on_an_illegal_button_do_nothing() {
    let mut client = client(4, [TrivialBot::AlwaysFold; 2]);
    // Before the hero is asked, no button does anything.
    let (x, y) = LAYOUT.call.center();
    assert_eq!(client.click(x, y), None);
    // Calling every bet soon leaves the hero with nothing to call: fold is
    // then not a button.
    loop {
        assert!(until_hero_acts(&mut client));
        if client.view().to_call == 0.0 {
            break;
        }
        client.click(x, y).unwrap();
    }
    let view = client.view();
    let (fold_x, fold_y) = LAYOUT.fold.center();
    assert_eq!(client.click(fold_x, fold_y), None);
    let (pot_x, pot_y) = LAYOUT.pot.center();
    assert_eq!(client.click(pot_x, pot_y), None);
    assert_eq!(client.view(), view);
}

#[test]
fn the_hero_can_play_a_whole_tournament_by_clicking() {
    let mut client = client(5, [TrivialBot::Random; 2]);
    let mut clicks = 0;
    loop {
        match client.advance() {
            Next::WaitForClick => {
                // Cycle through the legal buttons.
                let legal = client.view().legal;
                let (x, y) = LAYOUT.button(legal[clicks % legal.len()]).center();
                client.click(x, y).expect("a legal button");
                clicks += 1;
            }
            Next::GameOver => break,
            Next::After(_) => {}
        }
    }
    assert!(clicks > 0);
    let view = client.view();
    assert!(view.seats.iter().all(|s| s.place.is_some()), "{view:?}");
    let place = view.seats[HERO].place.unwrap();
    let ordinal = ["1ST", "2ND", "3RD"][place as usize - 1];
    assert_eq!(client.status(), format!("GAME OVER: YOU FINISH {ordinal}"));
    assert_eq!(client.advance(), Next::GameOver);
}
