use crate::panic_report::PanicReport;
use std::path::PathBuf;

/// A captured crash, from either a Rust panic or a native OS-level crash.
#[derive(Debug, Clone)]
pub struct CrashReport {
    pub kind: CrashKind,
    pub context: CrashContext,
    /// The most recent log lines at crash time; empty unless a
    /// `RecentLogsHandle` was passed via `CrashCapturePlugin::with_recent_logs`.
    #[cfg(feature = "recent-logs")]
    pub recent_logs: Vec<String>,
}

/// What kind of crash was captured.
#[derive(Debug, Clone)]
pub enum CrashKind {
    Panic(PanicReport),
    Native { minidump: Vec<u8>, path: PathBuf },
}

/// Build-time context attached to every report.
#[derive(Debug, Clone)]
pub struct CrashContext {
    /// Set via `CrashCapturePlugin::with_app_version`.
    pub app_version: Option<String>,
    /// The OS this binary was built for (`std::env::consts::OS`).
    pub os: &'static str,
}

// Bundles everything needed to turn a `CrashKind` into a full `CrashReport`,
// keeping the cfg-gated fields in one place for both crash paths.
#[derive(Clone)]
pub(crate) struct ReportAssembler {
    pub(crate) context: CrashContext,
    #[cfg(feature = "recent-logs")]
    pub(crate) recent_logs: Option<super::recent_logs::RecentLogsHandle>,
}

impl ReportAssembler {
    pub(crate) fn assemble(&self, kind: CrashKind) -> CrashReport {
        CrashReport {
            kind,
            context: self.context.clone(),
            #[cfg(feature = "recent-logs")]
            recent_logs: self
                .recent_logs
                .as_ref()
                .map(|handle| handle.snapshot())
                .unwrap_or_default(),
        }
    }
}
