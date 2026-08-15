import { isTauri } from "./tauri";
import { reportJsError } from "./tauri";

// Diagnostics (Roadmap E2.3): JS error capture + unified logging bridge.
//
// JS records flow into the same collector as Rust `log` records through
// `@tauri-apps/plugin-log`, and `window.onerror`/`onunhandledrejection` queue
// scrubbed crash records locally (never message content). Capture is always
// on; *transmission* is the opt-in action — see Settings → Diagnostics and
// docs/telemetry.md.

/** Wire the global JS error handlers and (DEV) console bridge. Called once
 * from the app entry before render. A no-op in the browser preview. */
export function initTelemetry(): void {
  if (!isTauri()) return;

  // Forward console.* into the shared log file. DEV only: in release the
  // webview target is the capture path and we don't want every console line
  // bloating the on-disk log.
  if (import.meta.env.DEV) {
    void attachConsoleForward();
  }

  window.addEventListener("error", (event) => {
    void reportJsError({
      message: event.message || "Unknown error",
      stack: event.error instanceof Error ? (event.error.stack ?? null) : null,
      source: event.filename ?? null,
      line: event.lineno ?? null,
      column: event.colno ?? null,
    });
  });

  window.addEventListener("unhandledrejection", (event) => {
    const reason: unknown = event.reason;
    void reportJsError({
      message:
        reason instanceof Error
          ? reason.message
          : String(reason ?? "Unhandled promise rejection"),
      stack: reason instanceof Error ? (reason.stack ?? null) : null,
      source: null,
      line: null,
      column: null,
    });
  });
}

async function attachConsoleForward(): Promise<void> {
  try {
    const { attachConsole } = await import("@tauri-apps/plugin-log");
    await attachConsole();
  } catch {
    // Plugin unavailable (preview / older webview) — console stays native.
  }
}
