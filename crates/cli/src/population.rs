//! `nitro population`: the population model of a set of hand histories.

use std::path::PathBuf;
use std::process::ExitCode;

use nitro_hh::parse_path;
use nitro_population::{
    Action, HandClass, Node, NodeStats, OffTreeAction, Players, PopulationModel, Position,
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
    let mut errors = 0;
    let mut hands = Vec::new();
    for path in paths {
        let batch = parse_path(path);
        errors += batch.errors.len();
        hands.extend(batch.tournaments.into_iter().flat_map(|t| t.hands));
    }
    (PopulationModel::build(&hands, Players::Opponents), errors)
}

fn print_model(model: &PopulationModel, errors: usize) {
    let unplaced = model.hands() - model.hands_in_tree() - model.hands_off_tree();
    println!(
        "{} hands: {} in the push/fold tree, {} off it, {unplaced} with seats that could not be placed",
        model.hands(),
        model.hands_in_tree(),
        model.hands_off_tree()
    );
    println!("{errors} files or hands skipped: other formats or unreadable (see `nitro hh`)");
    println!("Decisions of every player but the account owner of the histories.");
    println!();
    for (node, bucket, stats) in model.entries() {
        let action = node.aggressive_action();
        println!(
            "{node}, {bucket}: {action} {:.1}% on {} ({} shown)",
            100.0 * stats.frequency(action),
            stats.sample(),
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
        print_grid(stats, action);
    }
}

fn print_grid(stats: &NodeStats, action: Action) {
    const RANKS: [char; 13] = [
        'A', 'K', 'Q', 'J', 'T', '9', '8', '7', '6', '5', '4', '3', '2',
    ];
    let header: String = RANKS.iter().map(|r| format!("{r:>4}")).collect();
    println!("   {header}");
    for (row, rank) in RANKS.iter().enumerate() {
        let cells: String = (0..13)
            .map(|col| {
                let freq = stats
                    .estimated_frequency(action, HandClass::at_grid(row, col))
                    .unwrap_or(0.0);
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
