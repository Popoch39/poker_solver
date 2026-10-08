//! One Expresso Nitro played hand by hand through `NitroGame`.

use std::sync::{Arc, Mutex};

use nitro_simulator::{
    Decision, NitroGame, Position, SeatStrategy, SeatView, Street, Structure, TrivialBot,
};
use rand::Rng;

/// Plays like `bot` and keeps every view it was shown.
struct Recorder {
    bot: TrivialBot,
    views: Mutex<Vec<SeatView>>,
}

impl Recorder {
    fn new(bot: TrivialBot) -> Arc<Self> {
        Arc::new(Self {
            bot,
            views: Mutex::new(Vec::new()),
        })
    }

    fn views(&self) -> Vec<SeatView> {
        self.views.lock().unwrap().clone()
    }
}

impl SeatStrategy for Recorder {
    fn name(&self) -> &str {
        self.bot.name()
    }

    fn decide(&self, view: &SeatView, rng: &mut dyn Rng) -> Decision {
        self.views.lock().unwrap().push(view.clone());
        self.bot.decide(view, rng)
    }
}

fn seats(bots: [TrivialBot; 3]) -> [Arc<dyn SeatStrategy>; 3] {
    bots.map(|b| Arc::new(b) as Arc<dyn SeatStrategy>)
}

#[test]
fn the_game_starts_three_handed_at_10_20_with_300_chips() {
    let recorder = Recorder::new(TrivialBot::AlwaysFold);
    let seats: [Arc<dyn SeatStrategy>; 3] = [recorder.clone(), recorder.clone(), recorder.clone()];
    let mut game = NitroGame::new(Structure::expresso_nitro(), seats, 1);
    assert_eq!(game.stacks(), [300.0; 3]);

    let hand = game.play_hand().expect("the game is not over");
    assert_eq!(hand.number, 1);
    assert_eq!(hand.level, 1);
    assert_eq!((hand.small_blind, hand.big_blind), (10.0, 20.0));

    // Everyone folds: the button and then the small blind fold to the big
    // blind, who wins the small blind.
    let views = recorder.views();
    assert_eq!(views.len(), 2);
    assert_eq!(views[0].position, Position::Button);
    assert_eq!(views[0].seat, hand.button);
    assert_eq!(views[0].to_call, 20.0);
    assert_eq!(views[0].street, Street::Preflop);
    assert_eq!(views[0].players.len(), 3);
    assert_eq!(views[1].position, Position::SmallBlind);
    assert_eq!(views[1].to_call, 10.0);
    let big_blind = (hand.button + 2) % 3;
    let mut expected = [300.0; 3];
    expected[(hand.button + 1) % 3] = 290.0;
    expected[big_blind] = 310.0;
    assert_eq!(game.stacks(), expected);
}

#[test]
fn blinds_rise_after_the_configured_number_of_hands() {
    let mut structure = Structure::expresso_nitro();
    structure.hands_per_level = 4;
    let mut game = NitroGame::new(structure, seats([TrivialBot::AlwaysFold; 3]), 2);
    let blinds: Vec<(u32, f32, f32)> = (0..9)
        .map(|_| {
            let hand = game.play_hand().unwrap();
            (hand.level, hand.small_blind, hand.big_blind)
        })
        .collect();
    assert_eq!(
        blinds,
        [
            (1, 10.0, 20.0),
            (1, 10.0, 20.0),
            (1, 10.0, 20.0),
            (1, 10.0, 20.0),
            (2, 15.0, 30.0),
            (2, 15.0, 30.0),
            (2, 15.0, 30.0),
            (2, 15.0, 30.0),
            (3, 20.0, 40.0),
        ]
    );
    assert_eq!(game.stacks().iter().sum::<f32>(), 900.0);
}

#[test]
fn heads_up_the_button_posts_the_small_blind_and_acts_first() {
    let mut heads_up_hands = 0;
    for seed in 0..10 {
        let recorder = Recorder::new(TrivialBot::Random);
        let seats: [Arc<dyn SeatStrategy>; 3] =
            [recorder.clone(), recorder.clone(), recorder.clone()];
        let mut game = NitroGame::new(Structure::expresso_nitro(), seats, seed);
        let mut hands = Vec::new();
        while let Some(hand) = game.play_hand() {
            hands.push(hand);
        }
        let views = recorder.views();
        for hand in hands {
            let Some(first) = views.iter().find(|v| v.hand_number == hand.number) else {
                continue; // everyone was all-in from the blinds
            };
            if first.players.len() != 2 {
                continue;
            }
            heads_up_hands += 1;
            assert_eq!(first.seat, hand.button, "hand {}", hand.number);
            assert_eq!(first.position, Position::SmallBlind);
            let me = first.players.iter().find(|p| p.seat == first.seat).unwrap();
            let other = first.players.iter().find(|p| p.seat != first.seat).unwrap();
            assert_eq!(other.position, Position::BigBlind);
            assert!(
                me.street_bet < other.street_bet || other.all_in,
                "{first:?}"
            );
        }
    }
    assert!(heads_up_hands > 10, "only {heads_up_hands} heads-up hands");
}

#[test]
fn split_pots_leave_whole_chips() {
    let mut shared = 0;
    // A tie for an odd pot is rare: it takes thousands of games to meet one.
    for seed in 0..5_000 {
        let mut game = NitroGame::new(
            Structure::expresso_nitro(),
            seats([TrivialBot::Random; 3]),
            seed,
        );
        while let Some(hand) = game.play_hand() {
            let view = game.table_view(0);
            shared += usize::from(view.seats.iter().filter(|s| s.won > 0.0).count() > 1);
            for seat in view.seats {
                assert_eq!(seat.won.fract(), 0.0, "seed {seed}: {view:?}");
            }
            for stack in hand.stacks {
                assert_eq!(stack.fract(), 0.0, "seed {seed}: {hand:?}");
            }
            assert_eq!(hand.stacks.iter().sum::<f32>(), 900.0);
        }
    }
    assert!(shared > 100, "only {shared} pots shared");
}

#[test]
fn the_game_ends_when_one_seat_has_all_the_chips() {
    let mut game = NitroGame::new(
        Structure::expresso_nitro(),
        seats([TrivialBot::Random; 3]),
        3,
    );
    let places = game.play_to_end();
    let mut sorted = places;
    sorted.sort();
    assert_eq!(sorted, [1, 2, 3]);
    let winner = places.iter().position(|&p| p == 1).unwrap();
    assert_eq!(game.stacks()[winner], 900.0);
    assert!(game.play_hand().is_none());
}
