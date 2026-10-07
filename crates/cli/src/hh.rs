//! `nitro hh`: parse Winamax hand histories and report what was read.

use std::path::PathBuf;
use std::process::ExitCode;

use nitro_hh::{Batch, ErrorKind, parse_path};

/// Errors listed one per line before the rest are only counted.
const MAX_LISTED_ERRORS: usize = 20;

pub fn run(paths: &[PathBuf]) -> ExitCode {
    let mut batch = Batch::default();
    for path in paths {
        let parsed = parse_path(path);
        batch.files += parsed.files;
        batch.tournaments.extend(parsed.tournaments);
        batch.errors.extend(parsed.errors);
    }
    print_report(&batch);
    ExitCode::SUCCESS
}

fn print_report(batch: &Batch) {
    let with_summary = batch
        .tournaments
        .iter()
        .filter(|t| t.summary.is_some())
        .count();
    let count = |kind| batch.errors.iter().filter(|e| e.error.kind == kind).count();
    println!("{} files", batch.files);
    println!(
        "{} tournaments ({with_summary} with a summary)",
        batch.tournaments.len()
    );
    println!("{} hands", batch.hands().count());
    println!(
        "{} errors ({} malformed, {} unsupported)",
        batch.errors.len(),
        count(ErrorKind::Malformed),
        count(ErrorKind::Unsupported)
    );
    // Malformed hands first: other formats are expected in a real history folder.
    let listed = batch
        .errors
        .iter()
        .filter(|e| e.error.kind == ErrorKind::Malformed)
        .chain(
            batch
                .errors
                .iter()
                .filter(|e| e.error.kind == ErrorKind::Unsupported),
        );
    for error in listed.take(MAX_LISTED_ERRORS) {
        println!("  {}: {}", error.path.display(), error.error);
    }
    if batch.errors.len() > MAX_LISTED_ERRORS {
        println!("  … and {} more", batch.errors.len() - MAX_LISTED_ERRORS);
    }
}
