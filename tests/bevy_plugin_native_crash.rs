use std::path::Path;
use std::time::{Duration, Instant};
use std::{fs, process, thread};

fn wait_for_minidump_len(path: &Path, timeout: Duration) -> Option<usize> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(contents) = fs::read_to_string(path)
            && let Ok(len) = contents.trim().parse::<usize>()
            && len > 0
        {
            return Some(len);
        }
        thread::sleep(Duration::from_millis(100));
    }
    None
}

#[test]
fn plugin_captures_native_crash() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let report_path = temp_dir.path().join("report.txt");

    let status = process::Command::new(env!("CARGO_BIN_EXE_bevy_crash_trigger"))
        .env("CRASH_REPORT_OUTPUT_PATH", &report_path)
        .status()
        .expect("failed to spawn bevy_crash_trigger");

    assert!(!status.success());

    let minidump_len = wait_for_minidump_len(&report_path, Duration::from_secs(10))
        .expect("watcher process did not write a valid crash report in time");
    assert!(minidump_len > 0);
}
