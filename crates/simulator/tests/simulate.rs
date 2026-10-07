//! Many games through `simulate`: known outcomes, report and reproducibility.

use std::sync::Arc;

use nitro_simulator::{
    PrizeTable, Report, SeatStrategy, SimulationConfig, Structure, TrivialBot, simulate,
};

fn config(bots: [TrivialBot; 3], games: u64, seed: u64) -> SimulationConfig {
    SimulationConfig {
        prize_table: PrizeTable::for_buy_in(100).unwrap().clone(),
        structure: Structure::expresso_nitro(),
        games,
        seed,
        seats: bots.map(|b| Arc::new(b) as Arc<dyn SeatStrategy>),
    }
}

fn on_threads(threads: usize, config: &SimulationConfig) -> Report {
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .unwrap()
        .install(|| simulate(config))
}

#[test]
fn always_all_in_beats_two_always_fold() {
    use TrivialBot::*;
    let report = simulate(&config([AlwaysAllIn, AlwaysFold, AlwaysFold], 300, 1));
    assert_eq!(report.games, 300);
    assert_eq!(report.seats[0].name, "always-all-in");
    assert!(report.seats[0].win_rate > 0.99, "{report:?}");
    assert!(report.seats[0].roi > 1.0, "{report:?}");
    assert!(report.seats[1].roi < -0.9, "{report:?}");
}

#[test]
fn three_identical_strategies_win_a_third_each() {
    let report = simulate(&config([TrivialBot::Random; 3], 3_000, 2));
    let mut total_wins = 0;
    for seat in &report.seats {
        let (low, high) = seat.win_rate_ci95;
        assert!(low < 1.0 / 3.0 && 1.0 / 3.0 < high, "{seat:?}");
        assert!(low < seat.win_rate && seat.win_rate < high, "{seat:?}");
        total_wins += seat.wins;
    }
    assert_eq!(total_wins, 3_000);
}

#[test]
fn the_report_compares_the_win_rate_to_the_break_even() {
    let report = simulate(&config([TrivialBot::Random; 3], 200, 3));
    assert_eq!(report.buy_in_cents, 100);
    assert_eq!(report.rake_percent, 7);
    assert!((report.break_even_win_rate - 0.3584).abs() < 1e-4);
    for seat in &report.seats {
        let gap = seat.win_rate - report.break_even_win_rate;
        assert!((seat.break_even_gap - gap).abs() < 1e-12);
        let (low, high) = seat.roi_ci95;
        assert!(low < seat.roi && seat.roi < high, "{seat:?}");
    }
}

#[test]
fn the_same_seed_reproduces_the_same_report() {
    let config = config([TrivialBot::Random; 3], 500, 4);
    assert_eq!(simulate(&config), simulate(&config));
    let other = SimulationConfig {
        seed: 5,
        ..config.clone()
    };
    assert_ne!(simulate(&config), simulate(&other));
}

#[test]
fn the_report_does_not_depend_on_the_thread_count() {
    let config = config([TrivialBot::Random; 3], 500, 6);
    let one = on_threads(1, &config);
    assert_eq!(one, on_threads(3, &config));
    assert_eq!(one, on_threads(8, &config));
}
