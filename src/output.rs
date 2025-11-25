//! Output control for quiet mode and JSON output
//!
//! Provides a global quiet mode flag to suppress non-essential output.

use std::sync::atomic::{AtomicBool, Ordering};

/// Global quiet mode flag
static QUIET_MODE: AtomicBool = AtomicBool::new(false);

/// Enable quiet mode (suppresses informational output)
pub fn set_quiet(quiet: bool) {
    QUIET_MODE.store(quiet, Ordering::SeqCst);
}

/// Check if quiet mode is enabled
pub fn is_quiet() -> bool {
    QUIET_MODE.load(Ordering::SeqCst)
}

/// Print a message only if not in quiet mode
#[macro_export]
macro_rules! info_print {
    ($($arg:tt)*) => {
        if !$crate::output::is_quiet() {
            println!($($arg)*);
        }
    };
}

/// Print to stderr only if not in quiet mode (for warnings)
#[macro_export]
macro_rules! warn_print {
    ($($arg:tt)*) => {
        if !$crate::output::is_quiet() {
            eprintln!($($arg)*);
        }
    };
}
