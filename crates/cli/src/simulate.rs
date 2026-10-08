//! `nitro simulate`: many games between bots, summed up as a report.

use std::process::ExitCode;
use std::sync::Arc;

use nitro_simulator::{
    PrizeTable, Report, SeatStrategy, SimulationConfig, Structure, TrivialBot, simulate,
};

#[derive(clap::Args)]
pub struct Args {
    /// Buy-in in euros: 0.25, 0.50, 1, 2, 5, 10, 25, 50, 100, 250 or 500.
    #[arg(long, value_parser = parse_buy_in)]
    buy_in: &'static PrizeTable,
    /// Number of games.
    #[arg(long, default_value_t = 10_000)]
    games: u64,
    /// Random seed: the same seed gives the same report.
    #[arg(long, default_value_t = 0)]
    seed: u64,
    /// Bots of the three seats: always-all-in, always-fold or random.
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "BOT,BOT,BOT",
        required = true
    )]
    seats: Vec<TrivialBot>,
    /// Hands per one-minute level while three players are left.
    #[arg(long, default_value_t = Structure::expresso_nitro().hands_per_level)]
    hands_per_level: u32,
    /// Hands per one-minute level once heads-up.
    #[arg(long, default_value_t = Structure::expresso_nitro().hands_per_level_heads_up)]
    hands_per_level_heads_up: u32,
}

pub fn parse_buy_in(euros: &str) -> Result<&'static PrizeTable, String> {
    let cents = euros
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .map(|e| (e * 100.0).round())
        .filter(|&c| c > 0.0 && c <= f64::from(u32::MAX));
    cents
        .and_then(|c| PrizeTable::for_buy_in(c as u32))
        .ok_or_else(|| {
            let known: Vec<String> = PrizeTable::official_tables()
                .iter()
                .map(|t| euros_label(t.buy_in_cents()))
                .collect();
            format!(
                "no Expresso buy-in of {euros} € (expected {})",
                known.join(", ")
            )
        })
}

pub fn euros_label(cents: u32) -> String {
    if cents.is_multiple_of(100) {
        format!("{}", cents / 100)
    } else {
        format!("{}.{:02}", cents / 100, cents % 100)
    }
}

pub fn run(args: &Args) -> ExitCode {
    let seats: [TrivialBot; 3] = match args.seats[..] {
        [a, b, c] => [a, b, c],
        _ => {
            eprintln!("error: --seats takes three bots, got {}", args.seats.len());
            return ExitCode::from(2);
        }
    };
    if args.games == 0 || args.hands_per_level == 0 || args.hands_per_level_heads_up == 0 {
        eprintln!("error: --games and the hands per level must be at least 1");
        return ExitCode::from(2);
    }
    let structure = Structure {
        hands_per_level: args.hands_per_level,
        hands_per_level_heads_up: args.hands_per_level_heads_up,
        ..Structure::expresso_nitro()
    };
    let config = SimulationConfig {
        prize_table: args.buy_in.clone(),
        structure,
        games: args.games,
        seed: args.seed,
        seats: seats.map(|bot| Arc::new(bot) as Arc<dyn SeatStrategy>),
    };
    print_report(args, &simulate(&config));
    ExitCode::SUCCESS
}

fn print_report(args: &Args, report: &Report) {
    println!(
        "Expresso Nitro {} € (rake {} %), {} games, seed {}, {} hands per level ({} heads-up)",
        euros_label(report.buy_in_cents),
        report.rake_percent,
        report.games,
        args.seed,
        args.hands_per_level,
        args.hands_per_level_heads_up,
    );
    println!(
        "break-even win rate {:.2} %, average multiplier x{:.3}",
        100.0 * report.break_even_win_rate,
        report.average_multiplier
    );
    let width = report.seats.iter().map(|s| s.name.len()).max().unwrap_or(0);
    for (i, seat) in report.seats.iter().enumerate() {
        let (wl, wh) = seat.win_rate_ci95;
        let (rl, rh) = seat.roi_ci95;
        println!(
            "seat {} {:<width$}  win rate {:5.1} % [{:.1}, {:.1}]  vs break-even {:+5.1} pts  ROI {:+6.1} % [{:+.1}, {:+.1}]",
            i + 1,
            seat.name,
            100.0 * seat.win_rate,
            100.0 * wl,
            100.0 * wh,
            100.0 * seat.break_even_gap,
            100.0 * seat.roi,
            100.0 * rl,
            100.0 * rh,
        );
    }
}
