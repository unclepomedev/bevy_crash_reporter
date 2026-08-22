use crate::{CrashContext, CrashKind, CrashReport, PanicReport};
use serde::Serialize;
use std::time::Duration;

const DEFAULT_BASE_URL: &str = "https://api.github.com";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const USER_AGENT: &str = "bevy_crash_capture";
/// GitHub issue titles are capped at 256 chars; leave headroom for the
/// "panic: " prefix and an ellipsis.
const TITLE_MESSAGE_LIMIT: usize = 200;

/// Reports crashes by creating a GitHub Issue. Intended for development use (NEVER use for production).
pub struct DevGitHubIssuesReporter {
    owner: String,
    repo: String,
    token: String,
    base_url: String,
    timeout: Duration,
    #[cfg(feature = "confirm-dialog")]
    require_confirmation: bool,
}

impl DevGitHubIssuesReporter {
    pub fn new(
        owner: impl Into<String>,
        repo: impl Into<String>,
        token: impl Into<String>,
    ) -> Self {
        Self {
            owner: owner.into(),
            repo: repo.into(),
            token: token.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
            timeout: DEFAULT_TIMEOUT,
            #[cfg(feature = "confirm-dialog")]
            require_confirmation: false,
        }
    }

    /// Overrides the GitHub API base URL, for pointing at a local mock in tests.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Overrides the HTTP request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Shows a native Yes/No dialog before sending.
    #[cfg(feature = "confirm-dialog")]
    pub fn with_confirmation(mut self, enabled: bool) -> Self {
        self.require_confirmation = enabled;
        self
    }

    /// Use as the `on_report` callback for `CrashCapturePlugin`.
    pub fn notify(&self, report: CrashReport) {
        #[cfg(feature = "confirm-dialog")]
        if self.require_confirmation && !confirm_send() {
            return;
        }

        if let Err(err) = self.create_issue(&report) {
            eprintln!("bevy_crash_capture: failed to create GitHub issue: {err}");
        }
    }

    fn create_issue(&self, report: &CrashReport) -> Result<(), ureq::Error> {
        let payload = IssueRequest::from(report);
        let url = format!(
            "{}/repos/{}/{}/issues",
            self.base_url, self.owner, self.repo
        );
        ureq::post(&url)
            .config()
            .timeout_global(Some(self.timeout))
            .build()
            .header("Authorization", &format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", USER_AGENT)
            .send_json(&payload)?;
        Ok(())
    }
}

/// Reliable for panics (the game process is still running with a window).
/// For native crashes, the dialog runs in the separate watcher process, which never creates a window.
#[cfg(feature = "confirm-dialog")]
fn confirm_send() -> bool {
    rfd::MessageDialog::new()
        .set_title("Crash detected")
        .set_description("Send this crash report to GitHub Issues?")
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes
}

#[derive(Serialize)]
struct IssueRequest {
    title: String,
    body: String,
}

impl From<&CrashReport> for IssueRequest {
    fn from(report: &CrashReport) -> Self {
        let mut request = match &report.kind {
            CrashKind::Panic(panic_report) => Self {
                title: format!(
                    "panic: {}",
                    truncate(&panic_report.message, TITLE_MESSAGE_LIMIT)
                ),
                body: format!(
                    "{}\n\n{}",
                    format_panic_body(panic_report),
                    format_context(&report.context)
                ),
            },
            CrashKind::Native { minidump, .. } => Self {
                title: "native crash (minidump captured)".to_string(),
                body: format!(
                    "A native crash was captured.\n\n- minidump size: {} bytes\n{}",
                    minidump.len(),
                    format_context(&report.context)
                ),
            },
        };
        append_recent_logs(&mut request.body, report);
        request
    }
}

#[cfg(feature = "recent-logs")]
fn append_recent_logs(body: &mut String, report: &CrashReport) {
    if report.recent_logs.is_empty() {
        return;
    }
    let max_run = report
        .recent_logs
        .iter()
        .map(|line| longest_consecutive_backticks(line))
        .max()
        .unwrap_or(0);
    let fence_len = (max_run + 1).max(3);
    let fence = "`".repeat(fence_len);
    body.push_str(&format!(
        "\n\nRecent logs:\n{fence}\n{}\n{fence}",
        report.recent_logs.join("\n")
    ));
}

#[cfg(feature = "recent-logs")]
fn longest_consecutive_backticks(s: &str) -> usize {
    let mut max_run = 0;
    let mut current_run = 0;
    for ch in s.chars() {
        if ch == '`' {
            current_run += 1;
            max_run = max_run.max(current_run);
        } else {
            current_run = 0;
        }
    }
    max_run
}

#[cfg(not(feature = "recent-logs"))]
fn append_recent_logs(_body: &mut String, _report: &CrashReport) {}

fn format_panic_body(report: &PanicReport) -> String {
    match &report.location {
        Some(location) => format!(
            "```\n{}\n```\n\nLocation: `{}:{}:{}`",
            report.message, location.file, location.line, location.column
        ),
        None => format!("```\n{}\n```", report.message),
    }
}

fn format_context(context: &CrashContext) -> String {
    match &context.app_version {
        Some(version) => format!("- os: {}\n- app version: {}", context.os, version),
        None => format!("- os: {}", context.os),
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len).collect();
        format!("{truncated}…")
    }
}

// ============================================================================================
// UNIT TESTS
// ============================================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::PanicLocation;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::mpsc::{Receiver, channel};
    use std::thread;
    use std::time::Duration;

    fn test_context() -> CrashContext {
        CrashContext {
            app_version: Some("1.2.3".to_string()),
            os: "testos",
        }
    }

    fn panic_report(kind: CrashKind) -> CrashReport {
        CrashReport {
            kind,
            context: test_context(),
            #[cfg(feature = "recent-logs")]
            recent_logs: Vec::new(),
        }
    }

    struct MockServer {
        base_url: String,
        received: Receiver<(String, String)>,
    }

    fn start_mock_server(status_line: &'static str) -> MockServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind mock server");
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = channel();

        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                stream
                    .set_read_timeout(Some(Duration::from_millis(500)))
                    .ok();

                let mut received = Vec::new();
                let mut buf = [0u8; 8192];
                loop {
                    match stream.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            received.extend_from_slice(&buf[..n]);
                            if request_is_complete(&received) {
                                break;
                            }
                        }
                        Err(_) => break, // timed out; use what we have
                    }
                }

                let request = String::from_utf8_lossy(&received).to_string();
                let (headers, body) = request.split_once("\r\n\r\n").unwrap_or((&request, ""));
                let _ = tx.send((headers.to_string(), body.to_string()));
                let _ = stream.write_all(status_line.as_bytes());
            }
        });

        MockServer {
            base_url: format!("http://{addr}"),
            received: rx,
        }
    }

    /// True once we've received the header terminator and, if a
    /// Content-Length header is present, that many body bytes too.
    fn request_is_complete(received: &[u8]) -> bool {
        let Some(header_end) = received.windows(4).position(|w| w == b"\r\n\r\n") else {
            return false;
        };
        let headers = String::from_utf8_lossy(&received[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .and_then(|v| v.trim().parse::<usize>().ok())
            })
            .unwrap_or(0);
        received.len() >= header_end + 4 + content_length
    }

    #[test]
    fn sends_expected_request_for_panic_report() {
        let server = start_mock_server("HTTP/1.1 201 Created\r\nContent-Length: 2\r\n\r\n{}");

        let reporter = DevGitHubIssuesReporter::new("owner", "repo", "test-token")
            .with_base_url(server.base_url);

        reporter.notify(panic_report(CrashKind::Panic(PanicReport {
            message: "boom".to_string(),
            location: Some(PanicLocation {
                file: "src/main.rs".to_string(),
                line: 1,
                column: 1,
            }),
        })));

        let (headers, body) = server
            .received
            .recv_timeout(Duration::from_secs(2))
            .expect("mock server did not receive a request");

        assert!(headers.starts_with("POST /repos/owner/repo/issues"));
        assert!(
            headers
                .to_lowercase()
                .contains("authorization: bearer test-token")
        );
        assert!(body.contains("boom"));
        assert!(body.contains("src/main.rs"));
        assert!(body.contains("os: testos"));
        assert!(body.contains("app version: 1.2.3"));
    }

    #[cfg(feature = "recent-logs")]
    #[test]
    fn appends_recent_logs_to_body_when_present() {
        let mut report = panic_report(CrashKind::Panic(PanicReport {
            message: "boom".to_string(),
            location: None,
        }));
        report.recent_logs = vec!["INFO game: spawned".to_string()];
        let payload = IssueRequest::from(&report);
        assert!(payload.body.contains("Recent logs:"));
        assert!(payload.body.contains("```\nINFO game: spawned\n```"));
    }

    #[cfg(feature = "recent-logs")]
    #[test]
    fn escapes_backticks_in_recent_logs_with_longer_fence() {
        let mut report = panic_report(CrashKind::Panic(PanicReport {
            message: "boom".to_string(),
            location: None,
        }));
        report.recent_logs = vec![
            "INFO game: code block ```example``` inside".to_string(),
            "DEBUG game: another ````4-backtick```` run".to_string(),
        ];
        let payload = IssueRequest::from(&report);
        assert!(payload.body.contains("Recent logs:\n`````\n"));
        assert!(payload.body.ends_with("\n`````"));
        assert!(payload.body.contains("```example```"));
        assert!(payload.body.contains("````4-backtick````"));
    }

    #[test]
    fn truncates_long_panic_messages_in_title() {
        let payload = IssueRequest::from(&panic_report(CrashKind::Panic(PanicReport {
            message: "x".repeat(500),
            location: None,
        })));
        assert!(payload.title.chars().count() <= TITLE_MESSAGE_LIMIT + "panic: …".chars().count());
    }

    #[test]
    fn formats_native_crash_report_without_local_path() {
        let payload = IssueRequest::from(&panic_report(CrashKind::Native {
            minidump: vec![0; 42],
            path: PathBuf::from("/Users/secret_user/project/crash.dmp"),
        }));
        assert_eq!(payload.title, "native crash (minidump captured)");
        assert!(payload.body.contains("minidump size: 42 bytes"));
        assert!(payload.body.contains("os: testos"));
        assert!(payload.body.contains("app version: 1.2.3"));
        assert!(!payload.body.contains("secret_user"));
        assert!(!payload.body.contains("crash.dmp"));
    }

    #[test]
    fn times_out_when_endpoint_does_not_respond() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind mock server");
        let addr = listener.local_addr().unwrap();

        let _handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                // Do not send any response; sleep to keep connection open
                thread::sleep(Duration::from_secs(2));
            }
        });

        let reporter = DevGitHubIssuesReporter::new("owner", "repo", "test-token")
            .with_base_url(format!("http://{addr}"))
            .with_timeout(Duration::from_millis(100));

        let report = panic_report(CrashKind::Panic(PanicReport {
            message: "timeout test".to_string(),
            location: None,
        }));

        let start = std::time::Instant::now();
        let result = reporter.create_issue(&report);
        let elapsed = start.elapsed();

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ureq::Error::Timeout(_)));
        assert!(elapsed < Duration::from_secs(1));
    }
}
