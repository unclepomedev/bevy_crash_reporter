use bevy_app::App;
use bevy_crash_capture::{CrashCapturePlugin, CrashKind};
use std::hint::black_box;
use std::{env, fs, ptr};

fn main() {
    let output_path = env::var("CRASH_REPORT_OUTPUT_PATH")
        .expect("CRASH_REPORT_OUTPUT_PATH must be set by the test harness");

    let mut app = App::new();
    app.add_plugins(CrashCapturePlugin::new(move |report| {
        if let CrashKind::Native { minidump, .. } = report.kind {
            fs::write(&output_path, minidump.len().to_string())
                .expect("failed to write crash report marker");
        }
    }));

    let invalid_addr = black_box(1usize);
    let invalid_ptr = invalid_addr as *mut u8;
    unsafe {
        ptr::write_volatile(invalid_ptr, 1);
    }
}
