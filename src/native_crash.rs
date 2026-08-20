use minidumper_child::{ClientHandle, Error, MinidumperChild};
use std::path::Path;

/// Installs an out-of-process native crash handler.
///
/// - Call this as the very first thing in `main`, before creating the
///   `bevy::App`. Keep the returned guard alive for the app's lifetime.
/// - `on_minidump` runs in a separate watcher process and receives the
///   minidump bytes when the app crashes natively.
/// - Returns `Err` if the watcher process could not be spawned.
pub fn install_native_crash_capture<F>(on_minidump: F) -> Result<ClientHandle, Error>
where
    F: Fn(Vec<u8>, &Path) + Send + Sync + 'static,
{
    MinidumperChild::new().on_minidump(on_minidump).spawn()
}
