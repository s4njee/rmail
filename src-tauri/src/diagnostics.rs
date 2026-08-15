//! Diagnostics (Roadmap E2.3): unified logging, crash & error reporting, and
//! the opt-in usage ping.
//!
//! Privacy posture (deliberate, documented in `docs/telemetry.md`):
//! - Crash reports are **structured metadata only** — never free-form log text,
//!   never message content. Every free-text field passes through [`redact`]
//!   (emails, tokens, home paths) before it touches disk.
//! - Records are **always written locally** to `app_data_dir/crash_reports/`;
//!   *transmission* is the opt-in action. Nothing is sent unless the matching
//!   setting is on AND a build-time endpoint is configured.
//! - The plan2.md §10.4 invariant — credentials are never logged — holds: no
//!   log line or report ever interpolates an account address or credential
//!   (the sync-log migration replaced those with account ids).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use quill_store::types::DiagnosticsInfo;
use regex::Regex;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::settings;

/// Upload endpoint for crash reports, from the build-time env var.
/// Empty = sending disabled (reports still queue locally).
const CRASH_ENDPOINT: &str = match option_env!("QUILL_CRASH_ENDPOINT") {
    Some(v) => v,
    None => "",
};
/// Upload endpoint for the usage ping, from the build-time env var.
/// Empty = sending disabled.
const USAGE_ENDPOINT: &str = match option_env!("QUILL_USAGE_ENDPOINT") {
    Some(v) => v,
    None => "",
};

/// Cap on locally-queued JS-error reports (a runaway error loop must not fill
/// the disk). Panics are rare and exempt.
const MAX_PENDING_JS_ERRORS: usize = 200;

/// State the panic hook and commands need, populated in [`init`] during setup —
/// before any window is built, so a panic during startup is still captured.
static DIAG: OnceLock<DiagState> = OnceLock::new();

struct DiagState {
    app_version: String,
    pending_dir: PathBuf,
    sent_dir: PathBuf,
}

/// A scrubbed, structured crash report. Camel-case keys (matches the app's IPC
/// convention). Serialized verbatim when uploaded; see `docs/telemetry.md`.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CrashRecord {
    id: String,
    /// `"panic"` | `"js_error"`.
    kind: String,
    created_at_ms: i64,
    app_version: String,
    /// `std::env::consts::OS`: "macos" | "windows" | "linux".
    os: String,
    /// `std::env::consts::ARCH`.
    arch: String,
    thread: Option<String>,
    message: String,
    stack: Option<String>,
    /// JS only: source URL, line, column.
    source: Option<String>,
    line: Option<u32>,
    column: Option<u32>,
}

/// Payload from the frontend's `window.onerror` / `onunhandledrejection`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsErrorPayload {
    pub message: String,
    pub stack: Option<String>,
    pub source: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// Accepted values for the `log_level` setting / `set_log_level` command.
const LOG_LEVELS: [&str; 5] = ["error", "warn", "info", "debug", "trace"];

pub fn is_valid_log_level(level: &str) -> bool {
    LOG_LEVELS.contains(&level)
}

// ---------------------------------------------------------------------------
// Panic hook
// ---------------------------------------------------------------------------

/// Install the crash-capture panic hook. Called once at the top of `run()`, so
/// a panic anywhere in startup is still recorded. Under release `panic =
/// "abort"` the hook runs then the process dies — it must never log (logger
/// lock could deadlock) and never touch the network.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|l| l.to_string())
            .unwrap_or_default();
        let message = panic_message(info);
        // Preserve the default hook's visible output so terminal runs still
        // see it (the default hook is replaced by this one).
        eprintln!("thread '{}' panicked at {location}:\n{message}", thread_name());
        if let Some(state) = DIAG.get() {
            let record = CrashRecord {
                id: new_id(),
                kind: "panic".into(),
                created_at_ms: now_ms(),
                app_version: state.app_version.clone(),
                os: std::env::consts::OS.into(),
                arch: std::env::consts::ARCH.into(),
                thread: Some(thread_name()),
                message: redact(&message),
                stack: Some(redact(&std::backtrace::Backtrace::force_capture().to_string())),
                source: None,
                line: None,
                column: None,
            };
            // Best effort: a failed write must not double-panic inside the hook.
            let _ = write_record(&state.pending_dir, &record);
        }
    }));
}

fn panic_message(info: &std::panic::PanicHookInfo) -> String {
    if let Some(s) = info.payload().downcast_ref::<&str>() {
        return (*s).to_string();
    }
    if let Some(s) = info.payload().downcast_ref::<String>() {
        return s.clone();
    }
    "Panic occurred".to_string()
}

fn thread_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("<unnamed>")
        .to_string()
}

// ---------------------------------------------------------------------------
// Setup / background tasks
// ---------------------------------------------------------------------------

/// Called from the app setup hook, after plugins are initialized (so the log
/// plugin already owns the `log` facade). Stashes the state the panic hook
/// needs, applies the persisted log level, and spawns the once-per-launch
/// upload + ping task.
pub fn init(app: &AppHandle) {
    let Ok(data_dir) = app.path().app_data_dir() else {
        return;
    };
    let pending = data_dir.join("crash_reports").join("pending");
    let sent = data_dir.join("crash_reports").join("sent");
    let _ = std::fs::create_dir_all(&pending);
    let _ = std::fs::create_dir_all(&sent);

    let app_version = app.package_info().version.to_string();
    if DIAG.get().is_none() {
        let _ = DIAG.set(DiagState {
            app_version,
            pending_dir: pending,
            sent_dir: sent,
        });
    }

    let settings = settings::load(app);
    // The facade's max level is the single log-level control: both Rust
    // `log::` records and JS records (which arrive via the plugin command and
    // funnel through `log::log!`) gate on it. The plugin builder is left at
    // `Trace` so nothing is pre-filtered there.
    apply_log_level(&settings.log_level);

    // Background flush + ping, delayed so startup stays snappy and the first
    // paint isn't waiting on telemetry.
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let Some(state) = DIAG.get() else {
            return;
        };
        let client = http_client();
        if settings.crash_reporting_enabled && !CRASH_ENDPOINT.is_empty() {
            let n = upload_pending(&client, &state.pending_dir, &state.sent_dir, CRASH_ENDPOINT).await;
            if n > 0 {
                log::info!("uploaded {n} pending crash report(s)");
            }
        }
        if settings.usage_ping_enabled && !USAGE_ENDPOINT.is_empty() {
            match send_usage_ping(&client, state, USAGE_ENDPOINT).await {
                Ok(()) => log::debug!("usage ping sent"),
                Err(e) => log::warn!("usage ping failed: {e}"),
            }
        }
    });
}

fn diag_state() -> Result<&'static DiagState, String> {
    DIAG.get().ok_or_else(|| "diagnostics not initialized".to_string())
}

/// Apply a `log_level` string ("error".."trace") to the `log` facade.
pub fn apply_log_level(level: &str) {
    log::set_max_level(level_filter(level));
}

fn level_filter(level: &str) -> log::LevelFilter {
    match level {
        "error" => log::LevelFilter::Error,
        "warn" => log::LevelFilter::Warn,
        "debug" => log::LevelFilter::Debug,
        "trace" => log::LevelFilter::Trace,
        _ => log::LevelFilter::Info,
    }
}

// ---------------------------------------------------------------------------
// Queueing
// ---------------------------------------------------------------------------

fn write_record(dir: &Path, record: &CrashRecord) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.json", record.id));
    let json = serde_json::to_string_pretty(record).map_err(std::io::Error::other)?;
    std::fs::write(&path, json)?;
    Ok(path)
}

/// Enqueue a frontend JS error as a local crash record (always written,
/// scrubbed; transmission is gated separately). Returns the record id.
fn queue_js_error(state: &DiagState, payload: JsErrorPayload) -> Result<String, String> {
    let record = CrashRecord {
        id: new_id(),
        kind: "js_error".into(),
        created_at_ms: now_ms(),
        app_version: state.app_version.clone(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        thread: None,
        message: redact(&payload.message),
        stack: payload.stack.as_deref().map(redact),
        source: payload.source,
        line: payload.line,
        column: payload.column,
    };
    let id = record.id.clone();
    write_record(&state.pending_dir, &record).map_err(|e| e.to_string())?;
    trim_pending(&state.pending_dir, MAX_PENDING_JS_ERRORS);
    Ok(id)
}

/// Keep at most `max` pending reports: delete the oldest (by mtime) beyond it.
fn trim_pending(dir: &Path, max: usize) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
        .map(|e| (e.path(), e.metadata().and_then(|m| m.modified()).ok()))
        .collect();
    if entries.len() <= max {
        return;
    }
    entries.sort_by_key(|(_, m)| *m);
    let remove = entries.len() - max;
    for (path, _) in entries.into_iter().take(remove) {
        let _ = std::fs::remove_file(path);
    }
}

fn count_json_files(dir: &Path) -> u32 {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
        .count() as u32
}

// ---------------------------------------------------------------------------
// Transmission (reqwest — the webview CSP blocks all external fetch, so every
// outbound byte goes through Rust)
// ---------------------------------------------------------------------------

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("reqwest client config is valid")
}

/// POST each pending report to `endpoint`; move successful ones to `sent/`.
/// Failed uploads stay in `pending/` for a later attempt. Skip-on-missing-file
/// tolerates the "send test report" race with the background flush.
async fn upload_pending(
    client: &reqwest::Client,
    pending_dir: &Path,
    sent_dir: &Path,
    endpoint: &str,
) -> usize {
    let paths: Vec<PathBuf> = std::fs::read_dir(pending_dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
        .collect();
    upload_paths(client, sent_dir, endpoint, &paths).await
}

async fn upload_paths(
    client: &reqwest::Client,
    sent_dir: &Path,
    endpoint: &str,
    paths: &[PathBuf],
) -> usize {
    let mut sent = 0;
    for path in paths {
        let Ok(body) = std::fs::read(path) else {
            continue; // already moved/removed by a concurrent upload
        };
        match client.post(endpoint).body(body).send().await {
            Ok(resp) if resp.status().is_success() => {
                let dest = sent_dir.join(path.file_name().unwrap_or_default());
                let _ = std::fs::create_dir_all(sent_dir);
                if std::fs::rename(path, &dest).is_ok() {
                    sent += 1;
                }
            }
            _ => {
                // Server unreachable / rejected — keep for a later attempt.
            }
        }
    }
    sent
}

async fn send_usage_ping(
    client: &reqwest::Client,
    state: &DiagState,
    endpoint: &str,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "client": "quill",
        "event": "launch",
        "appVersion": state.app_version,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "channel": channel_of(&state.app_version),
    });
    let resp = client
        .post(endpoint)
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(format!("usage ping rejected: {}", resp.status()))
    }
}

/// "stable" for a plain version, else the pre-release tag's first segment
/// ("0.1.0-beta.2" -> "beta").
fn channel_of(version: &str) -> String {
    match version.split_once('-') {
        Some((_, pre)) => pre.split('.').next().unwrap_or("beta").to_string(),
        None => "stable".to_string(),
    }
}

// ---------------------------------------------------------------------------
// PII redaction
// ---------------------------------------------------------------------------

fn email_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"[A-Za-z0-9._%+'\-]+@[A-Za-z0-9\-]+(\.[A-Za-z0-9\-]+)+").expect("valid"))
}

fn secret_keyvalue_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        // Key, optional quote, `:` or `=`, then the value (optionally quoted) —
        // redacts `password=hunter2`, `"access_token": "ya29.…"`, `Bearer: …`,
        // and bare `token=`/`key=` forms.
        Regex::new(r#"(?i)(\b(?:password|passwd|pwd|secret|client[_-]?secret|refresh[_-]?token|access[_-]?token|code[_-]?verifier|api[_-]?key|auth[_-]?token|authorization|bearer|token|key)\b["']?\s*[:=]\s*)(["']?)([^\s,;"']+)(["']?)"#)
            .expect("valid")
    })
}

fn home_path_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"[/\\](?:Users|home)[/\\][A-Za-z0-9._-]+").expect("valid"))
}

fn bearer_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\bbearer\s+[A-Za-z0-9._~+/=\-]+").expect("valid"))
}

fn jwt_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{4,}").expect("valid"))
}

fn long_token_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"[A-Za-z0-9+/_\-=]{40,}").expect("valid"))
}

/// Best-effort PII redaction over free text (panic messages, stack traces).
/// The structured-only design means this only ever sees error metadata — never
/// mail content — but it is applied at write time and again at the edge anyway.
pub fn redact(input: &str) -> String {
    let mut out = input.to_string();
    if let Some(home) = home_dir_path() {
        let home = home.to_string_lossy();
        out = out.replace(&*home, "[redacted:path]");
    }
    out = home_path_re().replace_all(&out, "[redacted:path]").into_owned();
    out = email_re().replace_all(&out, "[redacted:email]").into_owned();
    out = secret_keyvalue_re()
        .replace_all(&out, "${1}${2}[redacted:token]${4}")
        .into_owned();
    out = bearer_re().replace_all(&out, "[redacted:token]").into_owned();
    out = jwt_re().replace_all(&out, "[redacted:token]").into_owned();
    out = long_token_re().replace_all(&out, "[redacted:token]").into_owned();
    out
}

fn home_dir_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Capture a JS error from `window.onerror` / `onunhandledrejection` into the
/// local pending queue. Always recorded locally; only transmitted when the
/// opt-in is on and an endpoint is configured.
#[tauri::command]
pub fn report_js_error(payload: JsErrorPayload) -> Result<String, String> {
    let state = diag_state()?;
    let id = queue_js_error(state, payload)?;
    log::error!("JS error captured (report {id})");
    Ok(id)
}

/// Change the unified log level at runtime and persist it.
#[tauri::command]
pub fn set_log_level(app: AppHandle, level: String) -> Result<(), String> {
    if !is_valid_log_level(&level) {
        return Err(format!("unknown log level: {level}"));
    }
    apply_log_level(&level);
    let mut settings = settings::load(&app);
    settings.log_level = level;
    settings::save(&app, &settings)
}

/// Reveal the local log file in the OS file manager.
#[tauri::command]
pub fn open_logs_folder(app: AppHandle) -> Result<(), String> {
    let log_dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
    reveal_in_file_manager(&log_dir);
    Ok(())
}

/// Reveal the pending crash-reports directory in the OS file manager.
#[tauri::command]
pub fn open_crash_reports_folder() -> Result<(), String> {
    let dir = diag_state()?.pending_dir.clone();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    reveal_in_file_manager(&dir);
    Ok(())
}

/// Queue a synthetic panic record and, if the opt-in + endpoint allow, transmit
/// it immediately. Used to verify the pipeline end-to-end without a real crash.
#[tauri::command]
pub async fn send_test_report(app: AppHandle) -> Result<String, String> {
    let state = diag_state()?;
    let record = CrashRecord {
        id: new_id(),
        kind: "panic".into(),
        created_at_ms: now_ms(),
        app_version: state.app_version.clone(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        thread: Some("main".into()),
        message: "Test crash report — generated from Settings → Diagnostics".into(),
        stack: Some(
            redact(&std::backtrace::Backtrace::force_capture().to_string())
                .chars()
                .take(2000)
                .collect(),
        ),
        source: None,
        line: None,
        column: None,
    };
    let id = record.id.clone();
    let path = write_record(&state.pending_dir, &record).map_err(|e| e.to_string())?;

    let settings = settings::load(&app);
    if settings.crash_reporting_enabled && !CRASH_ENDPOINT.is_empty() {
        let client = http_client();
        let sent = upload_paths(&client, &state.sent_dir, CRASH_ENDPOINT, &[path]).await;
        if sent == 1 {
            return Ok(format!("Test report {id} sent to the configured endpoint."));
        }
        return Ok(format!("Test report {id} queued — upload failed (kept locally)."));
    }
    if !settings.crash_reporting_enabled {
        Ok(format!("Test report {id} written locally (crash reporting is off)."))
    } else {
        Ok(format!("Test report {id} queued locally (no endpoint configured)."))
    }
}

/// Upload any locally-queued reports now (used when the user flips the opt-in
/// on in Settings, so they don't wait for the next launch).
#[tauri::command]
pub async fn flush_pending_reports() -> Result<u32, String> {
    if CRASH_ENDPOINT.is_empty() {
        return Ok(0);
    }
    let state = diag_state()?;
    let client = http_client();
    Ok(upload_pending(&client, &state.pending_dir, &state.sent_dir, CRASH_ENDPOINT).await as u32)
}

/// Status readout for Settings → Diagnostics.
#[tauri::command]
pub fn get_diagnostics_info(app: AppHandle) -> Result<DiagnosticsInfo, String> {
    let state = diag_state()?;
    let settings = settings::load(&app);
    let log_file_path = app
        .path()
        .app_log_dir()
        .ok()
        .map(|d| d.join("quill.log").to_string_lossy().into_owned());
    Ok(DiagnosticsInfo {
        app_version: state.app_version.clone(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        channel: channel_of(&state.app_version),
        log_level: settings.log_level.clone(),
        crash_reporting_enabled: settings.crash_reporting_enabled,
        usage_ping_enabled: settings.usage_ping_enabled,
        pending_report_count: count_json_files(&state.pending_dir),
        log_file_path,
        crash_reports_dir: state.pending_dir.to_string_lossy().into_owned(),
        endpoint_configured: !CRASH_ENDPOINT.is_empty() || !USAGE_ENDPOINT.is_empty(),
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn new_id() -> String {
    Uuid::new_v4().to_string()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn reveal_in_file_manager(path: &Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state(tag: &str) -> DiagState {
        let dir = std::env::temp_dir().join(format!("quill-diag-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        DiagState {
            app_version: "0.1.0".into(),
            pending_dir: dir.join("pending"),
            sent_dir: dir.join("sent"),
        }
    }

    fn cleanup(state: &DiagState) {
        let dir = state.pending_dir.parent().unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    fn sample_payload(message: &str) -> JsErrorPayload {
        JsErrorPayload {
            message: message.into(),
            stack: Some("TypeError: undefined is not an object\n    at fn (file:///Users/me/app.js:12:3)".into()),
            source: Some("http://localhost:5173/index.tsx".into()),
            line: Some(12),
            column: Some(3),
        }
    }

    #[test]
    fn redact_removes_emails() {
        assert_eq!(
            redact("sync failed for alice@example.com"),
            "sync failed for [redacted:email]"
        );
        assert_eq!(
            redact("bob.o'brien+tag@sub.corp.org rejected"),
            "[redacted:email] rejected"
        );
    }

    #[test]
    fn redact_removes_tokens() {
        assert_eq!(
            redact("Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"),
            "[redacted:token]"
        );
        assert_eq!(redact("password=hunter2!"), "password=[redacted:token]");
        assert_eq!(redact("token=sekrit123"), "token=[redacted:token]");
        assert_eq!(redact("key=abc123"), "key=[redacted:token]");
        assert_eq!(
            redact(r#""access_token": "ya29.abc123def456""#),
            r#""access_token": "[redacted:token]""#
        );
        assert_eq!(
            redact("client_secret=uJd7Wp2kLq9sRz4xNc8vBm1a"),
            "client_secret=[redacted:token]"
        );
        // 40+ char opaque token without a key name.
        assert_eq!(
            redact("token abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJ"),
            "token [redacted:token]"
        );
        // Short ids (uuids, action ids) survive.
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(redact(uuid), uuid);
    }

    #[test]
    fn redact_removes_home_path() {
        if let Some(home) = home_dir_path() {
            let home = home.to_string_lossy().into_owned();
            let input = format!("{home}/projects/rmail/src/main.rs:42");
            assert_eq!(redact(&input), "[redacted:path]/projects/rmail/src/main.rs:42");
        }
    }

    #[test]
    fn redact_cleans_sync_log_templates() {
        // Representative of the log lines that used to interpolate account
        // addresses (PII) — nothing with an '@' or a token may survive.
        let samples = [
            "backfilled 12 bodies for alice@example.com",
            "sync alice@example.com failed: connection refused",
            "action replay for bob@corp.org: timeout",
            "sync carol@example.com INBOX: auth failed",
            "failed action replay action_123: error",
        ];
        for s in samples {
            let out = redact(s);
            assert!(!out.contains('@'), "email leaked: {s} -> {out}");
            assert!(!out.contains("hunter2"), "token leaked in {s}");
        }
    }

    #[test]
    fn queue_js_error_writes_scrubbed_record() {
        let state = test_state("queue");
        let id = queue_js_error(&state, sample_payload("boom at alice@example.com")).unwrap();
        assert_eq!(count_json_files(&state.pending_dir), 1);

        let path = state.pending_dir.join(format!("{id}.json"));
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(json.contains(r#""kind": "js_error""#));
        assert!(!json.contains('@'), "record leaked an email: {json}");
        assert!(!json.contains("Users/me"), "stack leaked a home path: {json}");
        cleanup(&state);
    }

    #[test]
    fn panic_hook_writes_scrubbed_record() {
        // The panic hook is installed once per process and reads a once-set
        // DIAG; guard both so this test is safe to run repeatedly.
        let state = test_state("hook");
        if DIAG.get().is_none() {
            let _ = DIAG.set(DiagState {
                app_version: "0.1.0".into(),
                pending_dir: state.pending_dir.clone(),
                sent_dir: state.sent_dir.clone(),
            });
        }
        install_panic_hook();

        let result = std::panic::catch_unwind(|| {
            panic!("boom at alice@example.com token=sekrit123");
        });
        assert!(result.is_err(), "panic should unwind after the hook runs");

        let files: Vec<_> = std::fs::read_dir(&state.pending_dir)
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert_eq!(files.len(), 1, "hook wrote one pending record");
        let json = std::fs::read_to_string(files[0].path()).unwrap();
        assert!(json.contains(r#""kind": "panic""#));
        assert!(!json.contains('@'), "record leaked an email: {json}");
        assert!(!json.contains("sekrit123"), "record leaked a token: {json}");
        cleanup(&state);
    }

    #[test]
    fn trim_pending_bounds_the_queue() {
        let state = test_state("trim");
        for i in 0..5 {
            queue_js_error(&state, sample_payload(&format!("err {i}"))).unwrap();
        }
        trim_pending(&state.pending_dir, 2);
        assert_eq!(count_json_files(&state.pending_dir), 2);
        cleanup(&state);
    }

    #[test]
    fn channel_of_classifies_pre_releases() {
        assert_eq!(channel_of("0.1.0"), "stable");
        assert_eq!(channel_of("0.2.0-beta.3"), "beta");
        assert_eq!(channel_of("0.2.0-rc.1"), "rc");
    }

    fn read_http_request(stream: &mut std::net::TcpStream) -> String {
        use std::io::Read;
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let n = stream.read(&mut chunk).unwrap();
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let head = String::from_utf8_lossy(&buf).into_owned();
        let content_length: usize = head
            .lines()
            .find_map(|l| {
                let mut it = l.split(':');
                if it.next().map(|k| k.eq_ignore_ascii_case("content-length")) == Some(true) {
                    it.next().and_then(|v| v.trim().parse().ok())
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let header_end = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        while buf.len() - header_end < content_length {
            let n = stream.read(&mut chunk).unwrap();
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        String::from_utf8_lossy(&buf).into_owned()
    }

    fn spawn_ok_listener() -> (String, std::sync::mpsc::Receiver<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            use std::io::Write;
            let (mut stream, _) = listener.accept().unwrap();
            let req = read_http_request(&mut stream);
            let _ = tx.send(req);
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
            );
        });
        (format!("http://{addr}"), rx)
    }

    #[test]
    fn upload_posts_and_moves_to_sent() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let state = test_state("upload");
            queue_js_error(&state, sample_payload("boom at alice@example.com")).unwrap();

            let (endpoint, rx) = spawn_ok_listener();
            let client = http_client();
            let n = upload_pending(&client, &state.pending_dir, &state.sent_dir, &endpoint).await;

            assert_eq!(n, 1);
            assert_eq!(count_json_files(&state.pending_dir), 0);
            assert_eq!(count_json_files(&state.sent_dir), 1);

            let req = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
            assert!(req.starts_with("POST "), "expected POST, got: {req}");
            let body = req.split("\r\n\r\n").nth(1).unwrap_or("");
            assert!(body.contains(r#""kind": "js_error""#), "body not a crash record: {body}");
            assert!(!body.contains('@'), "body leaked an email: {body}");
            cleanup(&state);
        });
    }

    #[test]
    fn usage_ping_posts_schema() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let state = test_state("ping");
            let (endpoint, rx) = spawn_ok_listener();
            let client = http_client();
            send_usage_ping(&client, &state, &endpoint).await.unwrap();

            let req = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
            let body = req.split("\r\n\r\n").nth(1).unwrap_or("");
            let json: serde_json::Value = serde_json::from_str(body).unwrap();
            assert_eq!(json["client"], "quill");
            assert_eq!(json["event"], "launch");
            assert_eq!(json["appVersion"], "0.1.0");
            assert!(json["os"].is_string());
            assert!(json["arch"].is_string());
            cleanup(&state);
        });
    }
}
