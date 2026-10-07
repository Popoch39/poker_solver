//! Finding the local client's window, the only one the bot may look at.
//!
//! The window is identified by its exact title, [`TITLE`], and must also
//! carry the client's app_id, [`APP_ID`]: a browser tab or a terminal
//! titled the same is refused. Anything uncertain (no such window, two of
//! them) is refused too, never guessed.

use std::process::Command;

use nitro_local_client::{APP_ID, TITLE};
use serde::Deserialize;

/// A window as the compositor lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Window {
    pub address: String,
    pub title: String,
    /// The Wayland app_id (X11 class).
    pub class: String,
    /// The ext-foreign-toplevel identifier, what a per-window capture
    /// names the window by.
    #[serde(rename = "stableId", default)]
    pub stable_id: Option<String>,
}

/// Where the open windows come from.
pub trait WindowList {
    fn windows(&self) -> Result<Vec<Window>, TargetError>;
}

/// Hyprland's window list, from `hyprctl clients -j`.
pub struct Hyprctl;

impl WindowList for Hyprctl {
    fn windows(&self) -> Result<Vec<Window>, TargetError> {
        let output = Command::new("hyprctl")
            .args(["clients", "-j"])
            .output()
            .map_err(|e| TargetError::List(format!("cannot run hyprctl: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TargetError::List(format!("hyprctl failed: {stderr}")));
        }
        let json = String::from_utf8_lossy(&output.stdout);
        parse_hyprctl_clients(&json).map_err(|e| TargetError::List(e.to_string()))
    }
}

/// The windows of `hyprctl clients -j`.
pub fn parse_hyprctl_clients(json: &str) -> Result<Vec<Window>, serde_json::Error> {
    serde_json::from_str(json)
}

/// Why no window may be captured.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TargetError {
    #[error("cannot list the windows: {0}")]
    List(String),
    #[error("no window is titled {TITLE:?}: is the local client open?")]
    NotFound,
    #[error("{n} windows are titled {TITLE:?}; close all but one", n = .0)]
    Ambiguous(usize),
    #[error("the window titled {TITLE:?} belongs to {class:?}, not to {APP_ID:?}")]
    WrongApplication { class: String },
    #[error("the compositor gives no stable id for the local client's window")]
    NoStableId,
}

/// The local client's window: the only thing the bot may capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientWindow {
    window: Window,
    stable_id: String,
}

impl ClientWindow {
    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn stable_id(&self) -> &str {
        &self.stable_id
    }
}

/// The one window titled exactly [`TITLE`], provided it is the client's.
pub fn find_client_window(list: &dyn WindowList) -> Result<ClientWindow, TargetError> {
    let titled: Vec<Window> = list
        .windows()?
        .into_iter()
        .filter(|w| w.title == TITLE)
        .collect();
    // A look-alike is refused even next to the real client: something else
    // on the desktop pretends to be the table.
    if let Some(other) = titled.iter().find(|w| w.class != APP_ID) {
        return Err(TargetError::WrongApplication {
            class: other.class.clone(),
        });
    }
    let window = match <[Window; 1]>::try_from(titled) {
        Ok([window]) => window,
        Err(titled) if titled.is_empty() => return Err(TargetError::NotFound),
        Err(titled) => return Err(TargetError::Ambiguous(titled.len())),
    };
    let stable_id = window.stable_id.clone().ok_or(TargetError::NoStableId)?;
    Ok(ClientWindow { window, stable_id })
}
