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
    use crate::test_support::panic_hook_lock;
    use std::panic::{catch_unwind, set_hook, take_hook};
    use std::sync::{Arc, Mutex};

    #[test]
    fn captures_string_message_and_location() {
        let _guard = panic_hook_lock().lock().unwrap_or_else(|e| e.into_inner());

        let captured: Arc<Mutex<Option<PanicReport>>> = Arc::new(Mutex::new(None));
        let captured_clone = captured.clone();

        let previous_hook = take_hook();
        set_hook(Box::new(move |info| {
            *captured_clone.lock().unwrap() = Some(build_panic_report(info));
        }));

        let result = catch_unwind(|| {
            panic!("boom");
        });

        set_hook(previous_hook);

        assert!(result.is_err());
        let report = captured.lock().unwrap().take().expect("hook should run");
        assert_eq!(report.message, "boom");
        let location = report.location.expect("location should be captured");
        assert!(location.file.ends_with("panic_report.rs"));
    }

    #[test]
    fn captures_formatted_string_message() {
        let _guard = panic_hook_lock().lock().unwrap_or_else(|e| e.into_inner());

        let captured: Arc<Mutex<Option<PanicReport>>> = Arc::new(Mutex::new(None));
        let captured_clone = captured.clone();

        let previous_hook = take_hook();
        set_hook(Box::new(move |info| {
            *captured_clone.lock().unwrap() = Some(build_panic_report(info));
        }));

        let result = catch_unwind(|| {
            let code = 42;
            panic!("boom with code {code}");
        });

        set_hook(previous_hook);

        assert!(result.is_err());
        let report = captured.lock().unwrap().take().expect("hook should run");
        assert_eq!(report.message, "boom with code 42");
    }
}
