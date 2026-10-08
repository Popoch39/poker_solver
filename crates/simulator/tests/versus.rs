//! Slow tests (thousands of games, hundreds of solves): the solver's hero
//! in whole games. They live in their own test binary so the fast ones can
//! run without them: `cargo test -p nitro-simulator --test versus` runs only
//! these.

#[path = "support/populations.rs"]
mod populations;

use std::sync::Arc;

use nitro_simulator::{PopulationBot, SeatStrategy, SolverHero, Structure, compare, simulate};
use nitro_solver::SolveOptions;
use populations::{Station, config, population_of};

/// A simulation cannot tell strategies 1 mBB per hand apart.
fn options() -> SolveOptions {
    SolveOptions {
        iterations: 1_000,
        target_exploitability: Some(1e-3),
    }
}

#[test]
fn the_equilibrium_against_two_equilibrium_bots_wins_a_third_of_the_games() {
    let hero = Arc::new(SolverHero::equilibrium(options())) as Arc<dyn SeatStrategy>;
    let config = config(
        Structure::expresso_nitro(),
        [0; 3].map(|_| hero.clone()),
        3_000,
        11,
    );

    let report = simulate(&config);

    for seat in &report.seats {
        let (low, high) = seat.win_rate_ci95;
        assert!(low < 1.0 / 3.0 && 1.0 / 3.0 < high, "{seat:?}");
    }
}

#[test]
fn against_a_population_that_calls_too_often_the_exploit_wins_more_than_the_equilibrium() {
    // Whole Expresso Nitro, from 15 BB. The population calls every push and
    // never opens, so the two heroes only part ways on their own pushes: the
    // pairing then cancels most of the luck, and 10 000 games show the
    // exploit's edge (about +0.4 pt). Against stations that also open, the
    // edge is lost in the noise of their pushes (ADR 0007).
    let structure = Structure::expresso_nitro();
    let stations = [0; 3].map(|_| Arc::new(Station { opens: 0.0 }) as Arc<dyn SeatStrategy>);
    let model = Arc::new(population_of(structure.clone(), stations, 2_000, false));
    let bot = Arc::new(PopulationBot::new(Arc::clone(&model))) as Arc<dyn SeatStrategy>;
    let against = |hero: SolverHero| {
        let seats = [
            Arc::new(hero) as Arc<dyn SeatStrategy>,
            bot.clone(),
            bot.clone(),
        ];
        config(structure.clone(), seats, 10_000, 12)
    };

    let comparison = compare(
        &against(SolverHero::equilibrium(options())),
        &against(SolverHero::exploit(model, 20, options())),
        0,
    );

    let gain = comparison.gain;
    assert!(gain.win_rate_ci95.0 > 0.0, "{gain:?}");
}
