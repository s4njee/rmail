# Diagnostics & telemetry (E2.3)

What Quill collects, what it sends, and exactly what bytes leave the machine.

**The short version: nothing is sent unless you turn it on, and the opt-ins are
off by default.** Crash reports and the usage ping are opt-in toggles under
**Settings → Diagnostics**. Local-first is a product feature: the default state
of the app is that it never makes a telemetry request.

There are three pieces, all in `src-tauri/src/diagnostics.rs`:

| Piece                                | Always on?               | Leaves the machine?                      |
| ------------------------------------ | ------------------------ | ---------------------------------------- |
| Unified logging (`tauri-plugin-log`) | Yes (local)              | Never                                    |
| Crash & error reporting              | Capture yes, transmit no | Only when opted in + endpoint configured |
| Usage ping                           | No                       | Only when opted in + endpoint configured |

---

## Unified logging

One logging pipeline for the whole app. Rust `log` macros and JavaScript
records (via `@tauri-apps/plugin-log`) flow into the same collector, which
writes **rotating files locally** (1 MB per file, kept on disk) plus stdout.

- **Location (macOS):** `~/Library/Logs/app.quill/quill.log`
- **Level:** controlled in Settings → Diagnostics; persists in `settings.json`
  as `log_level` (`error` | `warn` | `info` | `debug` | `trace`). The facade's
  max level is the single control for both Rust and JS records.
- **Log files never leave the machine.** They are for the user and for manual
  support ("Share diagnostics" is a future feature).
- **Never logged:** credentials, OAuth tokens, account email addresses, and
  message content. The old sync logs interpolated account addresses; they now
  log account ids. A test (`redact_cleans_sync_log_templates` and the
  `redact` unit tests) guards that no address survives the scrubber.

---

## Crash & error reporting

Opt-in (Settings → Diagnostics → **Crash & error reporting**). Captures Rust
panics and uncaught JavaScript errors.

### What is captured, always, locally

Every panic and uncaught JS error is written as a JSON record to
`app_data_dir/crash_reports/pending/` — scrubbed **before it touches disk**.
JS-error capture is capped at 200 pending records to bound disk use.

### What is sent

Transmission happens only when **both** hold: the toggle is on **and** a
build-time endpoint is configured. On enabling the toggle, any already-queued
reports are flushed immediately; otherwise pending reports upload at the next
launch. Successful uploads move to `crash_reports/sent/`; failures stay in
`pending/` for a later attempt.

A report is **structured metadata only** — exactly this, nothing else:

```jsonc
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "kind": "panic" | "js_error",
  "createdAtMs": 1760000000000,
  "appVersion": "0.1.0",
  "os": "macos",             // std::env::consts::OS
  "arch": "aarch64",         // std::env::consts::ARCH
  "thread": "main",          // panics only
  "message": "…",            // scrubbed error message
  "stack": "… | null",       // scrubbed stack trace, panics only
  "source": null,            // JS only
  "line": null,              // JS only
  "column": null             // JS only
}
```

- **No message content, no account addresses, no credentials, no folder
  names, no log text.** The `message`/`stack` fields are the only free text,
  and both pass through the PII redactor.
- The redactor replaces, in this order:
  - your home-directory path (and any `/Users/<name>` / `/home/<name>` shaped
    path) → `[redacted:path]`
  - email addresses → `[redacted:email]`
  - `password=` / `client_secret=` / `refresh_token=` / `access_token=` /
    `code_verifier=` / `api_key=` / `authorization` / `bearer` / `token=` /
    `key=` values, JWT tokens, and 40+ char opaque token runs → `[redacted:token]`
- A panic under the release build's `panic = "abort"` still records the report,
  then the process exits. Reports written from a panic contain a backtrace
  captured with `Backtrace::force_capture()` (function names resolve because
  the release profile keeps `strip = "debuginfo"`).

---

## Usage ping

Opt-in (Settings → Diagnostics → **Anonymous usage statistics**). The point is
to know whether updates land — so the payload is just enough to correlate a
version with a platform. **One request per launch**, sent after startup when
the toggle is on and an endpoint is configured:

```jsonc
{
  "client": "quill",
  "event": "launch",
  "appVersion": "0.1.0",
  "os": "macos",
  "arch": "aarch64",
  "channel": "stable", // "stable", or the pre-release tag ("beta", "rc", …)
}
```

No email address, no install id, no identifier of any kind, no message or
calendar data.

---

## Endpoint configuration

Endpoints come from **build-time environment variables**. If unset (the
default), sending is disabled and everything stays local — matching how the
updater's endpoint is a placeholder until a real server exists.

```sh
# CI / release build
QUILL_CRASH_ENDPOINT=https://telemetry.example.com/v1/crash \
QUILL_USAGE_ENDPOINT=https://telemetry.example.com/v1/ping \
cargo tauri build
```

The server contract is: `POST` the documented JSON payloads, reply `2xx` to
acknowledge. `docs/telemetry.md` is the schema reference — a receiving service
should validate against it.

## Where the knobs live

- **Settings → Diagnostics:** both toggles, the log level, "Open logs folder",
  "Open crash reports folder", "Send test report" (verifies the pipeline
  end-to-end without a real crash), and a read-only status readout (version,
  OS/arch/channel, queued-report count, endpoint status).
- **settings.json** (`app_config_dir`): `crashReportingEnabled` and
  `usagePingEnabled` (both default `false`), `logLevel` (default `"info"`).
