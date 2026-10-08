//! Smoke tests of the `nitro versus` command, on the parser's fixtures as
//! the population.

use std::path::PathBuf;
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

fn fixtures() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../hh/tests/fixtures")
        .to_str()
        .unwrap()
        .to_owned()
}

/// A scratch folder under the build directory, emptied.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Few games and coarse solves: the command's output, not its precision.
fn versus(extra: &[&str]) -> Output {
    let fixtures = fixtures();
    let mut args = vec![
        "versus",
        &fixtures,
        "--buy-in",
        "1",
        "--games",
        "2",
        "--seed",
        "3",
        "--iterations",
        "20",
    ];
    args.extend_from_slice(extra);
    nitro(&args)
}

#[test]
fn versus_compares_equilibrium_and_exploit_against_the_population() {
    let out = stdout(&versus(&[]));
    assert!(out.contains("2 games"), "{out}");
    assert!(out.contains("rake 7 %"), "{out}");
    assert!(out.contains("break-even win rate 35.84 %"), "{out}");
    for hero in ["solver-equilibrium", "solver-exploit"] {
        let line = out.lines().find(|l| l.starts_with(hero)).expect(hero);
        assert!(line.contains("win rate"), "{line}");
        assert!(line.contains("ROI"), "{line}");
        assert!(line.contains("vs break-even"), "{line}");
        assert!(
            ["above break-even", "below break-even", "undecided"]
                .iter()
                .any(|v| line.contains(v)),
            "{line}"
        );
    }
    assert!(out.contains("exploit - equilibrium: win rate"), "{out}");
    assert!(out.contains("spots solved"), "{out}");
}

#[test]
fn versus_plays_one_hero_strategy_on_demand_and_reproduces_with_a_seed() {
    let out = stdout(&versus(&["--hero", "equilibrium"]));
    assert!(out.contains("solver-equilibrium"), "{out}");
    assert!(!out.contains("solver-exploit"), "{out}");
    let again = stdout(&versus(&["--hero", "equilibrium"]));
    let results = |out: &str| -> Vec<String> {
        out.lines()
            .filter(|l| l.starts_with("solver-") && l.contains("win rate"))
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(results(&out), results(&again));
}

#[test]
fn versus_exports_the_hands_as_open_hand_history_that_the_parser_reads() {
    let dir = scratch("versus-ohh");
    let dir_arg = dir.to_str().unwrap();
    stdout(&versus(&[
        "--hero",
        "exploit",
        "--ohh",
        dir_arg,
        "--ohh-games",
        "2",
    ]));
    let file = dir.join("solver-exploit.ohh");
    assert!(file.is_file(), "{}", file.display());

    let out = stdout(&nitro(&["hh", file.to_str().unwrap()]));
    assert!(out.contains("2 tournaments"), "{out}");
    assert!(out.contains("0 errors"), "{out}");
}

#[test]
fn versus_rejects_an_unknown_hero_strategy() {
    let output = versus(&["--hero", "gto"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("equilibrium"));
}
