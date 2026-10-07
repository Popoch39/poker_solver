//! `nitro`: command-line entry point of the Expresso Nitro study tool.
//!
//! A thin layer: it parses arguments, calls the public interface of the
//! solver, the simulator or the hand-history parser and formats the result.
//! No poker logic lives here.

mod exploit;
mod hh;
mod population;
mod simulate;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use nitro_solver::{Action, HandClass, Node, Solution, SolveOptions, Spot, solve};

#[derive(Parser)]
#[command(version, about = "Expresso Nitro preflop study tool")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Solve a push/fold spot and print its equilibrium ranges.
    Solve {
        /// Stacks in big blinds before posting the blinds: BTN,SB,BB for
        /// 3-max (0 for an eliminated player), SB,BB for heads-up.
        #[arg(long, value_delimiter = ',', value_name = "BTN,SB,BB", required = true)]
        stacks: Vec<f64>,
        /// Maximum number of solver iterations.
        #[arg(long, default_value_t = SolveOptions::default().iterations)]
        iterations: u32,
        /// Stop once the exploitability is below this, in mBB per hand
        /// (0 runs every iteration).
        #[arg(
            long,
            value_name = "MBB",
            default_value_t = 1000.0 * SolveOptions::default().target_exploitability.unwrap_or(0.0)
        )]
        target: f64,
        /// Only print the strategy of this hand (e.g. K7o, AKs, 99).
        #[arg(long)]
        hand: Option<HandClass>,
        /// Restrict --hand to one node (e.g. btn-open, sb-vs-btn-push).
        #[arg(long, requires = "hand")]
        node: Option<Node>,
        /// Also write the solution to this CSV file (one row per node, hand
        /// and action).
        #[arg(long, value_name = "FILE")]
        csv: Option<PathBuf>,
    },
    /// Play many Expresso Nitro between bots and report win rate and ROI.
    Simulate(simulate::Args),
    /// Parse Winamax Expresso Nitro hand histories and summaries, and report
    /// what was read and what was not.
    Hh {
        /// Hand-history, summary or Open Hand History (`.ohh`) files, or
        /// folders searched for `.txt` and `.ohh` files.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Also write every parsed hand to this Open Hand History file.
        #[arg(long, value_name = "FILE")]
        ohh: Option<PathBuf>,
    },
    /// Aggregate hand histories into the population model: action
    /// frequencies per push/fold node and stack bucket, with sample sizes.
    Population(population::Args),
    /// Lock the population's well-sampled nodes into a spot and re-solve the
    /// hero's: equilibrium and exploit ranges side by side, with the gain
    /// against the population and the cost against the equilibrium.
    Exploit(exploit::Args),
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Solve {
            stacks,
            iterations,
            target,
            hand,
            node,
            csv,
        } => run_solve(stacks, iterations, target, hand, node, csv),
        Command::Simulate(args) => simulate::run(&args),
        Command::Hh { paths, ohh } => hh::run(&paths, ohh.as_deref()),
        Command::Population(args) => population::run(&args),
        Command::Exploit(args) => exploit::run(&args),
    }
}

fn run_solve(
    stacks: Vec<f64>,
    iterations: u32,
    target: f64,
    hand: Option<HandClass>,
    node: Option<Node>,
    csv: Option<PathBuf>,
) -> ExitCode {
    let spot = match parse_spot(&stacks) {
        Ok(spot) => spot,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(2);
        }
    };
    let solution = solve(&spot, &solve_options(iterations, target));
    if let Some(path) = csv {
        let written = File::create(&path)
            .map(BufWriter::new)
            .and_then(|mut file| {
                solution.write_csv(&mut file)?;
                file.flush()
            });
        if let Err(err) = written {
            eprintln!("error: cannot write {}: {err}", path.display());
            return ExitCode::FAILURE;
        }
    }
    match hand {
        Some(hand) => {
            let nodes = match node {
                Some(node) => vec![node],
                None => solution.nodes().to_vec(),
            };
            for node in nodes {
                print_hand(&solution, node, hand);
            }
        }
        None => print_report(&solution),
    }
    ExitCode::SUCCESS
}

/// The spot of `--stacks`: BTN,SB,BB for 3-max, SB,BB for heads-up.
fn parse_spot(stacks: &[f64]) -> Result<Spot, String> {
    match stacks[..] {
        [btn, sb, bb] => Spot::three_max(btn, sb, bb).map_err(|err| err.to_string()),
        [sb, bb] => Spot::heads_up(sb, bb).map_err(|err| err.to_string()),
        _ => Err(format!(
            "--stacks takes two or three stacks (BTN,SB,BB or SB,BB), got {}",
            stacks.len()
        )),
    }
}

/// `--iterations` and `--target` (in mBB per hand, 0 for none).
fn solve_options(iterations: u32, target: f64) -> SolveOptions {
    SolveOptions {
        iterations,
        target_exploitability: (target > 0.0).then_some(target / 1000.0),
    }
}

fn print_hand(solution: &Solution, node: Node, hand: HandClass) {
    let strategy = solution
        .strategy(node, hand)
        .expect("the node belongs to the solved tree");
    let frequencies: Vec<String> = strategy
        .iter()
        .map(|(action, freq)| format!("{action} {:.1}%", 100.0 * freq))
        .collect();
    println!("{hand} at {node}: {}", frequencies.join(", "));
}

fn print_report(solution: &Solution) {
    let spot = solution.spot();
    let exploitability = solution.exploitability();
    let per_player: Vec<String> = exploitability
        .per_player()
        .iter()
        .map(|(position, gain)| format!("{position} {:.4}", 1000.0 * gain))
        .collect();
    let stacks: Vec<String> = spot
        .positions()
        .into_iter()
        .map(|position| format!("{position} {} BB", spot.stack(position)))
        .collect();
    match spot.positions()[..] {
        [_, _] => println!(
            "Heads-up push/fold, stacks {} (effective {} BB), chip EV",
            stacks.join(" / "),
            spot.effective_stack()
        ),
        _ => println!("3-max push/fold, stacks {}, chip EV", stacks.join(" / ")),
    }
    println!(
        "{} iterations, exploitability {:.4} mBB/hand ({})",
        solution.iterations(),
        1000.0 * exploitability.total(),
        per_player.join(", ")
    );
    for &node in solution.nodes() {
        // The last action of a node is the aggressive one (push or call).
        let action = *node.actions().last().expect("a node has actions");
        let share = solution
            .action_share(node, action)
            .expect("the node belongs to the solved tree");
        println!();
        println!("{node}: {action} {:.1}% of combos", 100.0 * share);
        println!("(% {action} per hand; pairs on the diagonal, suited above, offsuit below)");
        print_grid(solution, node, action);
    }
}

fn print_grid(solution: &Solution, node: Node, action: Action) {
    for line in grid(solution, node, action) {
        println!("{line}");
    }
}

/// The 13×13 grid of `action`'s frequency at `node`, in percent, one string
/// per line: a header of ranks, then one row per rank.
fn grid(solution: &Solution, node: Node, action: Action) -> Vec<String> {
    const RANKS: [char; 13] = [
        'A', 'K', 'Q', 'J', 'T', '9', '8', '7', '6', '5', '4', '3', '2',
    ];
    let header: String = RANKS.iter().map(|r| format!("{r:>4}")).collect();
    let mut lines = vec![format!("   {header}")];
    for (row, rank) in RANKS.iter().enumerate() {
        let cells: String = (0..13)
            .map(|col| {
                let hand = HandClass::at_grid(row, col);
                let freq = solution
                    .strategy(node, hand)
                    .expect("the node belongs to the solved tree")
                    .frequency(action);
                let percent = (100.0 * freq).round();
                if percent == 0.0 {
                    format!("{:>4}", ".")
                } else {
                    format!("{percent:>4}")
                }
            })
            .collect();
        lines.push(format!("  {rank}{cells}"));
    }
    lines
}
