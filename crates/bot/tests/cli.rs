//! `nitro-bot`, offscreen: measuring and reading a saved frame.

use std::path::PathBuf;
use std::process::{Command, Output};

fn nitro_bot(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nitro-bot"))
        .args(args)
        .output()
        .expect("the bot binary runs")
}

#[test]
fn measure_reports_the_error_rate_over_n_hands() {
    let output = nitro_bot(&["measure", "--hands", "20", "--seed", "4"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("20 hands"), "{stdout}");
    assert!(stdout.contains("0 misread"), "{stdout}");
    assert!(stdout.contains("error rate 0.00 %"), "{stdout}");
}

#[test]
fn offscreen_compares_the_bot_with_the_hero_simulated_directly() {
    let output = nitro_bot(&[
        "offscreen",
        "--games",
        "6",
        "--seed",
        "2",
        "--iterations",
        "100",
        "--target",
        "50",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("6 games"), "{stdout}");
    assert!(stdout.contains("concordance 100.00 %"), "{stdout}");
    assert!(stdout.contains("bot win rate"), "{stdout}");
    assert!(stdout.contains("hero simulated"), "{stdout}");
}

#[test]
fn the_exploit_needs_hand_histories() {
    let output = nitro_bot(&["offscreen", "--hero", "exploit"]);
    assert!(!output.status.success());
}

#[test]
fn play_refuses_to_start_while_the_stop_file_exists() {
    let stop = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cli-stop-file");
    std::fs::write(&stop, "").unwrap();
    let output = nitro_bot(&["play", "--stop-file", stop.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("stop file"), "{stderr}");
}

#[test]
fn read_prints_the_state_of_a_saved_frame() {
    let png = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/captures/shown-down.png");
    let output = nitro_bot(&["read", png]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("pot: 900.0"), "{stdout}");
    assert!(stdout.contains("Card(Qc)"), "{stdout}");
}

#[test]
fn read_fails_on_a_missing_frame() {
    let png = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/captures/missing.png");
    let output = nitro_bot(&["read", png]);
    assert!(!output.status.success());
}
