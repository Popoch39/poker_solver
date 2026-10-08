//! Parsing Winamax tournament summary files.

use std::time::Duration;

use chrono::{TimeZone, Utc};
use nitro_hh::{BuyIn, Euros, Level, parse_summary};

const FIXTURE: &str = include_str!("fixtures/nitro-618031930_summary.txt");

fn one_euro() -> BuyIn {
    BuyIn {
        prize: Euros::from_cents(93),
        rake: Euros::from_cents(7),
    }
}

#[test]
fn the_summary_of_a_lost_tournament_is_parsed() {
    let summary = parse_summary(FIXTURE).unwrap();
    assert_eq!(summary.tournament_id, "618031930");
    assert_eq!(summary.tournament_name, "Expresso Nitro");
    assert_eq!(summary.player, "hero");
    assert_eq!(summary.buy_in, one_euro());
    assert_eq!(summary.buy_in.total(), Euros::from_cents(100));
    assert_eq!(summary.registered_players, 3);
    assert_eq!(summary.prize_pool, Euros::from_cents(200));
    assert_eq!(
        summary.started_at,
        Utc.with_ymd_and_hms(2023, 1, 2, 11, 45, 56).unwrap()
    );
    assert_eq!(summary.played, Duration::from_secs(5 * 60 + 40));
    assert_eq!(summary.place, 2);
    assert_eq!(summary.winnings, Euros::from_cents(0));

    let minute = Duration::from_secs(60);
    assert_eq!(summary.levels.len(), 33);
    assert_eq!(
        summary.levels[0],
        Level {
            small_blind: 10,
            big_blind: 20,
            ante: 0,
            duration: minute,
        }
    );
    assert_eq!(
        summary.levels[32],
        Level {
            small_blind: 25000,
            big_blind: 50000,
            ante: 0,
            duration: minute,
        }
    );
    assert!(
        summary
            .levels
            .iter()
            .all(|l| l.ante == 0 && l.duration == minute)
    );
}

/// The format Winamax used from February 2023: `Speed` and `Flight ID` lines,
/// a doubled `Levels : ` prefix, a trailing space and a `You won` line.
#[test]
fn the_newer_summary_format_of_a_won_tournament_is_parsed() {
    let text = "Winamax Poker - Tournament summary : Expresso Nitro(632649880)\n\
                Player : hero\n\
                Buy-In : 1.86€ + 0.14€\n\
                Registered players : 3\n\
                Mode : sng\n\
                Type : sitngo\n\
                Speed : turbo\n\
                Flight ID : 0\n\
                Levels : Levels : [10-20:0:60:holdem-no-limit,15-30:0:60:holdem-no-limit]\n\
                Prizepool : 6€\n\
                Tournament started 2023/02/12 14:34:40 UTC\n\
                You played 48s \n\
                You finished in 1st place\n\
                You won 6€\n\
                \n";
    let summary = parse_summary(text).unwrap();
    assert_eq!(summary.tournament_id, "632649880");
    assert_eq!(summary.buy_in.total(), Euros::from_cents(200));
    assert_eq!(summary.levels.len(), 2);
    assert_eq!(summary.prize_pool, Euros::from_cents(600));
    assert_eq!(summary.played, Duration::from_secs(48));
    assert_eq!(summary.place, 1);
    assert_eq!(summary.winnings, Euros::from_cents(600));
}

#[test]
fn a_summary_with_a_malformed_line_is_rejected_with_its_line_number() {
    let damaged = FIXTURE.replace("You finished in 2nd place", "You finished in second place");
    let error = parse_summary(&damaged).unwrap_err();
    assert_eq!(error.line, 11, "{error}");
}
