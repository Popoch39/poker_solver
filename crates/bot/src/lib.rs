//! The clicker bot's vision: it finds the local client's window, captures
//! it and reads the table from its pixels.
//!
//! Two independent halves:
//!
//! - **Targeting and capture** ([`find_client_window`], [`capture_client`]):
//!   the only window the bot may look at is the one titled exactly
//!   [`nitro_local_client::TITLE`] with the client's app_id; anything else,
//!   or any doubt, is refused. The window list ([`WindowList`], Hyprland's
//!   [`Hyprctl`]) and the grabber ([`Grabber`], [`Grim`]) are injectable.
//! - **Recognition** ([`TableReader`]): a frame of the client becomes a
//!   [`TableState`], by template matching on the client's fixed layout with
//!   templates taken from the client's own renderer. Pure: no screen
//!   needed.
//!
//! [`measure`] plays hands on the client offscreen and compares what is
//! read with the simulator's view of the table: the read error rate.
//!
//! # Capture
//!
//! The capture is per window: `grim -T <stable id>`, which uses
//! ext-image-copy-capture on the client's foreign toplevel. wlr-screencopy
//! only captures outputs or screen regions, which would also catch any
//! window lying over the table; a per-window capture holds the client's
//! pixels only. The frame is read only at the client's size, so the window
//! must float unscaled on a scale-1 monitor, fully opaque: see the
//! Hyprland rules in [`nitro_local_client`].
//!
//! # Running
//!
//! ```text
//! cargo run --release -p nitro-bot -- measure --hands 1000 --seed 1 --bots random,random
//! cargo run --release -p nitro-bot -- read crates/bot/tests/captures/shown-down.png
//! cargo run --release -p nitro-bot -- read            # the live client window
//! cargo run --release -p nitro-bot -- capture table.png
//! ```

mod capture;
mod clicker;
mod inject;
mod live;
mod measure;
mod offscreen;
mod read;
mod state;
mod stop;
mod text;
mod uinput;
mod window;

pub use capture::{CaptureError, Grabber, Grim, capture_client};
pub use clicker::{BotError, ClickerBot, Turn};
pub use inject::{InjectError, Injector, ScreenArea, ScreenPoint, parse_hyprctl_monitors};
pub use live::{LiveError, Pace, play_live};
pub use measure::{MeasureConfig, Measurement, Misread, measure};
pub use offscreen::{Discordance, OffscreenReport, play_offscreen};
pub use read::{ReadError, TableReader};
pub use state::{Cards, SeatState, TableState};
pub use stop::EmergencyStop;
pub use uinput::Uinput;
pub use window::{
    ActiveWindow, ClientWindow, Focus, Hyprctl, TargetError, Window, WindowList,
    find_client_window, parse_hyprctl_activewindow, parse_hyprctl_clients,
};
