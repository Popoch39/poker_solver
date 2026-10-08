//! A game played one action at a time, with an external seat (a human
//! through a client) answering its own decisions.

use std::sync::Arc;

use nitro_hh::ActionKind;
use nitro_simulator::{
    ActError, Decision, NitroGame, Seat, SeatStrategy, Step, Street, Structure, TrivialBot,
};

fn bot(bot: TrivialBot) -> Seat {
    Seat::Strategy(Arc::new(bot) as Arc<dyn SeatStrategy>)
}

/// Steps until the external seat must act, and returns its seat.
fn until_external(game: &mut NitroGame) -> usize {
    for _ in 0..1_000 {
        if let Step::AwaitingExternal { seat } = game.step() {
            return seat;
        }
    }
    panic!("the external seat was never asked to act");
}

#[test]
fn an_external_seat_is_asked_to_act_and_the_game_waits_for_it() {
    let mut game = NitroGame::with_seats(
        Structure::expresso_nitro(),
        [
            Seat::External,
            bot(TrivialBot::AlwaysFold),
            bot(TrivialBot::AlwaysFold),
        ],
        7,
    );
    assert_eq!(until_external(&mut game), 0);
    let view = game.pending_decision().expect("seat 0 must act").clone();
    assert_eq!(view.seat, 0);
    assert_eq!(view.street, Street::Preflop);

    // Nothing moves until the external seat answers.
    assert_eq!(game.step(), Step::AwaitingExternal { seat: 0 });
    assert_eq!(game.pending_decision(), Some(&view));

    game.act(Decision::AllIn).unwrap();
    assert_ne!(game.pending_decision().map(|v| v.seat), Some(0));
}

#[test]
fn stepping_a_game_reports_the_hands_that_playing_it_hand_by_hand_gives() {
    let strategies = || {
        [
            TrivialBot::Random,
            TrivialBot::AlwaysAllIn,
            TrivialBot::Random,
        ]
        .map(|b| Arc::new(b) as Arc<dyn SeatStrategy>)
    };
    let mut by_hand = NitroGame::new(Structure::expresso_nitro(), strategies(), 11);
    let mut expected = Vec::new();
    while let Some(hand) = by_hand.play_hand() {
        expected.push(hand);
    }

    let mut by_step = NitroGame::new(Structure::expresso_nitro(), strategies(), 11);
    let mut reported = Vec::new();
    let mut started = Vec::new();
    loop {
        let asked = by_step.pending_decision().cloned();
        match by_step.step() {
            Step::HandStarted { number } => started.push(number),
            Step::Acted { seat, decision } => {
                let asked = asked.expect("a seat acts only when asked");
                assert_eq!(seat, asked.seat);
                if asked.to_call == 0.0 {
                    assert_ne!(decision, Decision::Fold, "a free fold is a check");
                }
            }
            Step::HandOver(hand) => reported.push(hand),
            Step::AwaitingExternal { seat } => panic!("seat {seat} is not external"),
            Step::GameOver => break,
        }
    }
    assert_eq!(reported, expected);
    assert_eq!(started, (1..=expected.len() as u32).collect::<Vec<_>>());
    assert_eq!(by_step.places(), by_hand.places());
    assert_eq!(by_step.step(), Step::GameOver);
}

#[test]
fn an_external_seat_plays_a_whole_game() {
    let mut game = NitroGame::with_seats(
        Structure::expresso_nitro(),
        [
            bot(TrivialBot::Random),
            Seat::External,
            bot(TrivialBot::Random),
        ],
        12,
    );
    let mut decisions = 0;
    loop {
        match game.step() {
            Step::AwaitingExternal { seat } => {
                assert_eq!(seat, 1);
                decisions += 1;
                let legal = game.pending_decision().unwrap().legal_decisions();
                game.act(*legal.last().unwrap()).unwrap();
            }
            Step::GameOver => break,
            _ => {}
        }
    }
    assert!(decisions > 0);
    let mut places = game.places().expect("the game is over");
    places.sort();
    assert_eq!(places, [1, 2, 3]);
    assert_eq!(game.stacks().iter().sum::<f32>(), 900.0);
}

#[test]
fn an_unraised_pot_can_be_min_raised_to_two_big_blinds_once() {
    let mut game = NitroGame::with_seats(
        Structure::expresso_nitro(),
        [Seat::External, Seat::External, Seat::External],
        3,
    );
    until_external(&mut game);
    let opener = game.pending_decision().unwrap().clone();
    assert_eq!(opener.to_call, 20.0);
    assert!(opener.legal_decisions().contains(&Decision::MinRaise));
    game.act(Decision::MinRaise).unwrap();

    // Raised, the pot cannot be min-raised again; then postflop, nor bet.
    let mut asked = 0;
    while let Some(view) = game.pending_decision().cloned() {
        asked += 1;
        assert!(!view.legal_decisions().contains(&Decision::MinRaise));
        assert_eq!(
            game.act(Decision::MinRaise),
            Err(ActError::Illegal(Decision::MinRaise))
        );
        if view.street == Street::Preflop {
            assert_eq!(view.to_call, 40.0 - view.players[view.seat].street_bet);
        }
        game.act(Decision::Call).unwrap();
    }
    assert!(asked >= 2);
    let history = game.hand_history(None).unwrap();
    let raise = history
        .actions
        .iter()
        .find(|a| matches!(a.kind, ActionKind::Raise { .. }))
        .unwrap();
    assert_eq!(raise.kind, ActionKind::Raise { to: 40 });
    assert_eq!(raise.seat, opener.seat as u8 + 1);
}

#[test]
fn nothing_can_be_answered_before_the_first_hand_is_dealt() {
    let mut game = NitroGame::with_seats(
        Structure::expresso_nitro(),
        [Seat::External, Seat::External, Seat::External],
        1,
    );
    assert_eq!(game.pending_decision(), None);
    assert_eq!(game.act(Decision::Call), Err(ActError::NoDecisionPending));
    assert_eq!(game.step(), Step::HandStarted { number: 1 });
    assert!(game.pending_decision().is_some());
}

#[test]
fn an_external_seat_can_only_take_a_legal_decision() {
    // The external seat calls every bet and the bots check or fold, so it
    // soon has nothing to call: it can then check or go all-in, not fold.
    let mut game = NitroGame::with_seats(
        Structure::expresso_nitro(),
        [
            Seat::External,
            bot(TrivialBot::AlwaysFold),
            bot(TrivialBot::AlwaysFold),
        ],
        0,
    );
    loop {
        until_external(&mut game);
        let view = game.pending_decision().unwrap().clone();
        if view.to_call == 0.0 {
            assert_eq!(view.legal_decisions(), [Decision::Call, Decision::AllIn]);
            assert_eq!(
                game.act(Decision::Fold),
                Err(ActError::Illegal(Decision::Fold))
            );
            assert_eq!(game.pending_decision(), Some(&view));
            game.act(Decision::Call).unwrap();
            break;
        }
        assert_eq!(view.legal_decisions(), Decision::ALL);
        game.act(Decision::Call).unwrap();
    }
}
