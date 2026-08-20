use crate::native_crash::install_native_crash_capture;
use crate::panic_report::{PanicReport, build_panic_report};
use bevy_app::{App, Plugin};
use std::panic::{set_hook, take_hook};
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

        install_panic_hook(self.on_report.clone());
    }
}

/// Chains onto whatever hook was previously installed, so panic output
/// (e.g. the default stderr printer) keeps working alongside `on_report`.
fn install_panic_hook(on_report: Arc<dyn Fn(CrashReport) + Send + Sync>) {
    let previous_hook = take_hook();
    set_hook(Box::new(move |info| {
        previous_hook(info);
        on_report(CrashReport::Panic(build_panic_report(info)));
    }));
}

// ============================================================================================
// UNIT TESTS
// ============================================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::panic_hook_lock;
    use std::panic::catch_unwind;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn panic_hook_chains_previous_hook_and_on_report() {
        let _guard = panic_hook_lock().lock().unwrap_or_else(|e| e.into_inner());

        let original_hook = take_hook();

        let previous_hook_called = Arc::new(AtomicBool::new(false));
        let previous_hook_called_clone = previous_hook_called.clone();
        set_hook(Box::new(move |_info| {
            previous_hook_called_clone.store(true, Ordering::SeqCst);
        }));

        let captured: Arc<Mutex<Option<CrashReport>>> = Arc::new(Mutex::new(None));
        let captured_clone = captured.clone();
        install_panic_hook(Arc::new(move |report| {
            *captured_clone.lock().unwrap() = Some(report);
        }));

        let result = catch_unwind(|| panic!("boom"));
        set_hook(original_hook);

        assert!(result.is_err());
        assert!(
            previous_hook_called.load(Ordering::SeqCst),
            "previous hook should still run"
        );
        match captured
            .lock()
            .unwrap()
            .take()
            .expect("on_report should run")
        {
            CrashReport::Panic(report) => assert_eq!(report.message, "boom"),
            CrashReport::Native { .. } => panic!("expected Panic, got Native"),
        }
    }
}
