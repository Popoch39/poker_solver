//! Simulated hands written as hand histories, and read back by the parser
//! through Open Hand History.

use std::io::Cursor;
use std::sync::Arc;

use nitro_hh::ohh::{HandReader, write_hand};
use nitro_hh::{ActionKind, Hand, Street, from_ohh, to_ohh};
use nitro_simulator::{
    PrizeTable, SeatStrategy, SimulationConfig, Structure, TrivialBot, hand_histories,
};

fn config(seed: u64) -> SimulationConfig {
    SimulationConfig {
        prize_table: PrizeTable::for_buy_in(100).unwrap().clone(),
        structure: Structure::expresso_nitro(),
        games: 1_000,
        seed,
        seats: [
            TrivialBot::Random,
            TrivialBot::AlwaysAllIn,
            TrivialBot::Random,
        ]
        .map(|b| Arc::new(b) as Arc<dyn SeatStrategy>),
    }
}

/// Chips each seat put in during the hand.
fn put_in(hand: &Hand, seat: u8) -> u32 {
    let mut total = 0;
    let mut on_street = 0;
    let mut street = Street::Preflop;
    for action in hand.actions.iter().filter(|a| a.seat == seat) {
        if action.street != street {
            street = action.street;
            on_street = 0;
        }
        let added = match action.kind {
            ActionKind::SmallBlind(n)
            | ActionKind::BigBlind(n)
            | ActionKind::Call(n)
            | ActionKind::Bet(n) => n,
            ActionKind::Raise { to } => to - on_street,
            ActionKind::Fold | ActionKind::Check => 0,
        };
        on_street += added;
        total += added;
    }
    total
}

#[test]
fn a_game_is_written_hand_by_hand_with_chips_that_add_up() {
    let config = config(1);
    let mut shared_pots = 0;
    for game in 0..200 {
        let hands = hand_histories(&config, game, Some(0));
        assert!(!hands.is_empty());
        let mut stacks = [300u32; 3];
        for (number, hand) in hands.iter().enumerate() {
            assert!(hand.level.is_some());
            assert_eq!(hand.table_size, 3);
            // Seats are numbered from 1; busted players are not dealt in.
            let dealt: Vec<u8> = hand.players.iter().map(|p| p.seat).collect();
            let alive: Vec<u8> = (1..=3).filter(|&s| stacks[s as usize - 1] > 0).collect();
            assert_eq!(dealt, alive, "game {game}, hand {}", number + 1);
            for player in &hand.players {
                let seat = player.seat;
                assert_eq!(player.stack, stacks[seat as usize - 1]);
                stacks[seat as usize - 1] = player.stack - put_in(hand, seat) + player.collected;
            }
            assert_eq!(stacks.iter().sum::<u32>(), 900);
            shared_pots += usize::from(hand.players.iter().filter(|p| p.collected > 0).count() > 1);
            // The hero's cards are always known.
            if let Some(hero) = hand.hero {
                assert_eq!(hero, 1);
                let hero = hand.players.iter().find(|p| p.seat == hero).unwrap();
                assert!(hero.cards.is_some());
            }
        }
        // The last hand leaves one player with every chip.
        assert_eq!(stacks.iter().filter(|&&s| s > 0).count(), 1, "game {game}");
    }
    assert!(shared_pots > 0);
}

#[test]
fn simulated_hands_round_trip_through_open_hand_history() {
    let config = config(2);
    // Enough games to split an odd pot.
    let hands: Vec<Hand> = (0..3_000)
        .flat_map(|game| hand_histories(&config, game, None))
        .collect();
    let mut file = Vec::new();
    for hand in &hands {
        write_hand(&mut file, to_ohh(hand, None)).unwrap();
    }

    let read_back: Vec<Hand> = HandReader::from_reader(Cursor::new(file))
        .map(|history| from_ohh(&history.unwrap()).unwrap())
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
    // Showdowns are written with the cards shown.
    assert!(expected.iter().any(|h| h.players.iter().any(|p| p.showed)));
}

#[test]
fn hand_histories_replay_the_simulated_game() {
    let config = config(3);
    assert_eq!(
        hand_histories(&config, 17, Some(1)),
        hand_histories(&config, 17, Some(1))
    );
    assert_ne!(
        hand_histories(&config, 17, None),
        hand_histories(&config, 18, None)
    );
}
