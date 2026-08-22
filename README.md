# bevy_crash_capture

[![Crates.io](https://img.shields.io/crates/v/bevy_crash_capture.svg)](https://crates.io/crates/bevy_crash_capture)

Catches panics and native crashes (SEH/segfault) in Bevy games.

## How to use

1. (must) Add `CrashCapturePlugin` as the *first* plugin, before `DefaultPlugins`.
2. Give it an `on_report` closure. This is the only thing that decides where a crash report goes.

```rust
use bevy::prelude::*;
use bevy_crash_capture::{CrashKind, CrashReport, CrashCapturePlugin};

fn main() {
    App::new()
        .add_plugins(CrashCapturePlugin::new(|report: CrashReport| {  // 1, 2
            match report.kind {
                CrashKind::Panic(panic) => eprintln!("panic: {}", panic.message),
                CrashKind::Native { minidump, .. } => {
                    eprintln!("native crash, {} byte minidump", minidump.len());
                }
            }
        })
        .with_app_version(env!("CARGO_PKG_VERSION")))
        .add_plugins(DefaultPlugins)
        .run();
}
```

Route `on_report` to whatever you already trust: your own backend, Sentry, a log file. This crate only catches; it does not send anything on its own.

Every report carries a `CrashContext` (`report.context`) with the build-target `os` and, if set via `.with_app_version(...)`, your `app_version`.

### If the native crash watcher fails to start

By default, `CrashCapturePlugin` panics on startup if it can't spawn the watcher process. Use `NativeCaptureFailurePolicy::Continue` to keep the game running without native crash capture instead (Rust panics are still reported):

```rust
use bevy_crash_capture::{CrashCapturePlugin, NativeCaptureFailurePolicy};

CrashCapturePlugin::new(on_report)
    .with_native_capture_failure_policy(NativeCaptureFailurePolicy::Continue);
```

### Attaching recent logs (optional feature)

`features = ["recent-logs"]` adds `RecentLogsLayer`, a `tracing_subscriber` layer that keeps the most recent log lines in a ring buffer. Reports then carry them in `report.recent_logs`.

This crate never installs or replaces a global tracing subscriber — it plugs into whatever log setup you already have. With Bevy's `LogPlugin`, hand the layer over through `custom_layer` (a plain `fn` pointer, so it is passed via a `static`):

```rust
use bevy::log::{BoxedLayer, LogPlugin};
use bevy_crash_capture::{CrashCapturePlugin, RecentLogsLayer};
use std::sync::Mutex;
use tracing_subscriber::Layer;

static RECENT_LOGS_LAYER: Mutex<Option<BoxedLayer>> = Mutex::new(None);

fn take_recent_logs_layer(_app: &mut App) -> Option<BoxedLayer> {
    RECENT_LOGS_LAYER.lock().unwrap().take()
}

fn main() {
    let (layer, handle) = RecentLogsLayer::new(100);
    *RECENT_LOGS_LAYER.lock().unwrap() = Some(layer.boxed());

    App::new()
        .add_plugins(CrashCapturePlugin::new(on_report).with_recent_logs(handle))
        .add_plugins(DefaultPlugins.set(LogPlugin {
            custom_layer: take_recent_logs_layer,
            ..Default::default()
        }))
        .run();
}
```

If you build your own tracing subscriber instead of using `LogPlugin`, add the layer there (e.g. `registry().with(layer)`); the handle works the same either way. Without `.with_recent_logs(...)`, `report.recent_logs` is empty.

### Local development: GitHub Issues (optional feature)

`features = ["github-issues"]` adds `DevGitHubIssuesReporter`, which files a GitHub Issue per crash. **Local development only** — the token ships inside the binary if you pass it as a literal, so never bundle this into a build you distribute to players.

```rust
use bevy_crash_capture::DevGitHubIssuesReporter;

let reporter = DevGitHubIssuesReporter::new("your-org", "your-repo", github_token);
CrashCapturePlugin::new(move |report| reporter.notify(report));
```

Add `features = ["confirm-dialog"]` for a native Yes/No prompt before sending (panics only — see doc comment for the native-crash caveat).

## Security

- Anything embedded as a literal in the binary (tokens, webhook URLs) is extractable by anyone with the game files. `DevGitHubIssuesReporter` is dev-only for this reason.
- Raw minidump bytes are never sent by `DevGitHubIssuesReporter` — only size and filename. A minidump is a memory snapshot and may contain arbitrary sensitive data.
- Panic messages are not filtered. Don't put secrets in `panic!()` messages.
- With `recent-logs`, log lines are attached to reports verbatim. This crate does not filter or redact them — keeping secrets and personal data out of your logs is your responsibility.

## What it does and does not do

- Captures Rust panics (via `std::panic::set_hook`, chained onto any existing hook) and native OS-level crashes (via an out-of-process watcher, `minidumper-child`).
- Runs on Windows / macOS / Linux. No WASM, no consoles.
- Does not send anything anywhere by default — `on_report` is required.
- Does not include a production notification backend. Bring your own (Sentry, your own service, etc.).
- Multiple `App`s in one process share a single panic hook and each get every report (see `plugin::panic_worker` if you need the details).

## License

MIT or Apache-2.0
