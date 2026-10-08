//! `nitro population`: the population model of a set of hand histories.

use std::path::PathBuf;
use std::process::ExitCode;

use nitro_hh::parse_paths;
use nitro_population::{
    Action, Node, NodeStats, OffTreeAction, Players, PopulationModel, Position,
};

#[derive(clap::Args)]
pub struct Args {
    /// Hand-history files or folders: the NitroVariance dataset, or a
    /// Winamax history folder such as
    /// `~/.config/winamax/documents/accounts/<account>/history`.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
    /// Also print, for each stack bucket of this node (e.g. bb-vs-btn-push),
    /// the range of its push or call estimated from the hands shown.
    #[arg(long, value_name = "NODE")]
    range: Option<Node>,
}

pub fn run(args: &Args) -> ExitCode {
    let (model, errors) = build(&args.paths);
    print_model(&model, errors);
    if let Some(node) = args.range {
        print_ranges(&model, node);
    }
    ExitCode::SUCCESS
}

/// The population model of the opponents in the histories at `paths`, and
/// how many files or hands could not be read.
pub fn build(paths: &[PathBuf]) -> (PopulationModel, usize) {
    let batch = parse_paths(paths);
    let model = PopulationModel::build(batch.hands(), Players::Opponents);
    (model, batch.errors.len())
}

fn print_model(model: &PopulationModel, errors: usize) {
    let unplaced = model.hands() - model.hands_in_tree() - model.hands_off_tree();
    println!(
        "{} hands: {} in the push/fold tree, {} off it, {unplaced} with seats that could not be placed",
        model.hands(),
        model.hands_in_tree(),
        model.hands_off_tree()
    );
    crate::hh::print_skipped(errors);
    println!("Decisions of every player but the account owner of the histories.");
    println!(
        "First in (BTN or SB open), every action: a min-raise is to 2 BB, a raise more, short of all-in."
    );
    println!();
    for (node, bucket, stats) in model.entries() {
        let action = node.aggressive_action();
        println!(
            "{node}, {bucket}: {}; {} {action} shown",
            distribution(stats),
            stats.known_hands(action).total()
        );
    }
    println!();
    println!("Decisions that left the push/fold tree:");
    let off_tree = model.off_tree();
    for position in [Position::Btn, Position::Sb, Position::Bb] {
        let counts: Vec<String> = OFF_TREE_ACTIONS
            .iter()
            .map(|&action| format!("{action} {}", off_tree.count(position, action)))
            .collect();
        println!("  {position}: {}", counts.join(", "));
    }
    println!(
        "Later decisions: {} preflop, {} postflop",
        off_tree.later_preflop(),
        off_tree.postflop()
    );
}

/// Every action counted at a node with its share of the decisions, then the
/// sample: `fold 50.0%, limp 20.0%, min-raise 5.0%, raise 3.0%, push 22.0%
/// on 1000`.
pub fn distribution(stats: &NodeStats) -> String {
    let mut shares: Vec<String> = stats
        .actions()
        .iter()
        .map(|&action| {
            let label = match action {
                Action::Raise => "min-raise".to_owned(),
                _ => action.to_string(),
            };
            format!("{label} {:.1}%", 100.0 * stats.frequency(action))
        })
        .collect();
    if stats.actions().contains(&Action::Raise) {
        let raises = f64::from(stats.other_raises()) / f64::from(stats.sample());
        // Raises above the min-raise sit between it and the push.
        shares.insert(shares.len() - 1, format!("raise {:.1}%", 100.0 * raises));
    }
    format!("{} on {}", shares.join(", "), stats.sample())
}

const OFF_TREE_ACTIONS: [OffTreeAction; 4] = [
    OffTreeAction::Limp,
    OffTreeAction::MinRaise,
    OffTreeAction::Raise,
    OffTreeAction::Other,
];

fn print_ranges(model: &PopulationModel, node: Node) {
    let action = node.aggressive_action();
    for (_, bucket, stats) in model.entries().filter(|&(n, _, _)| n == node) {
        let shown = stats.known_hands(action).total();
        if shown == 0 {
            continue;
        }
        println!();
        println!("{node}, {bucket}: {action} range estimated from {shown} shown hands");
        println!("(% {action} per hand; pairs on the diagonal, suited above, offsuit below)");
        let grid = crate::grid(|hand| stats.estimated_frequency(action, hand).unwrap_or(0.0));
        for line in grid {
            println!("{line}");
        }
    }
}
