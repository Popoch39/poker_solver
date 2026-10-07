//! The measurement mode: the state read from each frame against the
//! simulator's own view of the table.

use std::sync::Arc;

use nitro_bot::{MeasureConfig, ReadError, TableReader, measure};
use nitro_simulator::{SeatStrategy, TrivialBot};

fn config(hands: u32) -> MeasureConfig {
    MeasureConfig {
        hands,
        seed: 11,
        bots: [TrivialBot::Random, TrivialBot::AlwaysAllIn]
            .map(|b| Arc::new(b) as Arc<dyn SeatStrategy>),
    }
}

#[test]
fn the_reader_reads_every_state_of_many_hands() {
    let reader = TableReader::new();
    let measurement = measure(&config(60), |frame| reader.read(frame));
    assert_eq!(measurement.hands, 60);
    assert!(measurement.states > 120, "{measurement:?}");
    assert_eq!(measurement.misread, 0, "{measurement:?}");
    assert_eq!(measurement.error_rate(), 0.0);
    assert!(measurement.first_misread.is_none());
}

#[test]
fn counts_the_states_a_reader_gets_wrong() {
    let reader = TableReader::new();
    // Misreads the pot on the flop only.
    let measurement = measure(&config(60), |frame| {
        let mut state = reader.read(frame)?;
        if state.board.len() == 3 {
            state.pot += 1.0;
        }
        Ok(state)
    });
    assert!(measurement.misread > 0, "{measurement:?}");
    assert!(measurement.misread < measurement.states, "{measurement:?}");
    assert_eq!(
        measurement.error_rate(),
        f64::from(measurement.misread) / f64::from(measurement.states)
    );
    let misread = measurement.first_misread.expect("a misread is kept");
    let read = misread.read.expect("read, but wrong");
    assert_eq!(read.pot, misread.expected.pot + 1.0);
}

#[test]
fn a_frame_that_cannot_be_read_is_a_misread() {
    let measurement = measure(&config(5), |_| {
        Err(ReadError::Unreadable("anything".to_owned()))
    });
    assert_eq!(measurement.hands, 5);
    assert_eq!(measurement.misread, measurement.states);
    assert_eq!(measurement.error_rate(), 1.0);
}
