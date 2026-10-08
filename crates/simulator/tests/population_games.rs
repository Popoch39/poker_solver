//! Whole games between population bots, in a test binary of their own: the
//! first hand a bot ranks is then ranked inside the simulation's games.

#[path = "support/populations.rs"]
mod populations;

use std::sync::Arc;

use nitro_simulator::{PopulationBot, SeatStrategy, Structure, simulate};
use populations::{Station, config, population_of};

#[test]
fn three_population_bots_win_a_third_each() {
    let model = Arc::new(population_of(
        Structure::expresso_nitro(),
        [0; 3].map(|_| Arc::new(Station { opens: 0.2 }) as Arc<dyn SeatStrategy>),
        300,
        false,
    ));
    let bot = Arc::new(PopulationBot::new(model)) as Arc<dyn SeatStrategy>;
    let config = config(
        Structure::expresso_nitro(),
        [0; 3].map(|_| bot.clone()),
        3_000,
        5,
    );

    let report = simulate(&config);

    for seat in &report.seats {
        let (low, high) = seat.win_rate_ci95;
        assert!(low < 1.0 / 3.0 && 1.0 / 3.0 < high, "{seat:?}");
    }
}
