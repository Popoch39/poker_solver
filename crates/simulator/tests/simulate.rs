//! Many games through `simulate`: known outcomes, report and reproducibility.

use std::sync::Arc;

use nitro_simulator::{
    NitroGame, PrizeTable, Report, SeatStrategy, SimulationConfig, Structure, TrivialBot, Verdict,
    compare, compare_with, simulate,
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
fn a_seat_is_above_or_below_the_break_even_only_when_its_interval_says_so() {
    use TrivialBot::*;
    let report = simulate(&config([AlwaysAllIn, AlwaysFold, Random], 500, 8));
    assert_eq!(report.seats[0].verdict, Verdict::AboveBreakEven);
    assert_eq!(report.seats[1].verdict, Verdict::BelowBreakEven);
    let even = simulate(&config([Random; 3], 300, 9));
    let undecided = even
        .seats
        .iter()
        .filter(|s| s.verdict == Verdict::Undecided)
        .count();
    assert!(undecided >= 1, "{even:?}");
    for seat in &even.seats {
        let (low, high) = seat.win_rate_ci95;
        let expected = match () {
            _ if low > even.break_even_win_rate => Verdict::AboveBreakEven,
            _ if high < even.break_even_win_rate => Verdict::BelowBreakEven,
            _ => Verdict::Undecided,
        };
        assert_eq!(seat.verdict, expected, "{seat:?}");
    }
}

#[test]
fn two_strategies_compared_on_the_same_games_differ_by_their_paired_gain() {
    use TrivialBot::*;
    let baseline = config([Random, AlwaysAllIn, AlwaysAllIn], 2_000, 10);
    let challenger = config([AlwaysAllIn; 3], 2_000, 10);

    let comparison = compare(&baseline, &challenger, 0);

    assert_eq!(comparison.baseline, simulate(&baseline));
    assert_eq!(comparison.challenger, simulate(&challenger));
    let (seat, base) = (
        &comparison.challenger.seats[0],
        &comparison.baseline.seats[0],
    );
    let gain = comparison.gain;
    assert!((gain.win_rate - (seat.win_rate - base.win_rate)).abs() < 1e-12);
    assert!((gain.roi - (seat.roi - base.roi)).abs() < 1e-12);
    for (value, (low, high)) in [
        (gain.win_rate, gain.win_rate_ci95),
        (gain.roi, gain.roi_ci95),
    ] {
        assert!(low < value && value < high, "{gain:?}");
    }
    // The same deals until the play differs: pairing the games narrows the
    // interval of independent samples, whose half-widths add in quadrature.
    let half = |(low, high): (f64, f64)| (high - low) / 2.0;
    let independent = half(seat.roi_ci95).hypot(half(base.roi_ci95));
    assert!(half(gain.roi_ci95) < independent, "{gain:?}");

    let same = compare(&baseline, &baseline, 0).gain;
    assert_eq!((same.win_rate, same.win_rate_ci95), (0.0, (0.0, 0.0)));
}

#[test]
fn games_played_elsewhere_are_the_same_games_as_the_simulators() {
    use TrivialBot::*;
    let baseline = config([Random, AlwaysAllIn, AlwaysAllIn], 500, 12);
    let challenger = config([AlwaysAllIn; 3], 500, 12);
    // A client would play game `index` from `seed`, its hero seat answered
    // from outside; here the challenger's strategies play it.
    let played = compare_with(&baseline, 0, |_index, seed| {
        NitroGame::new(challenger.structure.clone(), challenger.seats.clone(), seed).play_to_end()
    });

    let direct = compare(&baseline, &challenger, 0);
    assert_eq!(played.baseline, direct.baseline);
    assert_eq!(played.gain, direct.gain);
    for (played, direct) in played.challenger.seats.iter().zip(&direct.challenger.seats) {
        assert_eq!((played.wins, played.roi), (direct.wins, direct.roi));
    }
    let same = compare_with(&baseline, 0, |_, seed| {
        NitroGame::new(baseline.structure.clone(), baseline.seats.clone(), seed).play_to_end()
    });
    assert_eq!(same.challenger, simulate(&baseline));
    assert_eq!(same.gain.win_rate_ci95, (0.0, 0.0));
}

#[test]
#[should_panic(expected = "the same games")]
fn strategies_are_only_compared_on_the_same_games() {
    let baseline = config([TrivialBot::Random; 3], 100, 1);
    compare(&baseline, &config([TrivialBot::Random; 3], 100, 2), 0);
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
