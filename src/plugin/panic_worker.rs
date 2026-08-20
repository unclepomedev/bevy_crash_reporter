use super::report::CrashReport;
use crate::panic_report::build_panic_report;
use std::panic::{AssertUnwindSafe, catch_unwind, set_hook, take_hook};
use std::sync::Arc;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread;

const REPORT_QUEUE_CAPACITY: usize = 16;
const WORKER_THREAD_NAME: &str = "bevy_crash_reporter-worker";

struct PanicWorker {
    sender: SyncSender<CrashReport>,
}

impl PanicWorker {
    fn spawn(on_report: Arc<dyn Fn(CrashReport) + Send + Sync>) -> Self {
        let (sender, receiver) = sync_channel::<CrashReport>(REPORT_QUEUE_CAPACITY);

        thread::Builder::new()
            .name(WORKER_THREAD_NAME.into())
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

    /// Called from inside a panic hook: must never block or retry. A full
    /// queue silently drops the report.
    fn report(&self, report: CrashReport) {
        let _ = self.sender.try_send(report);
    }
}

/// Chains onto whatever hook was previously installed, so panic output
/// (e.g. the default stderr printer) keeps working alongside `on_report`.
pub(super) fn install_panic_hook(on_report: Arc<dyn Fn(CrashReport) + Send + Sync>) {
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
    fn on_report_panic_is_isolated_to_worker_thread() {
        let _guard = panic_hook_lock().lock().unwrap_or_else(|e| e.into_inner());

        let original_hook = take_hook();

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
