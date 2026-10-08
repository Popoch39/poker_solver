//! `nitro`: command-line entry point of the Expresso Nitro study tool.
//!
//! A thin layer: it parses arguments, calls the public interface of the
//! solver, the simulator or the hand-history parser and formats the result.
//! No poker logic lives here.

mod exploit;
mod hh;
mod leaks;
mod population;
mod simulate;
mod versus;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use nitro_solver::{
    Action, HandClass, Node, RealizationFactors, Solution, SolveOptions, Spot, solve,
};

#[derive(Parser)]
#[command(version, about = "Expresso Nitro preflop study tool")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Solve a spot (push/fold, plus limps and min-raises if allowed) and
    /// print its equilibrium ranges.
    Solve(SolveArgs),
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
    /// Compare the account owner's frequencies to the equilibrium, node by
    /// node, sorted by the chips each leak is estimated to cost.
    Leaks(leaks::Args),
    /// Lock the population's well-sampled nodes into a spot and re-solve the
    /// hero's: equilibrium and exploit ranges side by side, with the gain
    /// against the population and the cost against the equilibrium.
    Exploit(exploit::Args),
    /// Play many Expresso Nitro, the solver's hero against two population
    /// bots: the equilibrium and the exploit compared on the same games,
    /// with win rate, ROI after rake and the break-even verdict.
    Versus(versus::Args),
}

#[derive(clap::Args)]
struct SolveArgs {
    /// Stacks in big blinds before posting the blinds: BTN,SB,BB for
    /// 3-max (0 for an eliminated player), SB,BB for heads-up.
    #[arg(long, value_delimiter = ',', value_name = "BTN,SB,BB", required = true)]
    stacks: Vec<f64>,
    /// Also allow limping (and checking from the BB).
    #[arg(long)]
    limp: bool,
    /// Also allow one min-raise per hand, to 2 BB.
    #[arg(long)]
    min_raise: bool,
    /// Realization factor, heads-up at the flop, of the player out of
    /// position.
    #[arg(
        long,
        value_name = "FACTOR",
        allow_negative_numbers = true,
        default_value_t = RealizationFactors::default().out_of_position
    )]
    realization_oop: f64,
    /// Realization factor, heads-up at the flop, of the player in position.
    #[arg(
        long,
        value_name = "FACTOR",
        allow_negative_numbers = true,
        default_value_t = RealizationFactors::default().in_position
    )]
    realization_ip: f64,
    /// Realization factors three-way at the flop, of the SB, the BB and the
    /// BTN [default: 0.9,1,1.1].
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "SB,BB,BTN",
        allow_negative_numbers = true
    )]
    realization_3way: Option<Vec<f64>>,
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
    /// Only print this node (e.g. btn-open, sb-vs-btn-push, bb-vs-sb-limp).
    #[arg(long)]
    node: Option<Node>,
    /// Also write the solution to this CSV file (one row per node, hand
    /// and action).
    #[arg(long, value_name = "FILE")]
    csv: Option<PathBuf>,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Solve(args) => run_solve(args),
        Command::Simulate(args) => simulate::run(&args),
        Command::Hh { paths, ohh } => hh::run(&paths, ohh.as_deref()),
        Command::Population(args) => population::run(&args),
        Command::Leaks(args) => leaks::run(&args),
        Command::Exploit(args) => exploit::run(&args),
        Command::Versus(args) => versus::run(&args),
    }
}

/// The spot of `nitro solve`: its stacks, allowed actions and realization
/// factors.
fn spot(args: &SolveArgs) -> Result<Spot, String> {
    let spot = parse_spot(&args.stacks)?;
    let defaults = RealizationFactors::default();
    let [three_way_sb, three_way_bb, three_way_btn] = match args.realization_3way.as_deref() {
        Some(&[sb, bb, btn]) => [sb, bb, btn],
        Some(factors) => {
            return Err(format!(
                "--realization-3way takes three factors (SB,BB,BTN), got {}",
                factors.len()
            ));
        }
        None => [
            defaults.three_way_sb,
            defaults.three_way_bb,
            defaults.three_way_btn,
        ],
    };
    let factors = RealizationFactors {
        out_of_position: args.realization_oop,
        in_position: args.realization_ip,
        three_way_sb,
        three_way_bb,
        three_way_btn,
    };
    spot.with_limp(args.limp)
        .with_min_raise(args.min_raise)
        .with_realization(factors)
        .map_err(|err| err.to_string())
}

fn run_solve(args: SolveArgs) -> ExitCode {
    let spot = match spot(&args) {
        Ok(spot) => spot,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(2);
        }
    };
    let SolveArgs {
        iterations,
        target,
        hand,
        node,
        csv,
        ..
    } = args;
    let solution = solve(&spot, &solve_options(iterations, target));
    if let Some(node) = node
        && solution.actions(node).is_none()
    {
        eprintln!("error: node {node} is not in this spot's tree");
        return ExitCode::from(2);
    }
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
        None => print_report(&solution, node),
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

/// The spot, the exploitability, then the ranges of every node (or only of
/// `only`).
fn print_report(solution: &Solution, only: Option<Node>) {
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
    let tree = match (spot.allows_limp(), spot.allows_min_raise()) {
        (false, false) => "push/fold",
        (true, false) => "push/fold + limp",
        (false, true) => "push/fold + min-raise",
        (true, true) => "push/fold + limp + min-raise",
    };
    match spot.positions()[..] {
        [_, _] => println!(
            "Heads-up {tree}, stacks {} (effective {} BB), chip EV",
            stacks.join(" / "),
            spot.effective_stack()
        ),
        _ => println!("3-max {tree}, stacks {}, chip EV", stacks.join(" / ")),
    }
    if spot.allows_limp() || spot.allows_min_raise() {
        let r = spot.realization();
        println!(
            "realization factors: heads-up OOP {} / IP {}, three-way SB {} / BB {} / BTN {}",
            r.out_of_position, r.in_position, r.three_way_sb, r.three_way_bb, r.three_way_btn
        );
    }
    println!(
        "{} iterations, exploitability {:.4} mBB/hand ({})",
        solution.iterations(),
        1000.0 * exploitability.total(),
        per_player.join(", ")
    );
    let nodes = match only {
        Some(node) => vec![node],
        None => solution.nodes().to_vec(),
    };
    for node in nodes {
        let actions = solution
            .actions(node)
            .expect("the node belongs to the solved tree");
        // The first action is the passive one (fold or check): the grids of
        // the others say what is left.
        for &action in &actions[1..] {
            let share = solution
                .action_share(node, action)
                .expect("the node belongs to the solved tree");
            println!();
            println!("{node}: {action} {:.1}% of combos", 100.0 * share);
            println!("(% {action} per hand; pairs on the diagonal, suited above, offsuit below)");
            print_grid(solution, node, action);
        }
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
