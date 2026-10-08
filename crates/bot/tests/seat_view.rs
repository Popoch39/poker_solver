//! What the bot reads becomes the view a seat strategy decides from: the
//! same view the simulator hands its own seats.

use std::sync::Arc;
use std::time::Duration;

use nitro_bot::{TableReader, TableState};
use nitro_local_client::{ClientConfig, HERO, LAYOUT, LocalClient, Next};
use nitro_simulator::{SeatStrategy, Structure, TrivialBot};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

#[test]
fn the_state_read_while_the_hero_acts_is_the_simulator_s_view_of_its_decision() {
    let reader = TableReader::new();
    let mut clicks = StdRng::seed_from_u64(3);
    let mut decisions = 0;
    let mut others = 0;
    for seed in 0..10 {
        let mut client = LocalClient::new(ClientConfig {
            structure: Structure::expresso_nitro(),
            seed,
            bots: [TrivialBot::Random, TrivialBot::AlwaysAllIn]
                .map(|b| Arc::new(b) as Arc<dyn SeatStrategy>),
            bot_delay: Duration::ZERO,
            hand_over_delay: Duration::ZERO,
        });
        loop {
            let read = reader.read(&client.frame()).expect("a readable frame");
            assert_eq!(read.seat_view().as_ref(), client.hero_decision());
            match client.advance() {
                Next::After(_) => others += 1,
                Next::GameOver => break,
                Next::WaitForClick => {
                    let state = TableState::drawn(&client.view());
                    let view = state.seat_view().expect("the hero is to act");
                    assert_eq!(Some(&view), client.hero_decision());
                    assert_eq!(view.seat, HERO);
                    decisions += 1;
                    let legal = client.view().legal;
                    let (x, y) = LAYOUT
                        .button(legal[clicks.random_range(0..legal.len())])
                        .center();
                    client.click(x, y).unwrap();
                }
            }
        }
    }
    assert!(decisions > 30 && others > 30, "{decisions} {others}");
}
