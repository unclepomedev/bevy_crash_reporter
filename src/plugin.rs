mod native_capture;
mod panic_worker;
mod report;

pub use self::native_capture::NativeCaptureFailurePolicy;
pub use self::report::CrashReport;

use self::native_capture::handle_native_capture_result;
use self::panic_worker::install_panic_hook;
use crate::native_crash::install_native_crash_capture;
use bevy_app::{App, Plugin};
use std::sync::Arc;

/// Captures panics and native crashes, forwarding both to `on_report`.
///
/// - Add this as the *first* plugin, before `DefaultPlugins`.
/// - `on_report` may fire from either the app process (panics) or the
///   separate watcher process (native crashes) — see `install_native_crash_capture`.
pub struct CrashCapturePlugin {
    on_report: Arc<dyn Fn(CrashReport) + Send + Sync>,
    native_capture_failure_policy: NativeCaptureFailurePolicy,
}

impl CrashCapturePlugin {
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

impl Plugin for CrashCapturePlugin {
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
