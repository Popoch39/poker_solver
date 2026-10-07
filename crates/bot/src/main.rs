//! `nitro-bot`: the clicker bot's vision, from the command line.
//!
//! A thin shell over the library: `measure` runs offscreen, `read` reads a
//! saved frame or the live client, `capture` saves the live client's
//! window.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use nitro_bot::{Grim, Hyprctl, MeasureConfig, TableReader, capture_client, measure};
use nitro_local_client::Frame;
use nitro_simulator::{SeatStrategy, TrivialBot};

/// Misread states the vision may have, at most (the ticket's target).
const TARGET_ERROR_RATE: f64 = 0.01;

#[derive(Parser)]
#[command(version, about = "Read the local client's table from its pixels")]
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
    /// Read the table from a PNG frame, or from the local client's window
    /// when no PNG is given.
    Read { png: Option<PathBuf> },
    /// Save the local client's window to a PNG.
    Capture { png: PathBuf },
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
            let [first, second] = bots[..] else {
                return Err(format!("--bots takes two bots, got {}", bots.len()));
            };
            let config = MeasureConfig {
                hands,
                seed,
                bots: [first, second].map(|b| Arc::new(b) as Arc<dyn SeatStrategy>),
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

fn load(path: &PathBuf) -> Result<Frame, String> {
    std::fs::read(path)
        .and_then(|bytes| Frame::decode_png(&bytes))
        .map_err(|e| format!("cannot read {}: {e}", path.display()))
}
