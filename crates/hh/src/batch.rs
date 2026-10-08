use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use rs_poker::open_hand_history::HandReader;

use crate::hand::Hand;
use crate::ohh_convert::from_ohh;
use crate::parse::{ErrorKind, ParseError, WINAMAX_HEADER, parse_hands};
use crate::summary::{SUMMARY_HEADER, Summary, parse_summary};

/// Everything read from a file or a folder of Winamax files.
#[derive(Clone, Debug, Default)]
pub struct Batch {
    /// Tournaments in the order their first file was read.
    pub tournaments: Vec<Tournament>,
    pub errors: Vec<FileError>,
    /// Number of files read.
    pub files: usize,
}

impl Batch {
    pub fn hands(&self) -> impl Iterator<Item = &Hand> {
        self.tournaments.iter().flat_map(|t| &t.hands)
    }
}

/// The hands and the summary of one tournament, as far as the files go.
#[derive(Clone, Debug)]
pub struct Tournament {
    pub id: String,
    pub summary: Option<Summary>,
    pub hands: Vec<Hand>,
}

#[derive(Clone, Debug)]
pub struct FileError {
    pub path: PathBuf,
    pub error: ParseError,
}

/// Parses a Winamax hand-history or summary file (`.txt`), an Open Hand
/// History file (`.ohh`), or every such file under a folder. Unreadable
/// files and bad hands are listed in [`Batch::errors`] and do not stop the
/// rest.
pub fn parse_path(path: &Path) -> Batch {
    parse_paths([path])
}

/// Parses every path like [`parse_path`], into one batch: a tournament whose
/// files lie under several paths is still one tournament.
pub fn parse_paths<P: AsRef<Path>>(paths: impl IntoIterator<Item = P>) -> Batch {
    let mut files = Vec::new();
    let mut batch = Batch::default();
    for path in paths {
        let path = path.as_ref();
        if path.is_dir() {
            let mut found = Vec::new();
            collect_files(path, &mut found, &mut batch.errors);
            found.sort();
            files.extend(found);
        } else {
            files.push(path.to_path_buf());
        }
    }

    let mut index_of: HashMap<String, usize> = HashMap::new();
    let mut tournament = |batch: &mut Batch, id: &str| -> usize {
        *index_of.entry(id.to_string()).or_insert_with(|| {
            batch.tournaments.push(Tournament {
                id: id.to_string(),
                summary: None,
                hands: Vec::new(),
            });
            batch.tournaments.len() - 1
        })
    };
    for path in files {
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                batch.errors.push(whole_file_error(&path, error));
                continue;
            }
        };
        batch.files += 1;
        let hands = if is_ohh(&path) {
            read_ohh(text)
        } else if !text.starts_with(WINAMAX_HEADER) {
            // Not even Winamax: a stray file in the folder, not a damaged one.
            batch.errors.push(FileError {
                path,
                error: ParseError {
                    line: 0,
                    kind: ErrorKind::Unsupported,
                    reason: "not a Winamax hand history or summary".into(),
                },
            });
            continue;
        } else if text.starts_with(SUMMARY_HEADER) {
            match parse_summary(&text) {
                Ok(summary) => {
                    let index = tournament(&mut batch, &summary.tournament_id);
                    batch.tournaments[index].summary = Some(summary);
                }
                Err(error) => batch.errors.push(FileError { path, error }),
            }
            continue;
        } else {
            parse_hands(&text)
        };
        for result in hands {
            match result {
                Ok(hand) => {
                    let id = hand.tournament.as_ref().map_or("", |t| t.id.as_str());
                    let index = tournament(&mut batch, id);
                    batch.tournaments[index].hands.push(hand);
                }
                Err(error) => batch.errors.push(FileError {
                    path: path.clone(),
                    error,
                }),
            }
        }
    }
    batch
}

fn is_ohh(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "ohh")
}

/// The hands of an Open Hand History file, such as the simulator writes.
fn read_ohh(text: String) -> Vec<Result<Hand, ParseError>> {
    HandReader::from_reader(Cursor::new(text.into_bytes()))
        .enumerate()
        .map(|(index, history)| {
            let malformed = |reason: String| ParseError {
                line: 0,
                kind: ErrorKind::Malformed,
                reason: format!("OHH hand {}: {reason}", index + 1),
            };
            let history = history.map_err(|e| malformed(e.to_string()))?;
            from_ohh(&history).map_err(|e| malformed(e.to_string()))
        })
        .collect()
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>, errors: &mut Vec<FileError>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(whole_file_error(dir, error));
            return;
        }
    };
    for entry in entries {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(error) => {
                errors.push(whole_file_error(dir, error));
                continue;
            }
        };
        if path.is_dir() {
            collect_files(&path, files, errors);
        } else if is_ohh(&path) || path.extension().is_some_and(|ext| ext == "txt") {
            files.push(path);
        }
    }
}

fn whole_file_error(path: &Path, error: std::io::Error) -> FileError {
    FileError {
        path: path.to_path_buf(),
        error: ParseError {
            line: 0,
            kind: ErrorKind::Malformed,
            reason: format!("cannot read: {error}"),
        },
    }
}
