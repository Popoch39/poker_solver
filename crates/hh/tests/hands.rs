//! Parsing Winamax hand-history text into structured hands.
//!
//! The fixture holds three real hands (4, 10 and 15) of the 1 € Expresso Nitro
//! 618031930 of 2023-01-02, with the pseudonyms replaced.

use chrono::{TimeZone, Utc};
use nitro_hh::{
    Action, ActionKind, BuyIn, Card, Euros, Hand, Player, Street, TournamentInfo, parse_hands,
};

const FIXTURE: &str = include_str!("fixtures/nitro-618031930.txt");

fn card(text: &str) -> Card {
    Card::try_from(text).unwrap()
}

fn cards(text: &str) -> Vec<Card> {
    text.split(' ').map(card).collect()
}

fn hole(text: &str) -> Option<[Card; 2]> {
    Some(cards(text).try_into().unwrap())
}

fn act(street: Street, seat: u8, kind: ActionKind) -> Action {
    Action {
        street,
        seat,
        kind,
        all_in: false,
    }
}

fn all_in(street: Street, seat: u8, kind: ActionKind) -> Action {
    Action {
        all_in: true,
        ..act(street, seat, kind)
    }
}

fn fixture_hands() -> Vec<Hand> {
    parse_hands(FIXTURE)
        .into_iter()
        .collect::<Result<_, _>>()
        .expect("every fixture hand parses")
}

fn nitro_1_euro() -> Option<TournamentInfo> {
    Some(TournamentInfo {
        id: "618031930".into(),
        name: "Expresso Nitro".into(),
        buy_in: BuyIn {
            prize: Euros::from_cents(93),
            rake: Euros::from_cents(7),
        },
    })
}

#[test]
fn a_three_handed_hand_to_showdown_is_parsed_exactly() {
    use ActionKind::*;
    use Street::*;
    let hand = &fixture_hands()[0];
    let expected = Hand {
        game_number: "2654426927233761281-4-1672660021".into(),
        started_at: Utc.with_ymd_and_hms(2023, 1, 2, 11, 47, 1).unwrap(),
        tournament: nitro_1_euro(),
        level: Some(1),
        table_name: "Expresso Nitro(618031930)#0".into(),
        table_size: 3,
        button: 2,
        small_blind: 10,
        big_blind: 20,
        players: vec![
            Player {
                seat: 1,
                name: "villain 1".into(),
                stack: 270,
                cards: None,
                showed: false,
                collected: 0,
            },
            Player {
                seat: 2,
                name: "hero".into(),
                stack: 220,
                cards: hole("Js Jh"),
                showed: true,
                collected: 180,
            },
            Player {
                seat: 3,
                name: "villain 2".into(),
                stack: 410,
                cards: hole("6s Qd"),
                showed: true,
                collected: 0,
            },
        ],
        hero: Some(2),
        actions: vec![
            act(Preflop, 3, SmallBlind(10)),
            act(Preflop, 1, BigBlind(20)),
            act(Preflop, 2, Raise { to: 40 }),
            act(Preflop, 3, Call(30)),
            act(Preflop, 1, Fold),
            act(Flop, 3, Bet(20)),
            act(Flop, 2, Call(20)),
            act(Turn, 3, Bet(20)),
            act(Turn, 2, Call(20)),
            act(River, 3, Check),
            act(River, 2, Check),
        ],
        board: cards("6h 3h 5d Ks Ac"),
    };
    assert_eq!(hand, &expected);
}

/// Parses the fixture with `from` replaced by `to` (exactly once), and checks
/// that only the first hand fails, at `line`, with a reason containing `why`.
fn assert_first_hand_rejected(from: &str, to: &str, line: usize, why: &str) {
    assert_eq!(FIXTURE.matches(from).count(), 1, "{from:?}");
    let results = parse_hands(&FIXTURE.replace(from, to));
    assert_eq!(results.len(), 3);
    let error = results[0]
        .as_ref()
        .expect_err("the damaged hand is rejected");
    assert_eq!(error.line, line, "{error}");
    assert!(error.reason.contains(why), "{error}");
    let others: Vec<_> = results[1..].iter().filter(|r| r.is_ok()).collect();
    assert_eq!(others.len(), 2, "the other hands still parse");
}

#[test]
fn a_malformed_line_rejects_its_hand_only() {
    assert_first_hand_rejected(
        "villain 2 calls 30",
        "villain 2 calls thirty",
        12,
        "invalid number",
    );
    assert_first_hand_rejected(
        "villain 1 folds",
        "villain 1 sits out",
        13,
        "unrecognised line",
    );
}

#[test]
fn a_hand_whose_chips_do_not_add_up_is_rejected() {
    assert_first_hand_rejected("Total pot 180 |", "Total pot 190 |", 28, "total pot");
    assert_first_hand_rejected(
        "hero collected 180 from pot",
        "hero collected 170 from pot",
        28,
        "collected",
    );
    // A raise increment that does not match the amount to call.
    assert_first_hand_rejected("hero raises 20 to 40", "hero raises 30 to 40", 11, "raise");
    assert_first_hand_rejected(
        "villain 2 bets 20\nhero calls 20\n*** TURN",
        "villain 2 bets 20\nhero calls 20 and is all-in\n*** TURN",
        16,
        "all-in",
    );
}

#[test]
fn a_truncated_hand_is_rejected() {
    let (first, rest) = FIXTURE.split_once("*** SUMMARY ***").unwrap();
    let (_, others) = rest.split_once("\n\n\n").unwrap();
    let results = parse_hands(&format!("{first}\n\n{others}"));
    let error = results[0].as_ref().expect_err("truncated hand");
    assert!(error.reason.contains("summary"), "{error}");
    assert!(results[1].is_ok() && results[2].is_ok());
}

#[test]
fn a_heads_up_all_in_with_a_split_pot_credits_each_player_once() {
    use ActionKind::*;
    use Street::*;
    let hand = &fixture_hands()[1];
    assert_eq!(hand.level, Some(5));
    assert_eq!((hand.small_blind, hand.big_blind), (40, 80));
    // Heads-up, the button posts the small blind.
    assert_eq!(hand.button, 1);
    assert_eq!(
        hand.actions,
        vec![
            act(Preflop, 1, SmallBlind(40)),
            act(Preflop, 2, BigBlind(80)),
            act(Preflop, 1, Raise { to: 160 }),
            all_in(Preflop, 2, Raise { to: 510 }),
            all_in(Preflop, 1, Call(230)),
        ]
    );
    assert_eq!(hand.board, cards("Qd 4s 7h 9h 6c"));
    let seats: Vec<(u8, u32, u32, Option<[Card; 2]>)> = hand
        .players
        .iter()
        .map(|p| (p.seat, p.stack, p.collected, p.cards))
        .collect();
    // 390 is half of the 780 main pot; hero's 510 adds the 120 nobody called.
    assert_eq!(
        seats,
        vec![(1, 390, 390, hole("2s Ac")), (2, 510, 510, hole("Ad 3s")),]
    );
}

#[test]
fn an_uncalled_excess_comes_back_to_the_bettor() {
    use ActionKind::*;
    use Street::*;
    let hand = &fixture_hands()[2];
    assert_eq!(
        hand.actions,
        vec![
            act(Preflop, 2, SmallBlind(50)),
            act(Preflop, 1, BigBlind(100)),
            all_in(Preflop, 2, Raise { to: 520 }),
            all_in(Preflop, 1, Call(280)),
        ]
    );
    let collected: Vec<u32> = hand.players.iter().map(|p| p.collected).collect();
    assert_eq!(collected, vec![760, 140]);
}
