import { createEffect, createSignal, For, Show } from "solid-js";
import type { DiagnosticsInfo } from "../../lib/ipc/DiagnosticsInfo";
import { useSettings, updateSettings } from "../../lib/settings";
import {
  flushPendingReports,
  getDiagnosticsInfo,
  openCrashReportsFolder,
  openLogsFolder,
  sendTestReport,
  setLogLevel,
} from "../../lib/tauri";
import "../Settings.css";

// Diagnostics & privacy (Roadmap E2.3). Everything here is opt-in and default
// off; the "what we send" copy must match docs/telemetry.md exactly.

const LOG_LEVELS = ["error", "warn", "info", "debug", "trace"] as const;

export function DiagnosticsSection() {
  const settings = useSettings();
  const [hydrated, setHydrated] = createSignal(false);
  const [crashEnabled, setCrashEnabled] = createSignal(false);
  const [pingEnabled, setPingEnabled] = createSignal(false);
  const [logLevel, setLogLevelState] = createSignal<string>("info");
  const [info, setInfo] = createSignal<DiagnosticsInfo | null>(null);
  const [status, setStatus] = createSignal<string | null>(null);

  // Same hydration pattern as the other sections: settings may not be loaded
  // when the section first renders.
  createEffect(() => {
    if (hydrated()) return;
    const s = settings();
    if (s) {
      setCrashEnabled(s.crashReportingEnabled ?? false);
      setPingEnabled(s.usagePingEnabled ?? false);
      setLogLevelState(s.logLevel ?? "info");
      setHydrated(true);
    }
  });

  const refreshInfo = async () => {
    setInfo(await getDiagnosticsInfo());
  };

  createEffect(() => {
    if (hydrated()) void refreshInfo();
  });

  const toggleCrash = async (enabled: boolean) => {
    setCrashEnabled(enabled);
    setStatus(null);
    await updateSettings({ crashReportingEnabled: enabled });
    await refreshInfo();
    if (enabled) {
      // Nothing stays queued behind the opt-in: flush pending reports now.
      const sent = await flushPendingReports();
      if (sent > 0) setStatus(`Uploaded ${sent} previously queued report(s).`);
    }
  };

  const togglePing = async (enabled: boolean) => {
    setPingEnabled(enabled);
    setStatus(null);
    await updateSettings({ usagePingEnabled: enabled });
    await refreshInfo();
  };

  const changeLogLevel = async (level: string) => {
    setLogLevelState(level);
    setStatus(null);
    await setLogLevel(level);
    await refreshInfo();
  };

  const onSendTestReport = async () => {
    setStatus("…");
    setStatus(await sendTestReport());
    await refreshInfo();
  };

  return (
    <div class="general-settings">
      <div class="privacy-settings-group">
        <h3 class="privacy-settings-group__title">Diagnostics & privacy</h3>
        <p class="privacy-settings-group__desc">
          Quill is local-first: nothing leaves your machine unless you turn it
          on below. Each option lists exactly what is sent.
        </p>

        <div class="settings-row general-option">
          <label class="general-option__label">
            <input
              type="checkbox"
              checked={crashEnabled()}
              onChange={(e) => void toggleCrash(e.currentTarget.checked)}
            />
            <span class="general-option__text">
              <span class="general-option__title">Crash & error reporting</span>
              <span class="general-option__desc">
                Send a small technical report when Quill crashes or hits a
                JavaScript error
              </span>
            </span>
          </label>
        </div>
        <details class="diagnostics-detail">
          <summary>What is sent</summary>
          <p>
            Only app version, OS & architecture, thread, timestamp, and the
            error message and stack trace — with email addresses, tokens, and
            your home-directory path removed first. Never message content,
            account addresses, or credentials. Reports are always kept locally
            in the crash-reports folder; they are uploaded only while this is on
            and an endpoint is configured.
          </p>
        </details>

        <div class="settings-row general-option">
          <label class="general-option__label">
            <input
              type="checkbox"
              checked={pingEnabled()}
              onChange={(e) => void togglePing(e.currentTarget.checked)}
            />
            <span class="general-option__text">
              <span class="general-option__title">
                Anonymous usage statistics
              </span>
              <span class="general-option__desc">
                Send an anonymous launch ping so we know if updates are landing
              </span>
            </span>
          </label>
        </div>
        <details class="diagnostics-detail">
          <summary>What is sent</summary>
          <p>
            One request per launch containing only app version, OS &
            architecture, and release channel. No email address, no identifier,
            no message or calendar data.
          </p>
        </details>
      </div>

      <div class="settings-row general-option general-option--undo">
        <div class="general-option__text">
          <span class="general-option__title">Log level</span>
          <span class="general-option__desc">
            Verbosity of the local log file (Rust and JavaScript share one
            pipeline)
          </span>
        </div>
        <select
          class="settings-select"
          value={logLevel()}
          onChange={(e) => void changeLogLevel(e.currentTarget.value)}
        >
          <For each={LOG_LEVELS}>
            {(level) => <option value={level}>{level}</option>}
          </For>
        </select>
      </div>

      <Show when={info()}>
        {(i) => (
          <dl class="diagnostics-info">
            <div>
              <dt>Version</dt>
              <dd>
                {i().appVersion} · {i().os} {i().arch} · {i().channel}
              </dd>
            </div>
            <div>
              <dt>Log file</dt>
              <dd>
                {i().logFilePath ??
                  "(no log file — logs go to the console in this build)"}
              </dd>
            </div>
            <div>
              <dt>Queued reports</dt>
              <dd>
                {i().pendingReportCount} locally in{" "}
                <code class="diagnostics-code">{i().crashReportsDir}</code>
              </dd>
            </div>
            <div>
              <dt>Upload endpoint</dt>
              <dd>
                {i().endpointConfigured ? "configured" : "not configured"}
              </dd>
            </div>
          </dl>
        )}
      </Show>

      <div class="diagnostics-actions">
        <button
          type="button"
          class="btn btn--secondary btn--sm"
          onClick={() => void openLogsFolder()}
        >
          Open logs folder
        </button>
        <button
          type="button"
          class="btn btn--secondary btn--sm"
          onClick={() => void openCrashReportsFolder()}
        >
          Open crash reports folder
        </button>
        <button
          type="button"
          class="btn btn--secondary btn--sm"
          onClick={() => void onSendTestReport()}
        >
          Send test report
        </button>
      </div>

      <Show when={status()}>
        {(s) => <p class="diagnostics-status">{s()}</p>}
      </Show>
    </div>
  );
}
