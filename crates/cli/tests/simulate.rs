//! Smoke tests of the `nitro simulate` command.

use std::process::{Command, Output};

fn nitro(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nitro"))
        .args(args)
        .output()
        .expect("the nitro binary runs")
}

fn stdout(output: &Output) -> String {
    assert!(
        output.status.success(),
        "nitro failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}

const ARGS: [&str; 9] = [
    "simulate",
    "--buy-in",
    "1",
    "--games",
    "200",
    "--seed",
    "7",
    "--seats",
    "always-all-in,always-fold,random",
];

#[test]
fn simulate_reports_win_rate_roi_and_break_even_per_seat() {
    let out = stdout(&nitro(&ARGS));
    assert!(out.contains("200 games"), "{out}");
    assert!(out.contains("rake 7 %"), "{out}");
    assert!(out.contains("break-even win rate 35.84 %"), "{out}");
    for name in ["always-all-in", "always-fold", "random"] {
        let line = out.lines().find(|l| l.contains(name)).expect(name);
        assert!(line.contains("win rate"), "{line}");
        assert!(line.contains("ROI"), "{line}");
        assert!(line.contains("vs break-even"), "{line}");
    }
}

#[test]
fn simulate_is_reproducible_with_a_seed() {
    assert_eq!(stdout(&nitro(&ARGS)), stdout(&nitro(&ARGS)));
}

#[test]
fn simulate_accepts_micro_stakes_with_a_comma() {
    let out = stdout(&nitro(&[
        "simulate",
        "--buy-in",
        "0,25",
        "--games",
        "50",
        "--seats",
        "random,random,random",
    ]));
    assert!(out.contains("rake 8 %"), "{out}");
    assert!(out.contains("break-even win rate 36.23 %"), "{out}");
}

#[test]
fn simulate_rejects_an_unknown_buy_in_or_bot() {
    let output = nitro(&[
        "simulate",
        "--buy-in",
        "3",
        "--seats",
        "random,random,random",
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("buy-in"));
    let output = nitro(&[
        "simulate",
        "--buy-in",
        "1",
        "--seats",
        "random,random,shark",
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown bot"));
}
