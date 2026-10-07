//! Exporting hands to Open Hand History (OHH) and reading them back.

use std::io::Cursor;

use nitro_hh::ohh::{Action as OhhAction, HandHistory, HandReader, write_hand};
use nitro_hh::{Hand, from_ohh, parse_hands, parse_summary, to_ohh};

const HANDS: &str = include_str!("fixtures/nitro-618031930.txt");
const SUMMARY: &str = include_str!("fixtures/nitro-618031930_summary.txt");

fn fixture_hands() -> Vec<Hand> {
    parse_hands(HANDS)
        .into_iter()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// Writes hands in the OHH storage format and reads them back, as another
/// program would.
fn through_ohh_file(histories: Vec<HandHistory>) -> Vec<HandHistory> {
    let mut file = Vec::new();
    for history in histories {
        write_hand(&mut file, history).unwrap();
    }
    HandReader::from_reader(Cursor::new(file))
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn hands_round_trip_through_an_ohh_file() {
    let summary = parse_summary(SUMMARY).unwrap();
    let hands = fixture_hands();
    let histories = hands.iter().map(|h| to_ohh(h, Some(&summary))).collect();

    let read_back: Vec<Hand> = through_ohh_file(histories)
        .iter()
        .map(|h| from_ohh(h).unwrap())
        .collect();

    // OHH has no field for the blind level.
    let expected: Vec<Hand> = hands
        .into_iter()
        .map(|hand| Hand {
            level: None,
            ..hand
        })
        .collect();
    assert_eq!(read_back, expected);
}

#[test]
fn the_export_follows_the_ohh_conventions() {
    let summary = parse_summary(SUMMARY).unwrap();
    let ohh = to_ohh(&fixture_hands()[0], Some(&summary));

    assert_eq!(ohh.site_name, "Winamax");
    assert_eq!(ohh.game_number, "2654426927233761281-4-1672660021");
    assert_eq!(ohh.dealer_seat, 2);
    assert_eq!((ohh.small_blind_amount, ohh.big_blind_amount), (10.0, 20.0));
    let info = ohh.tournament_info.as_ref().unwrap();
    assert_eq!(info.tournament_number, "618031930");
    assert_eq!((info.buyin_amount, info.fee_amount), (0.93, 0.07));
    assert_eq!(info.initial_stack, 300);
    assert_eq!(info.speed.round_time, 60);

    let hero = ohh.hero_player_id.unwrap();
    assert_eq!(ohh.players[hero as usize].seat, 2);

    let preflop = &ohh.rounds[0];
    assert_eq!(preflop.street, "Preflop");
    let summary_of = |a: &nitro_hh::ohh::ActionObj| {
        let seat = ohh.players[a.player_id as usize].seat;
        (a.action_number, seat, a.action.clone(), a.amount)
    };
    // Amounts are the chips each action adds: the raise to 40 adds 40, the
    // small blind's call adds 30.
    assert_eq!(
        preflop.actions.iter().map(summary_of).collect::<Vec<_>>(),
        vec![
            (0, 3, OhhAction::PostSmallBlind, 10.0),
            (1, 1, OhhAction::PostBigBlind, 20.0),
            (2, 2, OhhAction::DealtCards, 0.0),
            (3, 2, OhhAction::Raise, 40.0),
            (4, 3, OhhAction::Call, 30.0),
            (5, 1, OhhAction::Fold, 0.0),
        ]
    );
    let streets: Vec<(&str, usize, usize)> = ohh
        .rounds
        .iter()
        .map(|r| {
            let cards = r.cards.as_ref().map_or(0, Vec::len);
            (r.street.as_str(), cards, r.actions.len())
        })
        .collect();
    assert_eq!(
        streets,
        vec![
            ("Preflop", 0, 6),
            ("Flop", 3, 2),
            ("Turn", 1, 2),
            ("River", 1, 2),
            ("Showdown", 0, 2),
        ]
    );
    assert_eq!(ohh.pots.len(), 1);
    assert_eq!(ohh.pots[0].amount, 180.0);
    let winners: Vec<(u64, f32)> = ohh.pots[0]
        .player_wins
        .iter()
        .map(|w| (ohh.players[w.player_id as usize].seat, w.win_amount))
        .collect();
    assert_eq!(winners, vec![(2, 180.0)]);
}

/// The OHH examples leave an uncalled bet out of the pot, where Winamax
/// counts it in what the bettor collects: reading such a hand gives it back.
#[test]
fn an_uncalled_bet_left_out_of_the_ohh_pot_is_returned_to_the_bettor() {
    let json = r#"{"ohh": {
      "spec_version": "1.4.9", "site_name": "Winamax", "network_name": "Winamax",
      "internal_version": "0.1.0", "tournament": true,
      "game_number": "1-1-1", "start_date_utc": "2023-01-06T13:25:26Z",
      "table_name": "Expresso Nitro(123456789)#0", "game_type": "Holdem",
      "table_size": 3, "currency": "", "dealer_seat": 1,
      "small_blind_amount": 10, "big_blind_amount": 20, "ante_amount": 0,
      "players": [
        {"id": 0, "seat": 1, "name": "A", "starting_stack": 300},
        {"id": 1, "seat": 2, "name": "B", "starting_stack": 300},
        {"id": 2, "seat": 3, "name": "C", "starting_stack": 300}
      ],
      "rounds": [
        {"id": 0, "street": "Preflop", "actions": [
          {"action_number": 0, "player_id": 1, "action": "Post SB", "amount": 10, "is_allin": false},
          {"action_number": 1, "player_id": 2, "action": "Post BB", "amount": 20, "is_allin": false},
          {"action_number": 2, "player_id": 0, "action": "Raise", "amount": 40, "is_allin": false},
          {"action_number": 3, "player_id": 1, "action": "Fold", "amount": 0, "is_allin": false},
          {"action_number": 4, "player_id": 2, "action": "Call", "amount": 20, "is_allin": false}
        ]},
        {"id": 1, "street": "Flop", "cards": ["7c", "2d", "9s"], "actions": [
          {"action_number": 0, "player_id": 2, "action": "Check", "amount": 0, "is_allin": false},
          {"action_number": 1, "player_id": 0, "action": "Bet", "amount": 30, "is_allin": false},
          {"action_number": 2, "player_id": 2, "action": "Fold", "amount": 0, "is_allin": false}
        ]}
      ],
      "pots": [{"number": 0, "amount": 90, "rake": 0, "player_wins": [{"player_id": 0, "win_amount": 90}]}]
    }}"#;
    let ohh = through_ohh_reader(json);
    let hand = from_ohh(&ohh).unwrap();
    let collected: Vec<u32> = hand.players.iter().map(|p| p.collected).collect();
    assert_eq!(collected, vec![120, 0, 0]);
}

#[test]
fn an_ohh_hand_with_fractional_chips_is_rejected() {
    let summary = parse_summary(SUMMARY).unwrap();
    let mut ohh = to_ohh(&fixture_hands()[0], Some(&summary));
    ohh.pots[0].player_wins[0].win_amount = 179.5;
    assert!(from_ohh(&ohh).is_err());
}

fn through_ohh_reader(json: &str) -> HandHistory {
    HandReader::from_reader(Cursor::new(json.as_bytes().to_vec()))
        .next()
        .unwrap()
        .unwrap()
}
