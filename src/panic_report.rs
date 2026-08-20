use std::panic::PanicHookInfo;

/// A backend-agnostic representation of a captured Rust panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanicReport {
    pub message: String,
    pub location: Option<PanicLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanicLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

#[cfg_attr(not(test), expect(dead_code))]
/// Extracts a [`PanicReport`] from the raw hook info `std::panic` gives.
pub(crate) fn build_panic_report(info: &PanicHookInfo<'_>) -> PanicReport {
    PanicReport {
        message: panic_message(info),
        location: info.location().map(|l| PanicLocation {
            file: l.file().to_string(),
            line: l.line(),
            column: l.column(),
        }),
    }
}

fn panic_message(info: &PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

// ============================================================================================
// UNIT TESTS
// ============================================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use std::panic;
    use std::sync::{Arc, Mutex, OnceLock};

    /// `panic::set_hook` is process-global.
    fn hook_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn captures_string_message_and_location() {
        let _guard = hook_test_lock().lock().unwrap();

        let captured: Arc<Mutex<Option<PanicReport>>> = Arc::new(Mutex::new(None));
        let captured_clone = captured.clone();

        let previous_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            *captured_clone.lock().unwrap() = Some(build_panic_report(info));
        }));

        let result = panic::catch_unwind(|| {
            panic!("boom");
        });

        panic::set_hook(previous_hook);

        assert!(result.is_err());
        let report = captured.lock().unwrap().take().expect("hook should run");
        assert_eq!(report.message, "boom");
        let location = report.location.expect("location should be captured");
        assert!(location.file.ends_with("panic_report.rs"));
    }

    #[test]
    fn captures_formatted_string_message() {
        let _guard = hook_test_lock().lock().unwrap();

        let captured: Arc<Mutex<Option<PanicReport>>> = Arc::new(Mutex::new(None));
        let captured_clone = captured.clone();

        let previous_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            *captured_clone.lock().unwrap() = Some(build_panic_report(info));
        }));

        let result = panic::catch_unwind(|| {
            let code = 42;
            panic!("boom with code {code}");
        });

        panic::set_hook(previous_hook);

        assert!(result.is_err());
        let report = captured.lock().unwrap().take().expect("hook should run");
        assert_eq!(report.message, "boom with code 42");
    }
}
