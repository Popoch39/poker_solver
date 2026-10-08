//! `nitro hh`: parse Winamax hand histories and report what was read.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use nitro_hh::ohh::write_hand;
use nitro_hh::{Batch, ErrorKind, parse_paths, to_ohh};

/// Errors listed one per line before the rest are only counted.
const MAX_LISTED_ERRORS: usize = 20;

/// How the commands that read histories, but `nitro hh`, report what they
/// could not read.
pub fn print_skipped(errors: usize) {
    println!("{errors} files or hands skipped: other formats or unreadable (see `nitro hh`)");
}

pub fn run(paths: &[PathBuf], ohh: Option<&Path>) -> ExitCode {
    let batch = parse_paths(paths);
    print_report(&batch);
    if let Some(ohh) = ohh {
        match write_ohh(&batch, ohh) {
            Ok(count) => println!("{count} hands written to {}", ohh.display()),
            Err(err) => {
                eprintln!("error: cannot write {}: {err}", ohh.display());
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

fn write_ohh(batch: &Batch, path: &Path) -> io::Result<usize> {
    let mut file = BufWriter::new(File::create(path)?);
    let mut count = 0;
    for tournament in &batch.tournaments {
        for hand in &tournament.hands {
            write_hand(&mut file, to_ohh(hand, tournament.summary.as_ref()))?;
            count += 1;
        }
    }
    file.flush()?;
    Ok(count)
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
