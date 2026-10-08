//! The emergency stop's sources: a file a shortcut creates, and signals.
//! The bot's halt on a raised stop is tested in `click.rs`.

use std::fs;
use std::path::PathBuf;

use nitro_bot::EmergencyStop;
use signal_hook::consts::SIGTERM;
use signal_hook::low_level::raise;

fn stop_file(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_file(&path);
    path
}

#[test]
fn the_stop_file_appearing_raises_the_stop_for_good() {
    let path = stop_file("stop-file-appears");
    let stop = EmergencyStop::with_file(&path);
    assert!(!stop.is_triggered());
    fs::write(&path, "").unwrap();
    assert!(stop.is_triggered());
    fs::remove_file(&path).unwrap();
    assert!(stop.is_triggered(), "the stop never comes down by itself");
}

#[test]
fn clones_share_the_stop() {
    let stop = EmergencyStop::new();
    let shared = stop.clone();
    shared.trigger();
    assert!(stop.is_triggered());
}

/// One signal per test process: a second one, the stop already raised,
/// ends the process (SIGINT is handled the same way).
#[test]
fn sigterm_raises_the_stop_instead_of_ending_the_process() {
    let stop = EmergencyStop::new();
    stop.on_signals().unwrap();
    assert!(!stop.is_triggered());
    raise(SIGTERM).unwrap();
    assert!(stop.is_triggered());
}
