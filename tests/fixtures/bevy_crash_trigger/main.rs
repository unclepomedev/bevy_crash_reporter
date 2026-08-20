use bevy_app::App;
use bevy_crash_reporter::{CrashReport, CrashReporterPlugin};
use std::{env, fs, ptr};

fn main() {
    let output_path = env::var("CRASH_REPORT_OUTPUT_PATH")
        .expect("CRASH_REPORT_OUTPUT_PATH must be set by the test harness");

    let mut app = App::new();
    app.add_plugins(CrashReporterPlugin::new(move |report| {
        if let CrashReport::Native { minidump, .. } = report {
            fs::write(&output_path, minidump.len().to_string())
                .expect("failed to write crash report marker");
        }
    }));

    #[allow(deref_nullptr)]
    unsafe {
        *ptr::null_mut() = 1u8;
    }
}
