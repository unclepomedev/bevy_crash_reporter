use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

type SharedBuffer = Arc<Mutex<VecDeque<String>>>;

/// A `tracing_subscriber` layer that keeps the most recent log lines in a
/// ring buffer, so they can be attached to crash reports.
///
/// Add the layer to your existing subscriber (e.g. via `bevy_log::LogPlugin`'s
/// `custom_layer`) and pass the handle to `CrashCapturePlugin::with_recent_logs`.
pub struct RecentLogsLayer {
    buffer: SharedBuffer,
    capacity: usize,
}

/// Read side of a `RecentLogsLayer`'s ring buffer.
#[derive(Clone)]
pub struct RecentLogsHandle {
    buffer: SharedBuffer,
}

impl RecentLogsLayer {
    /// Creates a layer keeping at most `capacity` log lines, plus the handle
    /// to pass to `CrashCapturePlugin::with_recent_logs`.
    pub fn new(capacity: usize) -> (Self, RecentLogsHandle) {
        let buffer: SharedBuffer = Arc::new(Mutex::new(VecDeque::with_capacity(capacity)));
        (
            Self {
                buffer: buffer.clone(),
                capacity,
            },
            RecentLogsHandle { buffer },
        )
    }
}

impl RecentLogsHandle {
    // Clones the buffer under the lock if available; formatting happens on the write
    // side, so the crash path only pays for the copy and never blocks if the lock
    // is currently contended.
    pub(crate) fn snapshot(&self) -> Vec<String> {
        self.buffer
            .try_lock()
            .map(|guard| guard.iter().cloned().collect())
            .unwrap_or_default()
    }
}

impl<S: Subscriber> Layer<S> for RecentLogsLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if self.capacity == 0 {
            return;
        }
        // Format outside the lock to keep it held only for the insertion.
        let mut visitor = LineVisitor::default();
        event.record(&mut visitor);
        let metadata = event.metadata();
        let line = format!(
            "{} {}: {}",
            metadata.level(),
            metadata.target(),
            visitor.line
        );

        let mut buffer = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        if buffer.len() == self.capacity {
            buffer.pop_front();
        }
        buffer.push_back(line);
    }
}

#[derive(Default)]
struct LineVisitor {
    line: String,
}

impl Visit for LineVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        use fmt::Write;
        if field.name() == "message" {
            if self.line.is_empty() {
                self.line = format!("{value:?}");
            } else {
                self.line = format!("{value:?} {}", self.line);
            }
        } else {
            if !self.line.is_empty() {
                self.line.push(' ');
            }
            let _ = write!(self.line, "{}={:?}", field.name(), value);
        }
    }
}

// ============================================================================================
// UNIT TESTS
// ============================================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt;

    #[test]
    fn keeps_only_the_most_recent_lines() {
        let (layer, handle) = RecentLogsLayer::new(2);
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("first");
            tracing::info!("second");
            tracing::warn!("third");
        });

        let logs = handle.snapshot();
        assert_eq!(logs.len(), 2);
        assert!(logs[0].contains("second"));
        assert!(logs[1].contains("third"));
        assert!(logs[1].starts_with("WARN"));
    }

    #[test]
    fn records_fields_alongside_message() {
        let (layer, handle) = RecentLogsLayer::new(8);
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(player_id = 7, "spawned");
        });

        let logs = handle.snapshot();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].contains("spawned"));
        assert!(logs[0].contains("player_id=7"));
    }

    #[test]
    fn snapshot_is_empty_without_events() {
        let (_layer, handle) = RecentLogsLayer::new(4);
        assert!(handle.snapshot().is_empty());
    }

    #[test]
    fn snapshot_returns_empty_when_mutex_is_locked() {
        let (_layer, handle) = RecentLogsLayer::new(4);
        let _guard = handle.buffer.lock().unwrap();
        assert!(handle.snapshot().is_empty());
    }

    #[test]
    fn zero_capacity_records_nothing() {
        let (layer, handle) = RecentLogsLayer::new(0);
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("dropped");
        });
        assert!(handle.snapshot().is_empty());
    }
}
