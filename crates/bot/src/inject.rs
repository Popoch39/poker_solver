//! Click injection: a left click at a point of the screen.

/// A point of the screen, in the compositor's layout coordinates (logical
/// pixels across all monitors, as `hyprctl` gives window positions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,
}

/// Why a click was not injected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("cannot inject the click: {0}")]
pub struct InjectError(pub String);

/// Clicks on the screen. Only ever handed points of the local client's
/// window, checked to have the focus just before.
pub trait Injector {
    /// Moves the pointer to `at` and clicks the left button there.
    fn click(&mut self, at: ScreenPoint) -> Result<(), InjectError>;
}
