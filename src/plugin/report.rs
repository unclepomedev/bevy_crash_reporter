use crate::panic_report::PanicReport;
use std::path::PathBuf;

/// A captured crash, from either a Rust panic or a native OS-level crash.
#[derive(Debug, Clone)]
pub struct CrashReport {
    pub kind: CrashKind,
    pub context: CrashContext,
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
