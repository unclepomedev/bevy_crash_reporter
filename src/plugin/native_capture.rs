use std::fmt::Display;

/// What to do if installing the native crash watcher itself fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeCaptureFailurePolicy {
    /// Panic immediately. Fails fast during development.
    #[default]
    Panic,
    /// Log to stderr and continue without native crash capture.
    /// Rust panics are still reported via `on_report`.
    Continue,
}

pub(super) fn handle_native_capture_result<T, E: Display>(
    result: Result<T, E>,
    policy: NativeCaptureFailurePolicy,
) -> Option<T> {
    match result {
        Ok(guard) => Some(guard),
        Err(err) => match policy {
            NativeCaptureFailurePolicy::Panic => {
                panic!("failed to install native crash capture: {err}")
            }
            NativeCaptureFailurePolicy::Continue => {
                eprintln!(
                    "bevy_crash_reporter: failed to install native crash capture, continuing without it: {err}"
                );
                None
            }
        },
    }
}

// ============================================================================================
// UNIT TESTS
// ============================================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::panic_hook_lock;

    #[test]
    fn ok_result_returns_guard_regardless_of_policy() {
        let result: Result<u8, &str> = Ok(42);
        assert_eq!(
            handle_native_capture_result(result, NativeCaptureFailurePolicy::Continue),
            Some(42)
        );
    }

    #[test]
    fn continue_policy_returns_none_without_panicking() {
        let result: Result<(), &str> = Err("boom");
        let outcome = handle_native_capture_result(result, NativeCaptureFailurePolicy::Continue);
        assert!(outcome.is_none());
    }

    #[test]
    #[should_panic(expected = "failed to install native crash capture")]
    fn panic_policy_panics_on_failure() {
        let _guard = panic_hook_lock().lock().unwrap_or_else(|e| e.into_inner());

        let result: Result<(), &str> = Err("boom");
        handle_native_capture_result(result, NativeCaptureFailurePolicy::Panic);
    }
}
