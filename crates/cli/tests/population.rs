//! Smoke tests of the `nitro population` command.
//!
//! The fixture holds three hands: a BTN min-raise by the account owner, then
//! two heads-up hands where an opponent min-raises from the SB and an
//! opponent calls the account owner's push from the BB with K♣J♥.

use std::path::PathBuf;
use std::process::{Command, Output};

fn nitro(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nitro"))
        .args(args)
        .output()
        .expect("the nitro binary runs")
}

fn fixtures() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../hh/tests/fixtures")
        .to_str()
        .unwrap()
        .to_owned()
}

#[test]
fn population_prints_frequencies_per_node_and_stack_bucket() {
    let output = nitro(&["population", &fixtures()]);
    assert!(output.status.success());
    let out = String::from_utf8(output.stdout).unwrap();
    assert!(
        out.contains("3 hands: 1 in the push/fold tree, 2 off it"),
        "{out}"
    );
    assert!(
        out.contains("BB vs SB push, 2–4 BB: call 100.0% on 1 (1 shown)"),
        "{out}"
    );
    assert!(
        out.contains("SB: limp 0, min-raise 1, raise 0, other 0"),
        "{out}"
    );
}

#[test]
fn population_prints_the_range_estimated_from_showdowns() {
    let output = nitro(&["population", &fixtures(), "--range", "bb-vs-sb-push"]);
    assert!(output.status.success());
    let out = String::from_utf8(output.stdout).unwrap();
    assert!(
        out.contains("BB vs SB push, 2–4 BB: call range estimated from 1 shown hands"),
        "{out}"
    );
    // K♣J♥ is KJo: offsuit hands sit below the diagonal, row J, column K.
    let row_j = out
        .lines()
        .find(|l| l.trim_start().starts_with('J'))
        .unwrap();
    assert_eq!(
        row_j.split_whitespace().collect::<Vec<_>>(),
        [
            "J", ".", "100", ".", ".", ".", ".", ".", ".", ".", ".", ".", ".", "."
        ],
        "{out}"
    );
}

#[test]
fn population_output_names_no_player() {
    let fixtures = fixtures();
    for options in [&[][..], &["--range", "bb-vs-sb-push"]] {
        let args: Vec<&str> = ["population", &fixtures]
            .into_iter()
            .chain(options.iter().copied())
            .collect();
        let out = String::from_utf8(nitro(&args).stdout).unwrap();
        for name in ["villain", "hero"] {
            assert!(!out.contains(name), "{name} in {out}");
        }
    }
}
