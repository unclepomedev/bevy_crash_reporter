mod native_capture;
mod panic_worker;
#[cfg(feature = "recent-logs")]
mod recent_logs;
mod report;

pub use self::native_capture::NativeCaptureFailurePolicy;
#[cfg(feature = "recent-logs")]
pub use self::recent_logs::{RecentLogsHandle, RecentLogsLayer};
pub use self::report::{CrashContext, CrashKind, CrashReport};

use self::native_capture::handle_native_capture_result;
use self::panic_worker::install_panic_hook;
use self::report::ReportAssembler;
use crate::native_crash::install_native_crash_capture;
use bevy_app::{App, Plugin};
use std::env::consts;
use std::sync::Arc;

/// Captures panics and native crashes, forwarding both to `on_report`.
///
/// - Add this as the *first* plugin, before `DefaultPlugins`.
/// - `on_report` may fire from either the app process (panics) or the
///   separate watcher process (native crashes) — see `install_native_crash_capture`.
pub struct CrashCapturePlugin {
    on_report: Arc<dyn Fn(CrashReport) + Send + Sync>,
    native_capture_failure_policy: NativeCaptureFailurePolicy,
    app_version: Option<String>,
    #[cfg(feature = "recent-logs")]
    recent_logs: Option<RecentLogsHandle>,
}

impl CrashCapturePlugin {
    pub fn new(on_report: impl Fn(CrashReport) + Send + Sync + 'static) -> Self {
        Self {
            on_report: Arc::new(on_report),
            native_capture_failure_policy: NativeCaptureFailurePolicy::default(),
            app_version: None,
            #[cfg(feature = "recent-logs")]
            recent_logs: None,
        }
    }

    pub fn with_native_capture_failure_policy(
        mut self,
        policy: NativeCaptureFailurePolicy,
    ) -> Self {
        self.native_capture_failure_policy = policy;
        self
    }

    /// Attaches an application version (e.g. `env!("CARGO_PKG_VERSION")`) to every report.
    pub fn with_app_version(mut self, version: impl Into<String>) -> Self {
        self.app_version = Some(version.into());
        self
    }

    /// Attaches the most recent log lines from a `RecentLogsLayer` to every report.
    ///
    /// Only populated for panics. Native crashes run in a separate watcher process that never
    /// executed your game's logging, so `CrashReport::recent_logs` is always empty for those.
    #[cfg(feature = "recent-logs")]
    pub fn with_recent_logs(mut self, handle: RecentLogsHandle) -> Self {
        self.recent_logs = Some(handle);
        self
    }
}

impl Plugin for CrashCapturePlugin {
    fn build(&self, app: &mut App) {
        let assembler = ReportAssembler {
            context: CrashContext {
                app_version: self.app_version.clone(),
                os: consts::OS,
            },
            #[cfg(feature = "recent-logs")]
            recent_logs: self.recent_logs.clone(),
        };

        let native_cb = self.on_report.clone();
        let native_assembler = assembler.clone();
        let result = install_native_crash_capture(move |buffer, path| {
            native_cb(native_assembler.assemble(CrashKind::Native {
                minidump: buffer,
                path: path.to_path_buf(),
            }));
        });
        if let Some(guard) =
            handle_native_capture_result(result, self.native_capture_failure_policy)
        {
            // Must outlive the app, or the watcher process is torn down immediately.
            app.insert_non_send(guard);
        }

        install_panic_hook(self.on_report.clone(), assembler);
    }
}
