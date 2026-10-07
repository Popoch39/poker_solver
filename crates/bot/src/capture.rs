//! Capturing the local client's window.
//!
//! The capture is per window (ext-image-copy-capture on a foreign
//! toplevel, through `grim -T`), not a region of the screen
//! (wlr-screencopy): it holds the client's pixels only, even when another
//! window covers it, so nothing else on the desktop is ever read.

use std::process::Command;

use nitro_local_client::Frame;

use crate::window::{ClientWindow, TargetError, WindowList, find_client_window};

/// Why no frame was captured.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CaptureError {
    #[error(transparent)]
    Target(#[from] TargetError),
    #[error("cannot capture the window: {0}")]
    Grab(String),
    #[error("the local client's window changed during the capture")]
    WindowChanged,
}

/// Takes a picture of one window. It is only ever handed the local
/// client's.
pub trait Grabber {
    fn grab(&self, window: &ClientWindow) -> Result<Frame, CaptureError>;
}

/// `grim -T <stable id>`: one toplevel, in physical pixels.
pub struct Grim;

impl Grabber for Grim {
    fn grab(&self, window: &ClientWindow) -> Result<Frame, CaptureError> {
        // PNG on stdout, uncompressed: the fastest grim writes.
        let output = Command::new("grim")
            .args(["-T", window.stable_id(), "-t", "png", "-l", "0", "-"])
            .output()
            .map_err(|e| CaptureError::Grab(format!("cannot run grim: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CaptureError::Grab(format!("grim failed: {stderr}")));
        }
        Frame::decode_png(&output.stdout).map_err(|e| CaptureError::Grab(e.to_string()))
    }
}

/// A frame of the local client's window, found in `windows`.
pub fn capture_client(
    windows: &dyn WindowList,
    grabber: &dyn Grabber,
) -> Result<Frame, CaptureError> {
    let window = find_client_window(windows)?;
    let frame = grabber.grab(&window)?;
    // The client may have closed during the capture, and its id gone to
    // another window: the frame is only kept if the same window is still
    // the client's.
    if find_client_window(windows)? != window {
        return Err(CaptureError::WindowChanged);
    }
    Ok(frame)
}
