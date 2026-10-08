//! Decision points built by hand for the strategy tests.

use nitro_simulator::{
    Card, Decision, PlayerView, Position, SeatStrategy, SeatView, Street, Suit, Value,
};
use rand::{Rng, SeedableRng};

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

fn cards_of(text: &str) -> [Card; 2] {
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

/// The decisions a strategy takes at `view` over many draws, as shares of
/// fold, call and all-in.
pub fn shares(strategy: &dyn SeatStrategy, view: &SeatView) -> [f64; 3] {
    let mut rng = rand::rngs::StdRng::seed_from_u64(1);
    let mut counts = [0u32; 3];
    let draws = 2_000;
    for _ in 0..draws {
        let decision = strategy.decide(view, &mut rng as &mut dyn Rng);
        let i = Decision::ALL.iter().position(|&d| d == decision).unwrap();
        counts[i] += 1;
    }
    counts.map(|c| f64::from(c) / f64::from(draws))
}
