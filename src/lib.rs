mod native_crash;
mod panic_report;
mod plugin;

pub use native_crash::install_native_crash_capture;
pub use panic_report::{PanicLocation, PanicReport};
pub use plugin::{CrashReport, CrashReporterPlugin, NativeCaptureFailurePolicy};

// ============================================================================================
// TEST UTILS
// ============================================================================================
/// `std::panic::set_hook` is process-global: any test that installs a hook,
/// or that panics while one might be active, must serialize through this lock.
#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::{Mutex, OnceLock};

    pub(crate) fn panic_hook_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
}
