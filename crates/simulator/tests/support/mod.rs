//! Decision points and populations built by hand for the strategy tests.

use std::sync::Arc;

use nitro_population::{Players, PopulationModel};
use nitro_simulator::{
    Card, PlayerView, Position, PrizeTable, SeatStrategy, SeatView, SimulationConfig, Street,
    Structure, Suit, Value, hand_histories,
};
use rand::Rng;

/// One player at the table: position, chips behind, chips put in on the
/// street, folded, all-in.
pub type Seat = (Position, f32, f32, bool, bool);

/// The view of the player at `me` (an index of `players`, which is also its
/// table seat), holding `cards` (e.g. `"Ah Kd"`), preflop at 10/20.
pub fn view(me: usize, cards: &str, players: &[Seat]) -> SeatView {
    let players: Vec<PlayerView> = players
        .iter()
        .enumerate()
        .map(
            |(seat, &(position, stack, street_bet, folded, all_in))| PlayerView {
                seat,
                position,
                stack,
                street_bet,
                folded,
                all_in,
            },
        )
        .collect();
    let bet = players.iter().map(|p| p.street_bet).fold(0.0, f32::max);
    let mine = players[me];
    SeatView {
        hand_number: 1,
        level: 1,
        small_blind: 10.0,
        big_blind: 20.0,
        seat: me,
        position: mine.position,
        hole_cards: cards_of(cards),
        board: Vec::new(),
        street: Street::Preflop,
        pot: players.iter().map(|p| p.street_bet).sum(),
        to_call: (bet - mine.street_bet).min(mine.stack),
        players,
    }
}

pub fn cards_of(text: &str) -> [Card; 2] {
    let cards: Vec<Card> = text
        .split(' ')
        .map(|c| Card::try_from(c).expect("a card such as Ah"))
        .collect();
    cards.try_into().expect("two cards")
}

/// A flop view: the same table after the preflop action.
pub fn on_flop(mut view: SeatView) -> SeatView {
    view.street = Street::Flop;
    view.board = vec![
        Card::new(Value::Two, Suit::Club),
        Card::new(Value::Seven, Suit::Diamond),
        Card::new(Value::King, Suit::Heart),
    ];
    for player in &mut view.players {
        player.street_bet = 0.0;
    }
    view
}

pub fn config(seats: [Arc<dyn SeatStrategy>; 3], games: u64, seed: u64) -> SimulationConfig {
    SimulationConfig {
        prize_table: PrizeTable::for_buy_in(100).unwrap().clone(),
        structure: Structure::expresso_nitro(),
        games,
        seed,
        seats,
    }
}

/// The population model of every player of `games` simulated games
/// between `seats`; with `first_hands_only`, of their first hands only
/// (three-handed at 15 BB).
pub fn population_of(
    seats: [Arc<dyn SeatStrategy>; 3],
    games: u64,
    first_hands_only: bool,
) -> PopulationModel {
    let config = config(seats, games, 99);
    let hands: Vec<_> = (0..games)
        .flat_map(|game| {
            let hands = hand_histories(&config, game, None);
            let keep = if first_hands_only { 1 } else { hands.len() };
            hands.into_iter().take(keep)
        })
        .collect();
    PopulationModel::build(&hands, Players::Opponents)
}

/// The decisions a strategy takes at `view` over many draws, as shares of
/// fold, call and all-in.
pub fn shares(strategy: &dyn SeatStrategy, view: &SeatView) -> [f64; 3] {
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(1);
    let mut counts = [0u32; 3];
    let draws = 2_000;
    for _ in 0..draws {
        let decision = strategy.decide(view, &mut rng as &mut dyn Rng);
        counts[nitro_simulator::Decision::ALL
            .iter()
            .position(|&d| d == decision)
            .unwrap()] += 1;
    }
    counts.map(|c| f64::from(c) / f64::from(draws))
}
