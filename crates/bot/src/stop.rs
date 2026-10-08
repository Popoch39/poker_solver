//! The emergency stop: once raised, the bot clicks nothing more.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::flag;

/// Raised by [`EmergencyStop::trigger`], by SIGINT or SIGTERM once
/// [`EmergencyStop::on_signals`] is set up, or by the stop file appearing.
/// It never comes down: a bot that was stopped is started again by hand.
///
/// Clones share the stop.
#[derive(Debug, Clone, Default)]
pub struct EmergencyStop {
    raised: Arc<AtomicBool>,
    file: Option<Arc<PathBuf>>,
}

impl EmergencyStop {
    pub fn new() -> Self {
        Self::default()
    }

    /// Also raised once `path` exists: a desktop shortcut can `touch` it.
    pub fn with_file(path: impl Into<PathBuf>) -> Self {
        Self {
            raised: Arc::default(),
            file: Some(Arc::new(path.into())),
        }
    }

    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref().map(PathBuf::as_path)
    }

    /// Raises the stop on SIGINT or SIGTERM instead of terminating the
    /// process. A second one terminates it, in case the bot does not come
    /// back to look at the stop.
    pub fn on_signals(&self) -> io::Result<()> {
        for signal in [SIGINT, SIGTERM] {
            // Registered first, so it sees the flag before this signal sets it.
            flag::register_conditional_shutdown(signal, 130, Arc::clone(&self.raised))?;
            flag::register(signal, Arc::clone(&self.raised))?;
        }
        Ok(())
    }

    pub fn trigger(&self) {
        self.raised.store(true, Ordering::SeqCst);
    }

    pub fn is_triggered(&self) -> bool {
        if self.raised.load(Ordering::SeqCst) {
            return true;
        }
        if self.file.as_deref().is_some_and(|path| path.exists()) {
            self.trigger();
            return true;
        }
        false
    }
}
