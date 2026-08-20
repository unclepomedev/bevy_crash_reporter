use std::path::Path;
use std::time::{Duration, Instant};
use std::{fs, process, thread};

fn wait_for_file(path: &Path, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }
    false
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
    assert!(
        wait_for_file(&report_path, Duration::from_secs(10)),
        "watcher process did not write a crash report in time"
    );

    let contents = fs::read_to_string(&report_path).expect("failed to read crash report");
    let minidump_len: usize = contents
        .trim()
        .parse()
        .expect("report should contain a byte length");
    assert!(minidump_len > 0);
}
