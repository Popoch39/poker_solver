//! The clicker bot's vision.

mod capture;
mod measure;
mod read;
mod state;
mod text;
mod window;

pub use capture::{CaptureError, Grabber, Grim, capture_client};
pub use measure::{MeasureConfig, Measurement, Misread, measure};
pub use read::{ReadError, TableReader};
pub use state::{Cards, SeatState, TableState};
pub use window::{
    ClientWindow, Hyprctl, TargetError, Window, WindowList, find_client_window,
    parse_hyprctl_clients,
};
