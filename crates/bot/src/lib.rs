//! The clicker bot: it finds the local client's window, captures it, reads
//! the table from its pixels, decides with the solver's hero and clicks the
//! client's buttons.
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
//! - **Action** ([`ClickerBot`]): the state read becomes the hero's
//!   [`SeatView`](nitro_simulator::SeatView) ([`TableState::seat_view`]),
//!   any seat strategy decides (the solver's hero,
//!   [`SolverHero`](nitro_simulator::SolverHero), equilibrium or exploit,
//!   with its cache and fallback policy), and the decision is clicked on
//!   its [`LAYOUT`](nitro_local_client::LAYOUT) button. The focus
//!   ([`Focus`]), the window list and the click [`Injector`] are
//!   injectable; [`play_live`] loops capture → read → turn.
//!
//! [`measure`] plays hands on the client offscreen and compares what is
//! read with the simulator's view of the table: the read error rate.
//! [`play_offscreen`] plays whole tournaments with the bot on the client
//! offscreen, its clicks forwarded to the client's click handler, and the
//! same games with the hero directly in the simulator: the concordance of
//! the bot's decisions with the hero's, and the two win rates.
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
//! # Clicking only on the local client
//!
//! Before every click, [`ClickerBot::turn`] checks that exactly one window
//! is titled like the client and is the client's ([`find_client_window`]),
//! that it is the focused window (`hyprctl activewindow -j`: same address,
//! title and class), at its own unscaled size. Otherwise nothing is
//! clicked and the bot stops. The button's window point is mapped to the
//! screen by the focused window's position: `at` + the button's centre.
//!
//! # Injection: uinput
//!
//! [`Uinput`] creates a virtual absolute pointer (X/Y axes over the whole
//! monitor layout, [`ScreenArea`], and a left button). It moves to the
//! point, then presses and releases. `/dev/uinput` belongs to root
//! (`crw------- root`): to run the bot as the desktop's user, a udev rule
//! hands the device to the logged-in seat user (not applied by this
//! crate), for example `/etc/udev/rules.d/70-nitro-bot-uinput.rules`:
//!
//! ```text
//! KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput"
//! ```
//!
//! then `sudo udevadm control --reload && sudo udevadm trigger
//! /dev/uinput` (or `sudo modprobe -r uinput && sudo modprobe uinput`).
//! `uaccess` gives access to the user of the active local session only,
//! unlike a world- or group-writable node. Any process of that user can
//! then inject input: remove the rule when the bot is not in use.
//!
//! The wlr virtual-pointer protocol would need no root, but it is not
//! implemented: the spec asks for uinput, the udev rule above already runs
//! it without root, and a second backend would double the untested live
//! code. The [`Injector`] trait leaves room for it.
//!
//! # Emergency stop
//!
//! An [`EmergencyStop`], checked before every click and again between the
//! pointer's move and its press, is raised by SIGINT (Ctrl-C) or SIGTERM
//! (a second one ends the process), or by the stop file appearing
//! (`$XDG_RUNTIME_DIR/nitro-bot.stop` by default). It never comes down:
//! `play` refuses to start while the stop file exists. A desktop shortcut
//! can raise it; with Omarchy's Lua bindings (untried):
//!
//! ```lua
//! o.bind("SUPER + CTRL + ESCAPE", "Stop the clicker bot", "touch $XDG_RUNTIME_DIR/nitro-bot.stop")
//! ```
//!
//! # Running
//!
//! ```text
//! cargo run --release -p nitro-bot -- measure --hands 1000 --seed 1 --bots random,random
//! cargo run --release -p nitro-bot -- offscreen --games 1000 --histories data/nitrovariance
//! cargo run --release -p nitro-bot -- offscreen --games 1000 --hero exploit --histories data/nitrovariance
//! cargo run --release -p nitro-bot -- read crates/bot/tests/captures/shown-down.png
//! cargo run --release -p nitro-bot -- read            # the live client window
//! cargo run --release -p nitro-bot -- capture table.png
//! cargo run --release -p nitro-bot -- play            # clicks on the live client
//! ```
//!
//! `play` needs the local client open and focused, Hyprland, `grim` and a
//! writable `/dev/uinput`. It has not been run live.

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
