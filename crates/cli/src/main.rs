//! `nitro`: command-line entry point of the Expresso Nitro study tool.
//!
//! A thin layer: it parses arguments, calls the solver's or the simulator's
//! public interface and formats the result. No poker logic lives here.

mod simulate;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use nitro_solver::{Action, HandClass, Node, Position, Solution, SolveOptions, Spot, solve};

#[derive(Parser)]
#[command(version, about = "Expresso Nitro preflop study tool")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Solve a heads-up push/fold spot and print its equilibrium ranges.
    Solve {
        /// Stacks in big blinds before posting the blinds: SB,BB.
        #[arg(long, value_delimiter = ',', value_name = "SB,BB", required = true)]
        stacks: Vec<f64>,
        /// Number of CFR+ iterations.
        #[arg(long, default_value_t = SolveOptions::default().iterations)]
        iterations: u32,
        /// Only print the strategy of this hand (e.g. K7o, AKs, 99).
        #[arg(long)]
        hand: Option<HandClass>,
        /// Restrict --hand to one node: sb-open or bb-vs-sb-push.
        #[arg(long, requires = "hand")]
        node: Option<Node>,
    },
    /// Play many Expresso Nitro between bots and report win rate and ROI.
    Simulate(simulate::Args),
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Solve {
            stacks,
            iterations,
            hand,
            node,
        } => run_solve(stacks, iterations, hand, node),
        Command::Simulate(args) => simulate::run(&args),
    }
}

fn run_solve(
    stacks: Vec<f64>,
    iterations: u32,
    hand: Option<HandClass>,
    node: Option<Node>,
) -> ExitCode {
    let spot = match stacks[..] {
        [sb, bb] => Spot::heads_up(sb, bb).map_err(|err| err.to_string()),
        _ => Err(format!(
            "--stacks takes two stacks (SB,BB), got {}",
            stacks.len()
        )),
    };
    let spot = match spot {
        Ok(spot) => spot,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(2);
        }
    };
    let solution = solve(&spot, &SolveOptions { iterations });
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
    println!(
        "Heads-up push/fold, stacks SB {} BB / BB {} BB (effective {} BB), chip EV",
        spot.stack(Position::Sb),
        spot.stack(Position::Bb),
        spot.effective_stack()
    );
    println!(
        "{} CFR+ iterations, exploitability {:.4} mBB/hand ({})",
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
    const RANKS: [char; 13] = [
        'A', 'K', 'Q', 'J', 'T', '9', '8', '7', '6', '5', '4', '3', '2',
    ];
    let header: String = RANKS.iter().map(|r| format!("{r:>4}")).collect();
    println!("   {header}");
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
        println!("  {rank}{cells}");
    }
}
