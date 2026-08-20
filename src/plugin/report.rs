use crate::panic_report::PanicReport;
use std::path::PathBuf;

/// A captured crash, from either a Rust panic or a native OS-level crash.
#[derive(Debug, Clone)]
pub enum CrashReport {
    Panic(PanicReport),
    Native { minidump: Vec<u8>, path: PathBuf },
}
