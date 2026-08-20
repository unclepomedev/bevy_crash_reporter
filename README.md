# bevy_crash_capture

Catches panics and native crashes (SEH/segfault) in Bevy games.

## How to use

1. (must) Add `CrashReporterPlugin` as the *first* plugin, before `DefaultPlugins`.
2. Give it an `on_report` closure. This is the only thing that decides where a crash report goes.

```rust
use bevy::prelude::*;
use bevy_crash_capture::{CrashReport, CrashReporterPlugin};

fn main() {
    App::new()
        .add_plugins(CrashReporterPlugin::new(|report: CrashReport| {  // 1, 2
            match report {
                CrashReport::Panic(panic) => eprintln!("panic: {}", panic.message),
                CrashReport::Native { minidump, .. } => {
                    eprintln!("native crash, {} byte minidump", minidump.len());
                }
            }
        }))
        .add_plugins(DefaultPlugins)
        .run();
}
```

Route `on_report` to whatever you already trust: your own backend, Sentry, a log file. This crate only catches; it does not send anything on its own.

### If the native crash watcher fails to start

By default, `CrashReporterPlugin` panics on startup if it can't spawn the watcher process. Use `NativeCaptureFailurePolicy::Continue` to keep the game running without native crash capture instead (Rust panics are still reported):

```rust
use bevy_crash_capture::{CrashReporterPlugin, NativeCaptureFailurePolicy};

CrashReporterPlugin::new(on_report)
    .with_native_capture_failure_policy(NativeCaptureFailurePolicy::Continue);
```

### Local development: GitHub Issues (optional feature)

`features = ["github-issues"]` adds `DevGitHubIssuesReporter`, which files a GitHub Issue per crash. **Local development only** — the token ships inside the binary if you pass it as a literal, so never bundle this into a build you distribute to players.

```rust
use bevy_crash_capture::DevGitHubIssuesReporter;

let reporter = DevGitHubIssuesReporter::new("your-org", "your-repo", github_token);
CrashReporterPlugin::new(move |report| reporter.notify(report));
```

Add `features = ["confirm-dialog"]` for a native Yes/No prompt before sending (panics only — see doc comment for the native-crash caveat).

## Security

- Anything embedded as a literal in the binary (tokens, webhook URLs) is extractable by anyone with the game files. `DevGitHubIssuesReporter` is dev-only for this reason.
- Raw minidump bytes are never sent by `DevGitHubIssuesReporter` — only size and filename. A minidump is a memory snapshot and may contain arbitrary sensitive data.
- Panic messages are not filtered. Don't put secrets in `panic!()` messages.

## What it does and does not do

- Captures Rust panics (via `std::panic::set_hook`, chained onto any existing hook) and native OS-level crashes (via an out-of-process watcher, `minidumper-child`).
- Runs on Windows / macOS / Linux. No WASM, no consoles.
- Does not send anything anywhere by default — `on_report` is required.
- Does not include a production notification backend. Bring your own (Sentry, your own service, etc.).
- Multiple `App`s in one process share a single panic hook and each get every report (see `plugin::panic_worker` if you need the details).

## License

MIT or Apache-2.0
