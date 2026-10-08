//! Click injection: a left click at a point of the screen.

use serde::Deserialize;

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

/// The bounding box of every monitor, in layout coordinates: what an
/// absolute pointer device spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl ScreenArea {
    pub fn contains(&self, point: ScreenPoint) -> bool {
        let (dx, dy) = (
            i64::from(point.x) - i64::from(self.x),
            i64::from(point.y) - i64::from(self.y),
        );
        (0..i64::from(self.width)).contains(&dx) && (0..i64::from(self.height)).contains(&dy)
    }
}

/// A monitor of `hyprctl monitors -j`.
#[derive(Deserialize)]
struct Monitor {
    x: i32,
    y: i32,
    /// In physical pixels, before the transform.
    width: u32,
    height: u32,
    scale: f64,
    /// 1, 3, 5 and 7 turn it by a quarter.
    transform: u8,
    #[serde(default)]
    disabled: bool,
}

/// The screen of `hyprctl monitors -j`: its enabled monitors, at their
/// logical size.
pub fn parse_hyprctl_monitors(json: &str) -> Result<ScreenArea, InjectError> {
    let monitors: Vec<Monitor> =
        serde_json::from_str(json).map_err(|e| InjectError(format!("bad monitor list: {e}")))?;
    let boxes: Vec<(i64, i64, i64, i64)> = monitors
        .iter()
        .filter(|m| !m.disabled)
        .map(|m| {
            let logical = |pixels: u32| (f64::from(pixels) / m.scale).round() as i64;
            let (width, height) = match m.transform % 2 {
                0 => (logical(m.width), logical(m.height)),
                _ => (logical(m.height), logical(m.width)),
            };
            let (x, y) = (i64::from(m.x), i64::from(m.y));
            (x, y, x + width, y + height)
        })
        .collect();
    let left = boxes.iter().map(|b| b.0).min();
    let top = boxes.iter().map(|b| b.1).min();
    let right = boxes.iter().map(|b| b.2).max();
    let bottom = boxes.iter().map(|b| b.3).max();
    let (Some(left), Some(top), Some(right), Some(bottom)) = (left, top, right, bottom) else {
        return Err(InjectError("no monitor is enabled".to_owned()));
    };
    Ok(ScreenArea {
        x: fit(left)?,
        y: fit(top)?,
        width: fit(right - left)?,
        height: fit(bottom - top)?,
    })
}

fn fit<T: TryFrom<i64>>(value: i64) -> Result<T, InjectError> {
    T::try_from(value).map_err(|_| InjectError("the monitors lie too far out".to_owned()))
}
