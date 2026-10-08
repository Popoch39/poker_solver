//! `nitro-bot`: the clicker bot, from the command line.
//!
//! A thin shell over the library: `measure` and `offscreen` run offscreen,
//! `read` reads a saved frame or the live client, `capture` saves the live
//! client's window, and `play` plays the live client by injecting clicks.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use clap::{Parser, Subcommand, ValueEnum};
use nitro_bot::{
    ClickerBot, EmergencyStop, Grim, Hyprctl, MeasureConfig, Pace, TableReader, Uinput,
    capture_client, find_client_window, measure, play_live, play_offscreen,
};
use nitro_hh::parse_paths;
use nitro_local_client::Frame;
use nitro_population::{Players, PopulationModel};
use nitro_simulator::{
    PopulationBot, PrizeTable, SeatStrategy, SimulationConfig, SolverHero, Structure, TrivialBot,
};
use nitro_solver::SolveOptions;
use rand::SeedableRng;
use rand::rngs::StdRng;

/// Misread states the vision may have, at most (the ticket's target).
const TARGET_ERROR_RATE: f64 = 0.01;

/// Where a desktop shortcut drops the stop file by default, under
/// `$XDG_RUNTIME_DIR`.
const STOP_FILE: &str = "nitro-bot.stop";

#[derive(Parser)]
#[command(
    version,
    about = "The clicker bot: reads the local client's table and plays it"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Play hands on the client offscreen, read every frame and compare it
    /// with the simulator's table.
    Measure {
        #[arg(long, default_value_t = 1_000)]
        hands: u32,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// Bots of seats 1 and 2: always-all-in, always-fold or random.
        #[arg(
            long,
            value_delimiter = ',',
            value_name = "BOT,BOT",
            default_value = "random,random"
        )]
        bots: Vec<TrivialBot>,
    },
    /// Play whole tournaments with the bot on the client offscreen, and the
    /// same games with the hero directly in the simulator: decision
    /// concordance and win rates.
    Offscreen {
        #[command(flatten)]
        hero: HeroArgs,
        #[arg(long, default_value_t = 100)]
        games: u64,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// Bots of seats 1 and 2 without --histories: always-all-in,
        /// always-fold or random. With --histories, two population bots.
        #[arg(
            long,
            value_delimiter = ',',
            value_name = "BOT,BOT",
            default_value = "random,random"
        )]
        bots: Vec<TrivialBot>,
    },
    /// Play the local client's game by clicking its buttons: injects real
    /// input through uinput, into the local client's window only.
    Play {
        #[command(flatten)]
        hero: HeroArgs,
        /// Seeds the hero's mixed decisions. Defaults to the clock.
        #[arg(long)]
        seed: Option<u64>,
        /// Between two captures, in milliseconds.
        #[arg(long, default_value_t = 100)]
        poll_ms: u64,
        /// How long the client may take to show a click, or stay
        /// unreadable, before the bot stops, in milliseconds.
        #[arg(long, default_value_t = 10_000)]
        patience_ms: u64,
        /// Emergency stop: the bot stops as soon as this file exists.
        /// Defaults to $XDG_RUNTIME_DIR/nitro-bot.stop.
        #[arg(long, value_name = "PATH")]
        stop_file: Option<PathBuf>,
    },
    /// Read the table from a PNG frame, or from the local client's window
    /// when no PNG is given.
    Read { png: Option<PathBuf> },
    /// Save the local client's window to a PNG.
    Capture { png: PathBuf },
}

/// The hero the bot plays.
#[derive(clap::Args)]
struct HeroArgs {
    #[arg(long, value_enum, default_value_t = Hero::Equilibrium)]
    hero: Hero,
    /// Hand-history files or folders the population is modelled from: the
    /// exploit is locked on it, and offscreen the bots play it.
    #[arg(long, num_args = 1.., value_name = "PATH")]
    histories: Vec<PathBuf>,
    /// Fewest decisions observed at a node for the exploit to lock it.
    #[arg(long, default_value_t = 200)]
    min_sample: u32,
    /// Maximum number of solver iterations per spot.
    #[arg(long, default_value_t = SolveOptions::default().iterations)]
    iterations: u32,
    /// Stop each spot's solve below this exploitability, in mBB per hand.
    #[arg(long, value_name = "MBB", default_value_t = 1.0)]
    target: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Hero {
    Equilibrium,
    Exploit,
}

impl HeroArgs {
    /// The population of the histories, when some are given.
    fn population(&self) -> Option<Arc<PopulationModel>> {
        (!self.histories.is_empty()).then(|| Arc::new(population(&self.histories)))
    }

    /// The hero, locked on `population` for the exploit.
    fn hero(
        &self,
        population: Option<&Arc<PopulationModel>>,
    ) -> Result<Arc<dyn SeatStrategy>, String> {
        let options = SolveOptions {
            iterations: self.iterations,
            target_exploitability: (self.target > 0.0).then_some(self.target / 1000.0),
        };
        let hero = match (self.hero, population) {
            (Hero::Equilibrium, _) => SolverHero::equilibrium(options),
            (Hero::Exploit, Some(model)) => {
                SolverHero::exploit(Arc::clone(model), self.min_sample, options)
            }
            (Hero::Exploit, None) => return Err("--hero exploit needs --histories".to_owned()),
        };
        Ok(Arc::new(hero))
    }
}

/// The opponents' population model of the histories at `paths`.
fn population(paths: &[PathBuf]) -> PopulationModel {
    let batch = parse_paths(paths);
    if !batch.errors.is_empty() {
        eprintln!(
            "{} files or hands skipped: other formats or unreadable (see `nitro hh`)",
            batch.errors.len()
        );
    }
    PopulationModel::build(batch.hands(), Players::Opponents)
}

fn main() -> ExitCode {
    match run(Args::parse().command) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<ExitCode, String> {
    match command {
        Command::Measure { hands, seed, bots } => {
            let config = MeasureConfig {
                hands,
                seed,
                bots: two_bots(&bots)?,
            };
            let reader = TableReader::new();
            let measurement = measure(&config, |frame| reader.read(frame));
            println!(
                "{} hands, {} states, {} misread",
                measurement.hands, measurement.states, measurement.misread
            );
            let rate = measurement.error_rate();
            println!(
                "error rate {:.2} % (target < {:.0} %)",
                rate * 100.0,
                TARGET_ERROR_RATE * 100.0
            );
            if let Some(misread) = &measurement.first_misread {
                println!("first misread, hand {}:", misread.hand_number);
                println!("expected {:#?}", misread.expected);
                println!("read {:#?}", misread.read);
            }
            Ok(if rate < TARGET_ERROR_RATE {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        Command::Offscreen {
            hero,
            games,
            seed,
            bots,
        } => offscreen(&hero, games, seed, &bots),
        Command::Play {
            hero,
            seed,
            poll_ms,
            patience_ms,
            stop_file,
        } => play(&hero, seed, poll_ms, patience_ms, stop_file),
        Command::Read { png } => {
            let frame = match png {
                Some(path) => load(&path)?,
                None => capture_client(&Hyprctl, &Grim).map_err(|e| e.to_string())?,
            };
            let state = TableReader::new().read(&frame).map_err(|e| e.to_string())?;
            println!("{state:#?}");
            Ok(ExitCode::SUCCESS)
        }
        Command::Capture { png } => {
            let frame = capture_client(&Hyprctl, &Grim).map_err(|e| e.to_string())?;
            frame
                .save_png(&png)
                .map_err(|e| format!("cannot write {}: {e}", png.display()))?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn two_bots(bots: &[TrivialBot]) -> Result<[Arc<dyn SeatStrategy>; 2], String> {
    let [first, second] = bots[..] else {
        return Err(format!("--bots takes two bots, got {}", bots.len()));
    };
    Ok([first, second].map(|b| Arc::new(b) as Arc<dyn SeatStrategy>))
}

fn offscreen(
    hero: &HeroArgs,
    games: u64,
    seed: u64,
    bots: &[TrivialBot],
) -> Result<ExitCode, String> {
    if games == 0 {
        return Err("--games must be at least 1".to_owned());
    }
    let population = hero.population();
    let hero = hero.hero(population.as_ref())?;
    let [first, second] = match population {
        Some(model) => {
            let bot = Arc::new(PopulationBot::new(model)) as Arc<dyn SeatStrategy>;
            [bot.clone(), bot]
        }
        None => two_bots(bots)?,
    };
    let config = SimulationConfig {
        prize_table: PrizeTable::for_buy_in(100)
            .expect("1 € is a buy-in")
            .clone(),
        structure: Structure::expresso_nitro(),
        games,
        seed,
        seats: [hero, first, second],
    };
    let reader = TableReader::new();
    let report = play_offscreen(&config, |frame| reader.read(frame));
    let comparison = &report.comparison;
    let (bot, direct, gain) = (
        &comparison.challenger.seats[0],
        &comparison.baseline.seats[0],
        &comparison.gain,
    );
    let percent = |(low, high): (f64, f64)| format!("[{:.1} ; {:.1}]", low * 100.0, high * 100.0);
    println!(
        "{games} games, {} decisions, {} unplayed: concordance {:.2} %",
        report.decisions,
        report.unplayed,
        report.concordance() * 100.0
    );
    println!(
        "bot win rate   {:.1} % {}",
        bot.win_rate * 100.0,
        percent(bot.win_rate_ci95)
    );
    println!(
        "hero simulated {:.1} % {}",
        direct.win_rate * 100.0,
        percent(direct.win_rate_ci95)
    );
    println!(
        "bot − hero     {:+.1} pt {} on the same games",
        gain.win_rate * 100.0,
        percent(gain.win_rate_ci95)
    );
    if let Some(discordance) = &report.first_discordance {
        println!("first discordance: {discordance:?}");
    }
    Ok(if report.concordance() >= 1.0 - TARGET_ERROR_RATE {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn play(
    hero: &HeroArgs,
    seed: Option<u64>,
    poll_ms: u64,
    patience_ms: u64,
    stop_file: Option<PathBuf>,
) -> Result<ExitCode, String> {
    // Before anything else: a stop still raised from last time means no.
    let stop_file = match stop_file {
        Some(path) => path,
        None => std::env::var_os("XDG_RUNTIME_DIR")
            .map(|dir| Path::new(&dir).join(STOP_FILE))
            .ok_or("XDG_RUNTIME_DIR is not set: give a --stop-file")?,
    };
    if stop_file.exists() {
        return Err(format!(
            "the stop file {} exists: remove it to start",
            stop_file.display()
        ));
    }
    let stop = EmergencyStop::with_file(&stop_file);
    stop.on_signals()
        .map_err(|e| format!("cannot catch SIGINT and SIGTERM: {e}"))?;
    let hero = hero.hero(hero.population().as_ref())?;
    find_client_window(&Hyprctl).map_err(|e| e.to_string())?;
    let area = Hyprctl.screen_area().map_err(|e| e.to_string())?;
    let injector = Uinput::new(area, stop.clone()).map_err(|e| e.to_string())?;
    let mut bot = ClickerBot::new(
        hero,
        Box::new(Hyprctl),
        Box::new(Hyprctl),
        Box::new(injector),
        stop,
    );
    let seed = seed.unwrap_or_else(|| {
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);
        now.map_or(0, |d| d.as_nanos() as u64)
    });
    eprintln!(
        "playing (seed {seed}); emergency stop: Ctrl-C, SIGTERM or `touch {}`",
        stop_file.display()
    );
    let reader = TableReader::new();
    let played = play_live(
        &mut bot,
        &mut || capture_client(&Hyprctl, &Grim),
        &|frame| reader.read(frame),
        &mut StdRng::seed_from_u64(seed),
        Pace {
            poll: Duration::from_millis(poll_ms),
            patience: Duration::from_millis(patience_ms),
        },
    );
    match played {
        Ok(clicks) => {
            println!("game over, {clicks} clicks");
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => Err(format!("stopped: {err}")),
    }
}

fn load(path: &PathBuf) -> Result<Frame, String> {
    std::fs::read(path)
        .and_then(|bytes| Frame::decode_png(&bytes))
        .map_err(|e| format!("cannot read {}: {e}", path.display()))
}
