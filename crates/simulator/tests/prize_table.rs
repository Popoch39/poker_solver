//! The official multiplier tables: break-even, rake and payouts.

use std::collections::HashMap;

use nitro_simulator::PrizeTable;
use rand::SeedableRng;
use rand::rngs::StdRng;

const ALL_BUY_INS_CENTS: [u32; 11] = [25, 50, 100, 200, 500, 1000, 2500, 5000, 10000, 25000, 50000];

#[test]
fn every_official_buy_in_has_a_table() {
    for cents in ALL_BUY_INS_CENTS {
        let table = PrizeTable::for_buy_in(cents).expect("official buy-in");
        assert_eq!(table.buy_in_cents(), cents);
    }
    assert!(PrizeTable::for_buy_in(300).is_none());
}

#[test]
fn rake_is_8_percent_at_the_micro_stakes_and_7_percent_above() {
    assert_eq!(PrizeTable::for_buy_in(25).unwrap().rake_percent(), 8);
    assert_eq!(PrizeTable::for_buy_in(50).unwrap().rake_percent(), 8);
    for cents in [100, 200, 500, 1000, 50000] {
        assert_eq!(PrizeTable::for_buy_in(cents).unwrap().rake_percent(), 7);
    }
}

#[test]
fn break_even_win_rate_matches_the_spec() {
    // Spec #1: ≈ 35.8 % from 1 to 5 €, ≈ 36.2 % at 0.25 and 0.50 €
    // (1 / 2.79 and 1 / 2.76).
    for cents in [100, 200, 500] {
        let be = PrizeTable::for_buy_in(cents).unwrap().break_even_win_rate();
        assert!((be - 0.358_42).abs() < 1e-4, "{cents}: {be}");
    }
    for cents in [25, 50] {
        let be = PrizeTable::for_buy_in(cents).unwrap().break_even_win_rate();
        assert!((be - 0.362_32).abs() < 1e-4, "{cents}: {be}");
    }
}

#[test]
fn the_expected_pool_is_three_buy_ins_minus_the_rake() {
    // Winamax takes the rake only through the multiplier distribution, so a
    // table with a typo in one of its counts breaks this identity.
    for cents in ALL_BUY_INS_CENTS {
        let table = PrizeTable::for_buy_in(cents).unwrap();
        let expected = 3.0 * (1.0 - f64::from(table.rake_percent()) / 100.0);
        assert!(
            (table.expected_multiplier() - expected).abs() < 1e-9,
            "{cents}: {}",
            table.expected_multiplier()
        );
    }
}

#[test]
fn drawn_multipliers_follow_the_official_distribution() {
    let mut rng = StdRng::seed_from_u64(7);
    let draws = 2_000_000_u32;
    for table in [
        PrizeTable::for_buy_in(100).unwrap(),
        PrizeTable::for_buy_in(25_000).unwrap(),
    ] {
        let mut counts: HashMap<u32, u32> = HashMap::new();
        for _ in 0..draws {
            *counts.entry(table.draw(&mut rng)).or_default() += 1;
        }
        let mut seen = 0;
        for (multiplier, p) in table.outcomes() {
            let expected = f64::from(draws) * p;
            let sd = (expected * (1.0 - p)).sqrt();
            let got = counts.get(&multiplier).copied().unwrap_or(0);
            seen += got;
            assert!(
                (f64::from(got) - expected).abs() <= 5.0 * sd + 1.0,
                "x{multiplier}: {got} draws, expected {expected:.1}"
            );
        }
        assert_eq!(seen, draws, "only official multipliers are drawn");
    }
}

#[test]
fn winner_takes_all_up_to_x20_then_80_12_8() {
    let table = PrizeTable::for_buy_in(100).unwrap();
    assert_eq!(table.prize(2, 1), 2.0);
    assert_eq!(table.prize(2, 2), 0.0);
    assert_eq!(table.prize(20, 1), 20.0);
    assert_eq!(table.prize(20, 3), 0.0);
    assert_eq!(table.prize(100, 1), 80.0);
    assert_eq!(table.prize(100, 2), 12.0);
    assert_eq!(table.prize(100, 3), 8.0);
    assert_eq!(table.prize(100_000, 1), 80_000.0);
}
