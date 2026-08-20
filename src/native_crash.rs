use std::path::Path;

/// Installs an out-of-process native crash handler.
///
/// - Call this as the very first thing in `main`, before creating the
///   `bevy::App`. Keep the returned guard alive for the app's lifetime.
/// - `on_minidump` runs in a separate watcher process and receives the
///   minidump bytes when the app crashes natively.
/// - Returns `Err` if the watcher process could not be spawned.
pub fn install_native_crash_capture<F>(
    on_minidump: F,
) -> Result<minidumper_child::ClientHandle, minidumper_child::Error>
where
    F: Fn(&[u8], &Path) + Send + Sync + 'static,
{
    // minidumper_child re-execs the current binary as a "watcher" process;
    // in that process this function never returns. That's why "before creating bevy::App".
    minidumper_child::MinidumperChild::new()
        .on_minidump(move |buffer: Vec<u8>, path: &Path| {
            on_minidump(&buffer, path);
        })
        .spawn()
}
