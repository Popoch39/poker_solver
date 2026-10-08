//! The clicker bot plays whole tournaments on the local client, offscreen:
//! every decision is read from a rendered frame, decided by the hero's
//! strategy and clicked through an injector that forwards to the client.

use std::sync::Arc;

use nitro_bot::{Cards, OffscreenReport, ReadError, TableReader, play_offscreen};
use nitro_local_client::HERO;
use nitro_simulator::{
    Card, Decision, PrizeTable, SeatStrategy, SeatView, SimulationConfig, SolverHero, Structure,
    Suit, TrivialBot, Value,
};
use nitro_solver::SolveOptions;
use rand::Rng;

fn config(hero: Arc<dyn SeatStrategy>, games: u64) -> SimulationConfig {
    SimulationConfig {
        prize_table: PrizeTable::for_buy_in(100).unwrap().clone(),
        structure: Structure::expresso_nitro(),
        games,
        seed: 3,
        seats: [
            hero,
            Arc::new(TrivialBot::Random),
            Arc::new(TrivialBot::AlwaysAllIn),
        ],
    }
}

fn all_played(report: &OffscreenReport, games: u64) {
    let wins: u64 = report
        .comparison
        .challenger
        .seats
        .iter()
        .map(|s| s.wins)
        .sum();
    assert_eq!(wins, games, "every tournament is played to its end");
}

#[test]
fn the_bot_plays_whole_tournaments_as_the_solver_s_hero_would() {
    // Coarse solves: the test is about the bot, not the strategy's quality.
    let hero = Arc::new(SolverHero::equilibrium(SolveOptions {
        iterations: 200,
        target_exploitability: Some(0.05),
    }));
    let reader = TableReader::new();

    let report = play_offscreen(&config(hero, 100), |frame| reader.read(frame));

    all_played(&report, 100);
    assert!(report.decisions > 300, "{report:?}");
    assert_eq!(report.unplayed, 0, "{report:?}");
    assert_eq!(report.concordant, report.decisions, "{report:?}");
    assert_eq!(report.concordance(), 1.0);
    let (low, high) = report.comparison.gain.win_rate_ci95;
    assert!(low <= 0.0 && 0.0 <= high, "{:?}", report.comparison.gain);
}

/// Goes all-in with a pair, folds anything else.
struct PairPusher;

impl SeatStrategy for PairPusher {
    fn name(&self) -> &str {
        "pair-pusher"
    }

    fn decide(&self, view: &SeatView, _: &mut dyn Rng) -> Decision {
        let [a, b] = view.hole_cards;
        if a.value == b.value {
            Decision::AllIn
        } else {
            Decision::Fold
        }
    }
}

#[test]
fn a_misread_decision_is_counted_against_the_concordance() {
    let reader = TableReader::new();
    let aces = [Suit::Spade, Suit::Heart].map(|suit| Card::new(Value::Ace, suit));
    // Sees aces in every hand.
    let report = play_offscreen(&config(Arc::new(PairPusher), 20), |frame| {
        let mut state = reader.read(frame)?;
        if matches!(state.seats[HERO].cards, Cards::Shown(_)) {
            state.seats[HERO].cards = Cards::Shown(aces);
        }
        Ok(state)
    });

    all_played(&report, 20);
    assert!(report.concordant < report.decisions, "{report:?}");
    assert!(report.concordance() > 0.0, "pairs are read right");
    assert_eq!(report.unplayed, 0);
    let discordance = report.first_discordance.expect("a discordance is kept");
    assert_eq!(discordance.game, 0, "every game deals non-pairs");
    // All-in, or the call it becomes when all-in is not a button.
    assert!(
        matches!(discordance.clicked, Some(Decision::AllIn | Decision::Call)),
        "{discordance:?}"
    );
    assert_ne!(discordance.clicked, Some(discordance.expected));
}

#[test]
fn a_decision_the_bot_cannot_read_is_played_for_it_and_counted() {
    let reader = TableReader::new();
    // Every other hand is unreadable.
    let report = play_offscreen(&config(Arc::new(TrivialBot::Random), 10), |frame| {
        let state = reader.read(frame)?;
        if state.hand_number % 2 == 0 {
            return Err(ReadError::Unreadable("anything".to_owned()));
        }
        Ok(state)
    });

    all_played(&report, 10);
    assert!(report.unplayed > 0, "{report:?}");
    assert_eq!(report.concordant + report.unplayed, report.decisions);
    let discordance = report.first_discordance.expect("a discordance is kept");
    assert_eq!(discordance.clicked, None);
}
