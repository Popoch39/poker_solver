//! The local client: an Expresso Nitro table where a human plays the
//! simulator's bots, and the only table the clicker bot may play on.
//!
//! A display and input layer only: [`LocalClient`] steps a simulator game
//! whose hero seat is external, draws [`nitro_simulator::TableView`] with
//! [`render`], and turns clicks on [`LAYOUT`]'s buttons into the hero's
//! legal decisions. The frame is the same on screen and offscreen, and the
//! layout never moves, so the clicker bot can reuse both.
//!
//! # Running
//!
//! ```text
//! cargo run --release -p nitro-local-client -- --seed 7 --bots random,always-all-in
//! cargo run --release -p nitro-local-client -- --seed 7 --snapshot table.png
//! ```
//!
//! The first opens the window; the second renders the table at the hero's
//! first decision to a PNG, without a window (the same frame, pixel for
//! pixel).
//!
//! # Window
//!
//! Title [`TITLE`], Wayland app_id (X11 class) [`APP_ID`], fixed inner size
//! of [`LAYOUT`]`.window` (960×640) in physical pixels, not resizable.
//!
//! # Hyprland
//!
//! Hyprland tiles windows (resizing them), and Omarchy makes them slightly
//! transparent and dims inactive ones: all of it changes the captured
//! pixels. The window must float at its own size, fully opaque and never
//! dimmed, on a monitor at scale 1 (at another scale the compositor
//! resamples the frame). With the Lua configuration of Hyprland 0.56 under
//! Omarchy, for example in a file under `~/.config/hypr/`:
//!
//! ```lua
//! o.window({ class = "^nitro-local-client$", title = "^Nitro local client$" }, {
//!   float = true,
//!   size = { "960", "640" },
//!   tag = "-default-opacity",
//!   opacity = "1 1",
//!   no_dim = true,
//! })
//! ```
//!
//! The rule fields are those of Omarchy's own webcam-overlay rule
//! (`/usr/share/omarchy/default/hypr/apps/webcam-overlay.lua`); the rule
//! itself has not been tried. Move the window to a scale-1 monitor by hand
//! (or with a `monitor` rule, also untried).
//!
//! With the older `hyprland.conf` syntax (untried):
//!
//! ```text
//! windowrule = float, class:^(nitro-local-client)$
//! windowrule = size 960 640, class:^(nitro-local-client)$
//! windowrule = opacity 1.0 override 1.0 override, class:^(nitro-local-client)$
//! windowrule = nodim, class:^(nitro-local-client)$
//! ```

mod client;
mod font;
mod layout;
mod render;

pub use client::{ClientConfig, HERO, LocalClient, Next};
pub use layout::{CARD_HEIGHT, CARD_WIDTH, LAYOUT, Layout, Rect, SeatLayout, TEXT_HEIGHT};
pub use render::{Frame, card_back, card_sprite, chips, render};

/// The window title.
pub const TITLE: &str = "Nitro local client";
/// The Wayland app_id and X11 class, what window rules match on.
pub const APP_ID: &str = "nitro-local-client";
