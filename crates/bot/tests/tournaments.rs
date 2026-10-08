//! Slow tests (a thousand tournaments each, hundreds of solves): the
//! clicker bot against population bots on the local client, offscreen,
//! compared with the same hero simulated directly. They live in their own
//! test binary so the fast ones can run without them: `cargo test -p
//! nitro-bot --test tournaments -- --nocapture` runs only these and prints
//! the figures.

#[path = "../../simulator/tests/support/populations.rs"]
mod populations;

use std::sync::Arc;

use nitro_bot::{TableReader, play_offscreen};
use nitro_population::PopulationModel;
use nitro_simulator::{PopulationBot, SeatStrategy, SolverHero, Structure};
use nitro_solver::SolveOptions;
use populations::{Station, config, population_of};

fn options() -> SolveOptions {
    SolveOptions {
        iterations: 1_000,
        target_exploitability: Some(1e-3),
    }
}

fn stations() -> Arc<PopulationModel> {
    let stations = [0; 3].map(|_| Arc::new(Station) as Arc<dyn SeatStrategy>);
    Arc::new(population_of(
        Structure::expresso_nitro(),
        stations,
        1_000,
        false,
    ))
}

/// Plays `hero` through the bot and directly, against two bots of
/// `population`, and checks that the bot plays and wins like the hero.
fn bot_plays_like(hero: SolverHero, population: Arc<PopulationModel>, seed: u64) {
    let name = hero.name().to_owned();
    let bot = Arc::new(PopulationBot::new(population)) as Arc<dyn SeatStrategy>;
    let seats = [Arc::new(hero), bot.clone(), bot];
    let config = config(Structure::expresso_nitro(), seats, 1_000, seed);
    let reader = TableReader::new();

    let report = play_offscreen(&config, |frame| reader.read(frame));

    let comparison = &report.comparison;
    let (bot, hero) = (
        &comparison.challenger.seats[0],
        &comparison.baseline.seats[0],
    );
    eprintln!(
        "{name}: {} decisions, {} concordant ({:.2} %), {} unplayed",
        report.decisions,
        report.concordant,
        report.concordance() * 100.0,
        report.unplayed
    );
    eprintln!(
        "{name}: bot {:.1} % {:?}, simulated {:.1} % {:?}, gain {:+.1} pt {:?}",
        bot.win_rate * 100.0,
        bot.win_rate_ci95,
        hero.win_rate * 100.0,
        hero.win_rate_ci95,
        comparison.gain.win_rate * 100.0,
        comparison.gain.win_rate_ci95,
    );
    assert!(report.decisions > 3_000, "{report:?}");
    // The reader makes no mistake on the client's frames (#13): every
    // decision is the hero's.
    assert_eq!(report.concordant, report.decisions, "{report:?}");
    let (low, high) = comparison.gain.win_rate_ci95;
    assert!(low <= 0.0 && 0.0 <= high, "{:?}", comparison.gain);
    let (low, high) = hero.win_rate_ci95;
    assert!(
        low < bot.win_rate && bot.win_rate < high,
        "{bot:?} {hero:?}"
    );
}

#[test]
fn the_bot_s_equilibrium_wins_like_the_equilibrium_simulated_directly() {
    bot_plays_like(SolverHero::equilibrium(options()), stations(), 21);
}

#[test]
fn the_bot_s_exploit_wins_like_the_exploit_simulated_directly() {
    let population = stations();
    let hero = SolverHero::exploit(Arc::clone(&population), 20, options());
    bot_plays_like(hero, population, 22);
}
