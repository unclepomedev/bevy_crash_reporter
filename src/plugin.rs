use crate::native_crash::install_native_crash_capture;
use crate::panic_report::{PanicReport, build_panic_report};
use bevy_app::{App, Plugin};
use std::panic;
use std::path::PathBuf;
use std::sync::Arc;

/// A captured crash, from either a Rust panic or a native OS-level crash.
#[derive(Debug, Clone)]
pub enum CrashReport {
    Panic(PanicReport),
    Native { minidump: Vec<u8>, path: PathBuf },
}

/// Captures panics and native crashes, forwarding both to `on_report`.
///
/// - Add this as the *first* plugin, before `DefaultPlugins`.
/// - `on_report` may fire from either the app process (panics) or the
///   separate watcher process (native crashes) — see `install_native_crash_capture`.
pub struct CrashReporterPlugin {
    on_report: Arc<dyn Fn(CrashReport) + Send + Sync>,
}

impl CrashReporterPlugin {
    pub fn new(on_report: impl Fn(CrashReport) + Send + Sync + 'static) -> Self {
        Self {
            on_report: Arc::new(on_report),
        }
    }
}

impl Plugin for CrashReporterPlugin {
    fn build(&self, app: &mut App) {
        let native_cb = self.on_report.clone();
        let guard = install_native_crash_capture(move |buffer, path| {
            native_cb(CrashReport::Native {
                minidump: buffer.to_vec(),
                path: path.to_path_buf(),
            });
        })
        .expect("failed to install native crash capture");
        // Must outlive the app, or the watcher process is torn down immediately.
        app.insert_non_send(guard);

        let panic_cb = self.on_report.clone();
        panic::set_hook(Box::new(move |info| {
            panic_cb(CrashReport::Panic(build_panic_report(info)));
        }));
    }
}

// ============================================================================================
// UNIT TESTS
// ============================================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    // Same rationale as panic_report's tests: std::panic::set_hook is
    // process-global, so serialize tests that swap it.
    fn panic_hook_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn panic_is_forwarded_as_crash_report() {
        let _guard = panic_hook_lock().lock().unwrap();

        let captured: Arc<Mutex<Option<CrashReport>>> = Arc::new(Mutex::new(None));
        let captured_clone = captured.clone();

        let previous_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            *captured_clone.lock().unwrap() = Some(CrashReport::Panic(build_panic_report(info)));
        }));

        let result = panic::catch_unwind(|| panic!("boom"));
        panic::set_hook(previous_hook);

        assert!(result.is_err());
        match captured.lock().unwrap().take().expect("hook should run") {
            CrashReport::Panic(report) => assert_eq!(report.message, "boom"),
            CrashReport::Native { .. } => panic!("expected Panic, got Native"),
        }
    }
}
