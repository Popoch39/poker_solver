//! `nitro-local-client --snapshot`: the window's frame, offscreen.

use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use nitro_local_client::{ClientConfig, Frame, LocalClient, Next};
use nitro_simulator::{SeatStrategy, Structure, TrivialBot};

#[test]
fn the_snapshot_is_the_frame_at_the_heros_first_decision() {
    let path = format!("{}/snapshot-seed-3.png", env!("CARGO_TARGET_TMPDIR"));
    let output = Command::new(env!("CARGO_BIN_EXE_nitro-local-client"))
        .args([
            "--seed",
            "3",
            "--bots",
            "random,always-all-in",
            "--snapshot",
            &path,
        ])
        .output()
        .expect("the client binary runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let snapshot = Frame::decode_png(&std::fs::read(&path).unwrap()).unwrap();

    let mut client = LocalClient::new(ClientConfig {
        structure: Structure::expresso_nitro(),
        seed: 3,
        bots: [TrivialBot::Random, TrivialBot::AlwaysAllIn]
            .map(|b| Arc::new(b) as Arc<dyn SeatStrategy>),
        bot_delay: Duration::ZERO,
        hand_over_delay: Duration::ZERO,
    });
    while let Next::After(_) = client.advance() {}
    assert_eq!(snapshot, client.frame());
}
