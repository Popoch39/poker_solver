//! What an observer sees of the table at every step: its own cards, the
//! public state, and the cards shown down.

use std::sync::Arc;

use nitro_simulator::{
    Decision, NitroGame, Position, Seat, SeatStrategy, Step, Street, Structure, TrivialBot,
};

fn bot(bot: TrivialBot) -> Seat {
    Seat::Strategy(Arc::new(bot) as Arc<dyn SeatStrategy>)
}

fn game(seats: [Seat; 3], seed: u64) -> NitroGame {
    NitroGame::with_seats(Structure::expresso_nitro(), seats, seed)
}

/// Steps until the external seat 0 must act, or returns `false` at the end
/// of the hand.
fn until_seat_0_acts_in_this_hand(game: &mut NitroGame) -> bool {
    loop {
        match game.step() {
            Step::AwaitingExternal { seat: 0 } => return true,
            Step::HandOver(_) | Step::GameOver => return false,
            _ => {}
        }
    }
}

/// Steps until the current hand is over (before it is reported).
fn until_hand_is_over(game: &mut NitroGame, decision: Decision) {
    while !game.table_view(0).hand_over {
        if let Step::AwaitingExternal { .. } = game.step() {
            game.act(decision).unwrap();
        }
    }
}

#[test]
fn before_the_first_hand_the_table_is_empty() {
    let game = game(
        [
            Seat::External,
            bot(TrivialBot::Random),
            bot(TrivialBot::Random),
        ],
        3,
    );
    let view = game.table_view(0);
    assert_eq!(view.hand_number, 0);
    assert_eq!(view.level, 1);
    assert_eq!((view.small_blind, view.big_blind), (10.0, 20.0));
    assert_eq!(view.button, game.button());
    assert_eq!(view.pot, 0.0);
    assert!(view.board.is_empty());
    assert_eq!(view.to_act, None);
    assert!(view.legal.is_empty());
    assert!(!view.hand_over);
    for seat in &view.seats {
        assert_eq!(seat.stack, 300.0);
        assert_eq!(seat.position, None);
        assert!(!seat.in_hand);
        assert_eq!(seat.hole_cards, None);
        assert_eq!(seat.place, None);
    }
}

#[test]
fn the_observer_sees_its_own_cards_and_the_public_state() {
    let mut game = game(
        [
            Seat::External,
            bot(TrivialBot::Random),
            bot(TrivialBot::Random),
        ],
        4,
    );
    let mut checked = 0;
    while checked < 5 {
        if !until_seat_0_acts_in_this_hand(&mut game) {
            continue;
        }
        let asked = game.pending_decision().unwrap().clone();
        let view = game.table_view(0);
        assert_eq!(view.hand_number, asked.hand_number);
        assert_eq!(
            (view.small_blind, view.big_blind),
            (asked.small_blind, asked.big_blind)
        );
        assert_eq!(view.street, asked.street);
        assert_eq!(view.board, asked.board);
        assert_eq!(view.pot, asked.pot);
        assert_eq!(view.to_act, Some(0));
        assert_eq!(view.to_call, asked.to_call);
        assert_eq!(view.legal, asked.legal_decisions());
        assert_eq!(view.seats[0].hole_cards, Some(asked.hole_cards));
        for player in &asked.players {
            let seat = &view.seats[player.seat];
            assert_eq!(seat.stack, player.stack);
            assert_eq!(seat.street_bet, player.street_bet);
            assert_eq!(seat.position, Some(player.position));
            assert_eq!(seat.in_hand, !player.folded);
            assert_eq!(seat.all_in, player.all_in);
            if player.position == Position::Button {
                assert_eq!(view.button, player.seat);
            }
            if player.seat != 0 {
                assert_eq!(seat.hole_cards, None, "seat {} is hidden", player.seat);
            }
        }
        // Another observer sees the same table, but its own cards.
        let other = game.table_view(1);
        assert_eq!(other.seats[0].hole_cards, None);
        assert!(other.legal.is_empty());
        assert_eq!(other.pot, view.pot);

        game.act(Decision::Call).unwrap();
        checked += 1;
    }
}

#[test]
fn cards_shown_down_are_revealed_once_the_hand_is_over() {
    let mut game = game(
        [
            Seat::External,
            bot(TrivialBot::AlwaysAllIn),
            bot(TrivialBot::AlwaysAllIn),
        ],
        5,
    );
    game.step();
    until_hand_is_over(&mut game, Decision::Call);
    let view = game.table_view(0);
    assert_eq!(view.board.len(), 5);
    assert_eq!(view.street, Street::River);
    assert_eq!(view.to_act, None);
    assert!(view.legal.is_empty());
    for (seat, shown) in view.seats.iter().enumerate() {
        assert!(shown.hole_cards.is_some(), "seat {seat}: {shown:?}");
    }
    let won: f32 = view.seats.iter().map(|s| s.won).sum();
    // Split pots are shared in floating point.
    assert!((won - view.pot).abs() < 1e-3, "{won} won of {}", view.pot);
    assert_eq!(view.seats.map(|s| s.stack), game.stacks());

    // The view stays on the finished hand until the next one is dealt.
    assert!(matches!(game.step(), Step::HandOver(_)));
    assert_eq!(game.table_view(0), view);
}

#[test]
fn cards_stay_hidden_when_everyone_else_folds() {
    let mut game = game(
        [
            Seat::External,
            bot(TrivialBot::AlwaysFold),
            bot(TrivialBot::AlwaysFold),
        ],
        6,
    );
    for _ in 0..3 {
        game.step();
        until_hand_is_over(&mut game, Decision::AllIn);
        let view = game.table_view(0);
        assert!(view.hand_over);
        assert!(view.seats[0].hole_cards.is_some());
        assert_eq!(view.seats[1].hole_cards, None);
        assert_eq!(view.seats[2].hole_cards, None);
        assert_eq!(view.seats.iter().filter(|s| s.won > 0.0).count(), 1);
        game.step();
    }
}

#[test]
fn eliminated_seats_show_their_place() {
    let mut game = game(
        [
            Seat::External,
            bot(TrivialBot::AlwaysAllIn),
            bot(TrivialBot::AlwaysAllIn),
        ],
        8,
    );
    loop {
        match game.step() {
            Step::AwaitingExternal { .. } => game.act(Decision::Call).unwrap(),
            Step::GameOver => break,
            _ => {}
        }
    }
    let view = game.table_view(0);
    let places = game.places().unwrap();
    for (seat, place) in view.seats.iter().zip(places) {
        assert_eq!(seat.place, Some(place));
        assert_eq!(seat.stack > 0.0, place == 1);
    }
}
