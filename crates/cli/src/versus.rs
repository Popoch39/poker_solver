//! `nitro versus`: the solver's hero against two population bots, the
//! equilibrium and the exploit compared on the same games.

use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nitro_hh::ohh::write_hand;
use nitro_hh::to_ohh;
use nitro_simulator::{
    PopulationBot, PrizeTable, SeatReport, SeatStrategy, SimulationConfig, SolverHero, Structure,
    Verdict, compare, hand_histories, simulate,
};
use nitro_solver::SolveOptions;

use crate::simulate::{euros_label, parse_buy_in};

#[derive(clap::Args)]
pub struct Args {
    /// Hand-history files or folders the population is modelled from.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
    /// Buy-in in euros: 0.25, 0.50, 1, 2, 5, 10, 25, 50, 100, 250 or 500.
    #[arg(long, value_parser = parse_buy_in)]
    buy_in: &'static PrizeTable,
    /// The hero's strategy: equilibrium, exploit, or both on the same games.
    #[arg(long, value_enum, default_value_t = Hero::Both)]
    hero: Hero,
    /// Number of games per hero strategy.
    #[arg(long, default_value_t = 10_000)]
    games: u64,
    /// Random seed: the same seed gives the same games and the same report.
    #[arg(long, default_value_t = 0)]
    seed: u64,
    /// Fewest decisions observed at a node, at a spot's stack bucket and
    /// table size, for the exploit to lock it.
    #[arg(long, default_value_t = 200)]
    min_sample: u32,
    /// Maximum number of solver iterations per spot.
    #[arg(long, default_value_t = SolveOptions::default().iterations)]
    iterations: u32,
    /// Stop each spot's solve once its exploitability is below this, in mBB
    /// per hand: a simulation cannot see the difference below about 1.
    #[arg(long, value_name = "MBB", default_value_t = 1.0)]
    target: f64,
    /// Hands per one-minute level while three players are left.
    #[arg(long, default_value_t = Structure::expresso_nitro().hands_per_level)]
    hands_per_level: u32,
    /// Hands per one-minute level once heads-up.
    #[arg(long, default_value_t = Structure::expresso_nitro().hands_per_level_heads_up)]
    hands_per_level_heads_up: u32,
    /// Also write the hands of the first games to this folder, one Open Hand
    /// History file per hero strategy (`solver-equilibrium.ohh`…).
    #[arg(long, value_name = "DIR")]
    ohh: Option<PathBuf>,
    /// Number of games written with --ohh.
    #[arg(long, default_value_t = 100, requires = "ohh")]
    ohh_games: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum Hero {
    Equilibrium,
    Exploit,
    Both,
}

/// The hero's table seat; the first button is drawn every game.
const HERO_SEAT: usize = 0;

pub fn run(args: &Args) -> ExitCode {
    if args.games == 0 || args.hands_per_level == 0 || args.hands_per_level_heads_up == 0 {
        eprintln!("error: --games and the hands per level must be at least 1");
        return ExitCode::from(2);
    }
    let (model, errors) = crate::population::build(&args.paths);
    let model = Arc::new(model);
    let options = crate::solve_options(args.iterations, args.target);
    let mut heroes = Vec::new();
    if args.hero != Hero::Exploit {
        heroes.push(SolverHero::equilibrium(options.clone()));
    }
    if args.hero != Hero::Equilibrium {
        heroes.push(SolverHero::exploit(
            Arc::clone(&model),
            args.min_sample,
            options,
        ));
    }

    let structure = Structure {
        hands_per_level: args.hands_per_level,
        hands_per_level_heads_up: args.hands_per_level_heads_up,
        ..Structure::expresso_nitro()
    };
    println!(
        "Hero against two population bots modelled on {} hands ({errors} files or hands skipped), push/fold",
        model.hands()
    );
    let bot: Arc<dyn SeatStrategy> = Arc::new(PopulationBot::new(Arc::clone(&model)));
    let heroes: Vec<Arc<SolverHero>> = heroes.into_iter().map(Arc::new).collect();
    let configs: Vec<SimulationConfig> = heroes
        .iter()
        .map(|hero| SimulationConfig {
            prize_table: args.buy_in.clone(),
            structure: structure.clone(),
            games: args.games,
            seed: args.seed,
            seats: [hero.clone(), bot.clone(), bot.clone()],
        })
        .collect();
    let start = Instant::now();
    let (reports, gain) = match &configs[..] {
        [equilibrium, exploit] => {
            let comparison = compare(equilibrium, exploit, HERO_SEAT);
            let reports = vec![comparison.baseline, comparison.challenger];
            (reports, Some(comparison.gain))
        }
        _ => (configs.iter().map(simulate).collect(), None),
    };
    let elapsed = start.elapsed();
    if let Some(dir) = &args.ohh {
        for (hero, config) in heroes.iter().zip(&configs) {
            let path = dir.join(format!("{}.ohh", hero.name()));
            match write_ohh(config, args.ohh_games.min(args.games), &path) {
                Ok(hands) => println!("{hands} hands written to {}", path.display()),
                Err(err) => {
                    eprintln!("error: cannot write {}: {err}", path.display());
                    return ExitCode::FAILURE;
                }
            }
        }
    }

    let first = &reports[0];
    println!(
        "Expresso Nitro {} € (rake {} %), {} games, seed {}, {} hands per level ({} heads-up)",
        euros_label(first.buy_in_cents),
        first.rake_percent,
        first.games,
        args.seed,
        args.hands_per_level,
        args.hands_per_level_heads_up,
    );
    println!(
        "break-even win rate {:.2} %, average multiplier x{:.3}",
        100.0 * first.break_even_win_rate,
        first.average_multiplier
    );
    for report in &reports {
        print_seat(&report.seats[HERO_SEAT]);
    }
    if let Some(gain) = gain {
        let (wl, wh) = gain.win_rate_ci95;
        let (rl, rh) = gain.roi_ci95;
        println!(
            "exploit - equilibrium, paired on the same games: win rate {:+.1} pts [{:+.1}, {:+.1}]  ROI {:+.1} % [{:+.1}, {:+.1}]",
            100.0 * gain.win_rate,
            100.0 * wl,
            100.0 * wh,
            100.0 * gain.roi,
            100.0 * rl,
            100.0 * rh,
        );
    }
    for hero in &heroes {
        println!(
            "{}: {} spots solved in {}",
            hero.name(),
            hero.spots_solved(),
            seconds(hero.solving_time()),
        );
    }
    println!("simulation: {} in all", seconds(elapsed));
    ExitCode::SUCCESS
}

fn print_seat(seat: &SeatReport) {
    let (wl, wh) = seat.win_rate_ci95;
    let (rl, rh) = seat.roi_ci95;
    let verdict = match seat.verdict {
        Verdict::AboveBreakEven => "above break-even",
        Verdict::BelowBreakEven => "below break-even",
        Verdict::Undecided => "undecided",
    };
    println!(
        "{}  win rate {:5.1} % [{:.1}, {:.1}]  vs break-even {:+5.1} pts  ROI after rake {:+6.1} % [{:+.1}, {:+.1}]  {verdict}",
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

fn seconds(duration: Duration) -> String {
    format!("{:.1} s", duration.as_secs_f64())
}

/// Writes the hands of the first `games` games, seen from the hero's seat.
fn write_ohh(config: &SimulationConfig, games: u64, path: &Path) -> io::Result<usize> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut file = BufWriter::new(File::create(path)?);
    let mut count = 0;
    for game in 0..games {
        for hand in hand_histories(config, game, Some(HERO_SEAT)) {
            write_hand(&mut file, to_ohh(&hand, None))?;
            count += 1;
        }
    }
    file.flush()?;
    Ok(count)
}
