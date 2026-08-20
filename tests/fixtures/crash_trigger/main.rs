use std::path::Path;
use std::{env, fs, ptr};

fn main() {
    let output_path = env::var("CRASH_REPORT_OUTPUT_PATH")
        .expect("CRASH_REPORT_OUTPUT_PATH must be set by the test harness");

    let _guard = bevy_crash_reporter::install_native_crash_capture(move |buffer, _path: &Path| {
        fs::write(&output_path, buffer.len().to_string())
            .expect("failed to write crash report marker");
    })
    .expect("failed to install native crash capture");

    // Deliberately trigger a native (non-unwindable) crash.
    #[allow(deref_nullptr)]
    unsafe {
        *ptr::null_mut() = 1u8;
    }
}
