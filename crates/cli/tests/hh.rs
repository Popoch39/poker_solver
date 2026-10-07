//! Smoke tests of the `nitro hh` command.

use std::path::PathBuf;
use std::process::{Command, Output};

fn nitro(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nitro"))
        .args(args)
        .output()
        .expect("the nitro binary runs")
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../hh/tests/fixtures")
}

#[test]
fn hh_summarises_a_folder() {
    let output = nitro(&["hh", fixtures().to_str().unwrap()]);
    assert!(output.status.success());
    let out = String::from_utf8(output.stdout).unwrap();
    assert!(out.contains("2 files"), "{out}");
    assert!(out.contains("1 tournaments (1 with a summary)"), "{out}");
    assert!(out.contains("3 hands"), "{out}");
    assert!(out.contains("0 errors"), "{out}");
}
