use super::report::{CrashKind, CrashReport, ReportAssembler};
use crate::panic_report::build_panic_report;
use std::panic::{AssertUnwindSafe, catch_unwind, set_hook, take_hook};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread;

const REPORT_QUEUE_CAPACITY: usize = 16;
const WORKER_THREAD_NAME: &str = "bevy_crash_capture-worker";

type ReportCallback = Arc<dyn Fn(CrashReport) + Send + Sync>;

// Process-global: `std::panic::set_hook` is process-wide, so multiple
// `App`s in the same process each with their own `CrashCapturePlugin`
// must share a single installed hook and fan out to every registered callback.
static CALLBACKS: Mutex<Vec<ReportCallback>> = Mutex::new(Vec::new());
static HOOK_INSTALLED: Mutex<bool> = Mutex::new(false);

/// Registers `on_report` and, on the first call in this process, chains a
/// panic hook that dispatches to every registered callback.
// The hook is installed once per process, so the first caller's assembler is
// baked into the hook and used for every subsequent report.
pub(super) fn install_panic_hook(on_report: ReportCallback, assembler: ReportAssembler) {
    register_callback(on_report);
    if claim_hook_installation() {
        chain_panic_hook(spawn_worker(), assembler);
    }
}

fn register_callback(on_report: ReportCallback) {
    CALLBACKS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(on_report);
}

/// Returns `true` only for the caller that wins the race to install the
/// hook; subsequent callers just registered their callback above.
fn claim_hook_installation() -> bool {
    let mut installed = HOOK_INSTALLED.lock().unwrap_or_else(|e| e.into_inner());
    if *installed {
        return false;
    }
    *installed = true;
    true
}

/// Starts the worker thread that runs every registered callback for each
/// report, isolated from the panic hook so a panicking callback can't abort the process.
fn spawn_worker() -> SyncSender<CrashReport> {
    let (sender, receiver) = sync_channel::<CrashReport>(REPORT_QUEUE_CAPACITY);

    thread::Builder::new()
        .name(WORKER_THREAD_NAME.into())
        .spawn(move || {
            for report in receiver {
                let subscribers = CALLBACKS.lock().unwrap_or_else(|e| e.into_inner()).clone();
                for callback in subscribers {
                    if catch_unwind(AssertUnwindSafe(|| callback(report.clone()))).is_err() {
                        eprintln!("bevy_crash_capture: on_report callback panicked");
                    }
                }
            }
        })
        .expect("failed to spawn bevy_crash_capture worker thread");

    sender
}

/// Chains onto whatever hook was previously installed, so panic output
/// (e.g. the default stderr printer) keeps working alongside `on_report`.
fn chain_panic_hook(sender: SyncSender<CrashReport>, assembler: ReportAssembler) {
    let previous_hook = take_hook();
    set_hook(Box::new(move |info| {
        previous_hook(info);
        // Called from inside a panic hook: must never block or retry.
        let _ = sender.try_send(assembler.assemble(CrashKind::Panic(build_panic_report(info))));
    }));
}

// ============================================================================================
// UNIT TESTS
// ============================================================================================
#[cfg(test)]
pub(super) fn reset_for_test() {
    *HOOK_INSTALLED.lock().unwrap_or_else(|e| e.into_inner()) = false;
    CALLBACKS.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::panic_hook_lock;
    use std::env::consts;
    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::mpsc::sync_channel as test_sync_channel;
    use std::time::{Duration, Instant};

    fn test_assembler() -> ReportAssembler {
        ReportAssembler {
            context: crate::CrashContext {
                app_version: Some("1.2.3".to_string()),
                os: consts::OS,
            },
            #[cfg(feature = "recent-logs")]
            recent_logs: None,
        }
    }

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
        reset_for_test();

        let original_hook = take_hook();

        let previous_hook_called = Arc::new(AtomicBool::new(false));
        let previous_hook_called_clone = previous_hook_called.clone();
        set_hook(Box::new(move |_info| {
            previous_hook_called_clone.store(true, Ordering::SeqCst);
        }));

        let captured: Arc<StdMutex<Option<CrashReport>>> = Arc::new(StdMutex::new(None));
        let captured_clone = captured.clone();
        install_panic_hook(
            Arc::new(move |report| {
                *captured_clone.lock().unwrap() = Some(report);
            }),
            test_assembler(),
        );

        let result = catch_unwind(|| panic!("boom"));
        set_hook(original_hook);

        assert!(result.is_err());
        assert!(
            previous_hook_called.load(Ordering::SeqCst),
            "previous hook should still run"
        );

        let report = wait_for(|| captured.lock().unwrap().take(), Duration::from_secs(2))
            .expect("on_report should run");
        match report.kind {
            CrashKind::Panic(panic) => assert_eq!(panic.message, "boom"),
            CrashKind::Native { .. } => panic!("expected Panic, got Native"),
        }
        assert_eq!(report.context.app_version.as_deref(), Some("1.2.3"));
        assert_eq!(report.context.os, consts::OS);
    }

    #[test]
    fn on_report_panic_is_isolated_to_worker_thread() {
        let _guard = panic_hook_lock().lock().unwrap_or_else(|e| e.into_inner());
        reset_for_test();

        let original_hook = take_hook();

        let (started_tx, started_rx) = test_sync_channel::<()>(0);
        install_panic_hook(
            Arc::new(move |_report| {
                let _ = started_tx.send(());
                panic!("on_report itself panicked");
            }),
            test_assembler(),
        );

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

    #[test]
    fn multiple_installations_fan_out_without_duplicating_per_callback() {
        let _guard = panic_hook_lock().lock().unwrap_or_else(|e| e.into_inner());
        reset_for_test();

        let original_hook = take_hook();

        let first_calls = Arc::new(AtomicUsize::new(0));
        let second_calls = Arc::new(AtomicUsize::new(0));

        let first_calls_clone = first_calls.clone();
        install_panic_hook(
            Arc::new(move |_report| {
                first_calls_clone.fetch_add(1, Ordering::SeqCst);
            }),
            test_assembler(),
        );

        let second_calls_clone = second_calls.clone();
        install_panic_hook(
            Arc::new(move |_report| {
                second_calls_clone.fetch_add(1, Ordering::SeqCst);
            }),
            test_assembler(),
        );

        let result = catch_unwind(|| panic!("boom"));
        set_hook(original_hook);

        assert!(result.is_err());

        wait_for(
            || (first_calls.load(Ordering::SeqCst) > 0).then_some(()),
            Duration::from_secs(2),
        )
        .expect("first callback should run");
        wait_for(
            || (second_calls.load(Ordering::SeqCst) > 0).then_some(()),
            Duration::from_secs(2),
        )
        .expect("second callback should run");

        assert_eq!(first_calls.load(Ordering::SeqCst), 1);
        assert_eq!(second_calls.load(Ordering::SeqCst), 1);
    }
}
