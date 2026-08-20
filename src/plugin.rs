use crate::native_crash::install_native_crash_capture;
use crate::panic_report::{PanicReport, build_panic_report};
use bevy_app::{App, Plugin};
use std::fmt::Display;
use std::panic::{AssertUnwindSafe, catch_unwind, set_hook, take_hook};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread;

/// A captured crash, from either a Rust panic or a native OS-level crash.
#[derive(Debug, Clone)]
pub enum CrashReport {
    Panic(PanicReport),
    Native { minidump: Vec<u8>, path: PathBuf },
}

/// What to do if installing the native crash watcher itself fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeCaptureFailurePolicy {
    /// Panic immediately. Fails fast during development.
    #[default]
    Panic,
    /// Log to stderr and continue without native crash capture.
    /// Rust panics are still reported via `on_report`.
    Continue,
}

/// Captures panics and native crashes, forwarding both to `on_report`.
///
/// - Add this as the *first* plugin, before `DefaultPlugins`.
/// - `on_report` may fire from either the app process (panics) or the
///   separate watcher process (native crashes) — see `install_native_crash_capture`.
pub struct CrashReporterPlugin {
    on_report: Arc<dyn Fn(CrashReport) + Send + Sync>,
    native_capture_failure_policy: NativeCaptureFailurePolicy,
}

impl CrashReporterPlugin {
    pub fn new(on_report: impl Fn(CrashReport) + Send + Sync + 'static) -> Self {
        Self {
            on_report: Arc::new(on_report),
            native_capture_failure_policy: NativeCaptureFailurePolicy::default(),
        }
    }

    pub fn with_native_capture_failure_policy(
        mut self,
        policy: NativeCaptureFailurePolicy,
    ) -> Self {
        self.native_capture_failure_policy = policy;
        self
    }
}

impl Plugin for CrashReporterPlugin {
    fn build(&self, app: &mut App) {
        let native_cb = self.on_report.clone();
        let result = install_native_crash_capture(move |buffer, path| {
            native_cb(CrashReport::Native {
                minidump: buffer,
                path: path.to_path_buf(),
            });
        });
        if let Some(guard) =
            handle_native_capture_result(result, self.native_capture_failure_policy)
        {
            // Must outlive the app, or the watcher process is torn down immediately.
            app.insert_non_send(guard);
        }

        install_panic_hook(self.on_report.clone());
    }
}

/// Applies `policy` to a native-capture install result.
fn handle_native_capture_result<T, E: Display>(
    result: Result<T, E>,
    policy: NativeCaptureFailurePolicy,
) -> Option<T> {
    match result {
        Ok(guard) => Some(guard),
        Err(err) => match policy {
            NativeCaptureFailurePolicy::Panic => {
                panic!("failed to install native crash capture: {err}")
            }
            NativeCaptureFailurePolicy::Continue => {
                eprintln!(
                    "bevy_crash_reporter: failed to install native crash capture, continuing without it: {err}"
                );
                None
            }
        },
    }
}

struct PanicWorker {
    sender: SyncSender<CrashReport>,
}

impl PanicWorker {
    fn spawn(on_report: Arc<dyn Fn(CrashReport) + Send + Sync>) -> Self {
        let (sender, receiver) = sync_channel::<CrashReport>(16);

        thread::Builder::new()
            .name("bevy_crash_reporter-worker".into())
            .spawn(move || {
                for report in receiver {
                    if catch_unwind(AssertUnwindSafe(|| on_report(report))).is_err() {
                        eprintln!("bevy_crash_reporter: on_report callback panicked");
                    }
                }
            })
            .expect("failed to spawn bevy_crash_reporter worker thread");

        Self { sender }
    }

    /// Best-effort: called from inside a panic hook, so this must never
    /// block or retry. A full queue silently drops the report.
    fn report(&self, report: CrashReport) {
        let _ = self.sender.try_send(report);
    }
}

/// Chains onto whatever hook was previously installed, so panic output
/// (e.g. the default stderr printer) keeps working alongside `on_report`.
fn install_panic_hook(on_report: Arc<dyn Fn(CrashReport) + Send + Sync>) {
    let worker = PanicWorker::spawn(on_report);
    let previous_hook = take_hook();
    set_hook(Box::new(move |info| {
        previous_hook(info);
        worker.report(CrashReport::Panic(build_panic_report(info)));
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
    use std::sync::mpsc::sync_channel as test_sync_channel;
    use std::time::{Duration, Instant};

    fn wait_for<T>(mut poll: impl FnMut() -> Option<T>, timeout: Duration) -> Option<T> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(value) = poll() {
                return Some(value);
            }
            thread::sleep(Duration::from_millis(10));
        }
        None
    }

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

        let report = wait_for(|| captured.lock().unwrap().take(), Duration::from_secs(2))
            .expect("on_report should run");
        match report {
            CrashReport::Panic(report) => assert_eq!(report.message, "boom"),
            CrashReport::Native { .. } => panic!("expected Panic, got Native"),
        }
    }

    #[test]
    fn ok_result_returns_guard_regardless_of_policy() {
        let result: Result<u8, &str> = Ok(42);
        assert_eq!(
            handle_native_capture_result(result, NativeCaptureFailurePolicy::Continue),
            Some(42)
        );
    }

    #[test]
    fn continue_policy_returns_none_without_panicking() {
        let result: Result<(), &str> = Err("boom");
        let outcome = handle_native_capture_result(result, NativeCaptureFailurePolicy::Continue);
        assert!(outcome.is_none());
    }

    #[test]
    #[should_panic(expected = "failed to install native crash capture")]
    fn panic_policy_panics_on_failure() {
        let _guard = panic_hook_lock().lock().unwrap_or_else(|e| e.into_inner());

        let result: Result<(), &str> = Err("boom");
        handle_native_capture_result(result, NativeCaptureFailurePolicy::Panic);
    }

    #[test]
    fn on_report_panic_is_isolated_to_worker_thread() {
        let _guard = panic_hook_lock().lock().unwrap_or_else(|e| e.into_inner());

        let original_hook = take_hook();

        // Rendezvous so we know the worker thread actually ran (and panicked)
        // before this test returns, rather than racing past it.
        let (started_tx, started_rx) = test_sync_channel::<()>(0);
        install_panic_hook(Arc::new(move |_report| {
            let _ = started_tx.send(());
            panic!("on_report itself panicked");
        }));

        let result = catch_unwind(|| panic!("boom"));
        set_hook(original_hook);

        assert!(
            result.is_err(),
            "the original panic should still unwind normally"
        );
        started_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("worker thread never invoked on_report");
        thread::sleep(Duration::from_millis(50));
    }
}
