//! `nitro leaks`: the account owner's frequencies next to the equilibrium,
//! sorted by the chips each deviation is estimated to cost.

use std::path::PathBuf;
use std::process::ExitCode;

use nitro_hh::parse_paths;
use nitro_population::{LeakOptions, LeakReport};

#[derive(clap::Args)]
pub struct Args {
    /// Hand-history files or folders, such as
    /// `~/.config/winamax/documents/accounts/<account>/history`.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
    /// Pseudo of the account whose histories are analysed: only the hands
    /// dealt to it are read. It is neither kept nor printed.
    #[arg(long, value_name = "PSEUDO")]
    hero: String,
    /// Number of decisions below which a node is flagged inconclusive.
    #[arg(long, value_name = "N", default_value_t = LeakOptions::default().min_sample)]
    min_sample: u32,
    /// Monte Carlo deals per hand class and node to estimate the EVs.
    #[arg(long, value_name = "N", default_value_t = LeakOptions::default().samples)]
    samples: u32,
}

pub fn run(args: &Args) -> ExitCode {
    let batch = parse_paths(&args.paths);
    let options = LeakOptions {
        min_sample: args.min_sample,
        samples: args.samples,
        ..LeakOptions::default()
    };
    let report = LeakReport::build(batch.hands(), &args.hero, &options);
    if report.hands() == 0 {
        eprintln!("error: no hand was dealt to the account given with --hero");
        return ExitCode::FAILURE;
    }
    print_report(&report, options.min_sample, batch.errors.len());
    ExitCode::SUCCESS
}

fn print_report(report: &LeakReport, min_sample: u32, errors: usize) {
    crate::hh::print_skipped(errors);
    println!(
        "{} hands read; your push (or call) frequency per node and stack bucket,",
        report.hands()
    );
    println!("next to the equilibrium of a spot where everyone has the bucket's middle stack.");
    println!(
        "Chips lost: EV of the equilibrium's play of each of your hands minus that of yours, \
         against equilibrium opponents."
    );
    println!("Fewer than {min_sample} decisions: inconclusive, listed after the others.");
    println!();
    for leak in report.leaks() {
        let action = leak.node.aggressive_action();
        let equilibrium = match leak.equilibrium_frequency {
            Some(frequency) => format!(
                "{:.1}% at equilibrium ({} BB each)",
                100.0 * frequency,
                leak.equilibrium_stack
            ),
            None => format!(
                "no decision at equilibrium ({} BB each)",
                leak.equilibrium_stack
            ),
        };
        let lost = leak.chips_lost.map_or_else(
            || "no chips lost estimated".to_owned(),
            // Sampling noise around 0 would print as "-0".
            |c| format!("{:.0} chips lost", if c.abs() < 0.5 { 0.0 } else { c }),
        );
        let flag = if leak.inconclusive {
            ", inconclusive"
        } else {
            ""
        };
        println!(
            "{}, {}: {action} {:.1}% on {} vs {equilibrium}, {lost}{flag}",
            leak.node,
            leak.bucket,
            100.0 * leak.hero_frequency,
            leak.sample,
        );
    }
}
