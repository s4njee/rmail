import { createSignal, For, Show } from "solid-js";
import type { Account } from "../../lib/ipc/Account";
import type { CalendarCollection } from "../../lib/ipc/CalendarCollection";
import type { ConnectionIssue } from "../../lib/ipc/ConnectionIssue";
import type { ConnectionTestReport } from "../../lib/ipc/ConnectionTestReport";
import type { DiscoveredSettings } from "../../lib/ipc/DiscoveredSettings";
import type { ProviderPreset } from "../../lib/ipc/ProviderPreset";
import type { ServerFolder } from "../../lib/ipc/ServerFolder";
import type { SyncedFolder } from "../../lib/ipc/SyncedFolder";
import { refreshMail } from "../../lib/mail";
import {
  addAccount,
  discoverCalDav,
  discoverMailFolders,
  discoverSettings,
  exchangeOAuthCode,
  getOAuthInit,
  listProviderPresets,
  onStoreEvent,
  removeCalendarSource,
  setSyncedFolders,
  syncAccountNow,
  testConnectionSettings,
  waitOAuthCode,
} from "../../lib/tauri";
import "../Settings.css";
import "./Onboarding.css";

// First-run onboarding (backlog.md P0.2): welcome/privacy → provider choice →
// authentication + discovery → choose folders/calendars → initial sync →
// usable inbox. Shown when the store has no accounts; a "Skip" sets the
// dismissal flag so the empty shell is reachable (dev).
type Step = "welcome" | "provider" | "connect" | "select" | "sync";

const STEP_ORDER: Step[] = ["welcome", "provider", "connect", "select", "sync"];

type OAuthSession = {
  provider: "google" | "microsoft365";
  authUrl: string;
  verifier: string;
  redirectUri: string;
  state: string;
  clientId: string;
};

function authBadge(p: ProviderPreset): string {
  if (p.auth === "oauth") return "Sign in with browser";
  if (p.auth === "app_password") return "App password";
  return "Password";
}

function kindLabel(kind: string): string {
  const map: Record<string, string> = {
    inbox: "Inbox",
    drafts: "Drafts",
    sent: "Sent",
    archive: "Archive",
    junk: "Junk",
    trash: "Trash",
    starred: "Starred",
  };
  return map[kind] ?? kind;
}

export function Onboarding(props: { onDone: () => void }) {
  const [step, setStep] = createSignal<Step>("welcome");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");

  // Provider choice
  const [presets, setPresets] = createSignal<ProviderPreset[]>([]);
  const [preset, setPreset] = createSignal<ProviderPreset | null>(null);

  // Password / manual connect form
  const [email, setEmail] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [server, setServer] = createSignal("");
  const [port, setPort] = createSignal(993);
  const [tls, setTls] = createSignal(true);
  const [serverTouched, setServerTouched] = createSignal(false);
  const [discovery, setDiscovery] = createSignal<DiscoveredSettings | null>(
    null,
  );
  const [testReport, setTestReport] = createSignal<ConnectionTestReport | null>(
    null,
  );

  // OAuth
  const [oauthSession, setOauthSession] = createSignal<OAuthSession | null>(
    null,
  );
  const [oauthCode, setOauthCode] = createSignal("");
  const [showPaste, setShowPaste] = createSignal(false);

  // Created account + what to sync
  const [account, setAccount] = createSignal<Account | null>(null);
  const [folders, setFolders] = createSignal<ServerFolder[]>([]);
  const [enabledFolders, setEnabledFolders] = createSignal<Set<string>>(
    new Set(),
  );
  const [calendars, setCalendars] = createSignal<CalendarCollection[]>([]);
  const [enabledCalendars, setEnabledCalendars] = createSignal<Set<string>>(
    new Set(),
  );

  // Initial-sync progress
  const [syncState, setSyncState] = createSignal<"idle" | "syncing" | "done">(
    "idle",
  );
  const [downloaded, setDownloaded] = createSignal(0);

  const gotoProvider = () => {
    setStep("provider");
    if (presets().length === 0) {
      void listProviderPresets()
        .then(setPresets)
        .catch((e) => setError(String(e)));
    }
  };

  // --- OAuth path (Gmail / Microsoft 365) ----------------------------------

  const startOAuth = async (provider: "google" | "microsoft365") => {
    setError("");
    setBusy(true);
    try {
      const init = await getOAuthInit(provider);
      setOauthSession({
        provider,
        authUrl: init.auth_url,
        verifier: init.code_verifier,
        redirectUri: init.redirect_uri,
        state: init.state,
        clientId: init.client_id,
      });
      setStep("connect");
      window.open(init.auth_url, "_blank");
      // The loopback listener captures the redirect automatically; the 90s
      // wait falls back to the paste-the-code box.
      const result = await waitOAuthCode(init.redirect_uri, init.state);
      if (result.ok && result.code) {
        await finishOAuthCode(result.code);
      } else {
        setShowPaste(true);
        setError(
          result.error ||
            "Waiting for the browser sign-in timed out — paste the code below.",
        );
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const finishOAuthCode = async (code: string) => {
    const s = oauthSession();
    if (!s) return;
    setBusy(true);
    setError("");
    try {
      const acc = await exchangeOAuthCode(
        s.provider,
        code,
        s.verifier,
        s.redirectUri,
        s.clientId,
      );
      setAccount(acc);
      setEmail(acc.address);
      await loadSelection(acc);
    } catch (e) {
      setError(String(e));
      setShowPaste(true);
    } finally {
      setBusy(false);
    }
  };

  // --- Password / manual path ----------------------------------------------

  const choosePreset = (p: ProviderPreset) => {
    setPreset(p);
    setError("");
    if (p.auth === "oauth") {
      void startOAuth(
        p.oauth_provider === "microsoft365" ? "microsoft365" : "google",
      );
    } else {
      setServer(p.imap.host);
      setPort(p.imap.port);
      setTls(p.imap.tls);
      setStep("connect");
    }
  };

  const chooseOther = () => {
    setPreset(null);
    setError("");
    setStep("connect");
  };

  const runDiscovery = async () => {
    const em = email().trim();
    if (!em.includes("@")) return;
    setBusy(true);
    setError("");
    try {
      const d = await discoverSettings(em);
      setDiscovery(d);
      if (d.imap && !serverTouched()) {
        setServer(d.imap.host);
        setPort(d.imap.port);
        setTls(d.imap.tls);
      }
      if (d.provider && !preset()) {
        setPreset(d.provider);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const runTest = async () => {
    setBusy(true);
    setError("");
    try {
      const report = await testConnectionSettings(
        {
          email: email().trim(),
          protocol: "imap",
          server: server().trim(),
          port: port(),
          tls: tls(),
        },
        password(),
      );
      setTestReport(report);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const connectPassword = async () => {
    setBusy(true);
    setError("");
    try {
      const protocol = preset()?.id === "proton" ? "Bridge" : "IMAP";
      const acc = await addAccount(
        {
          address: email().trim(),
          protocol,
          server: server().trim(),
          port: port(),
          tls: tls(),
          sync_mode: "every 2 min",
        },
        password(),
      );
      setAccount(acc);
      await loadSelection(acc);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  // --- What to sync --------------------------------------------------------

  const loadSelection = async (acc: Account) => {
    setStep("select");
    setBusy(true);
    setError("");
    try {
      const p = preset();
      const pwd = password().trim() || undefined;
      const f = await discoverMailFolders(
        acc.address,
        acc.server,
        acc.port,
        acc.tls,
        pwd,
      );
      setFolders(f);
      setEnabledFolders(new Set(f.map((x) => x.serverName)));

      // CalDAV calendars (password/Caldav accounts only — OAuth calendars sync
      // via the provider API and are managed in Settings).
      if (p && p.auth !== "oauth" && p.caldav) {
        const cols = await discoverCalDav(
          `https://${p.caldav.host}`,
          acc.address,
          password(),
        );
        setCalendars(cols);
        setEnabledCalendars(new Set(cols.map((c) => c.href)));
      } else if (acc.protocol === "CalDAV") {
        const cols = await discoverCalDav(
          `https://${acc.server}`,
          acc.address,
          password(),
        );
        setCalendars(cols);
        setEnabledCalendars(new Set(cols.map((c) => c.href)));
      }
    } catch (e) {
      // Folder discovery failing isn't fatal for OAuth accounts (calendars and
      // mail still sync); surface it but stay on the step.
      setError(`Couldn't list folders/calendars: ${e}`);
    } finally {
      setBusy(false);
    }
  };

  const toggleFolder = (serverName: string) => {
    const next = new Set(enabledFolders());
    if (next.has(serverName)) next.delete(serverName);
    else next.add(serverName);
    setEnabledFolders(next);
  };

  const toggleCalendar = (href: string) => {
    const next = new Set(enabledCalendars());
    if (next.has(href)) next.delete(href);
    else next.add(href);
    setEnabledCalendars(next);
  };

  const saveSelection = async () => {
    const acc = account();
    if (!acc) return;
    setBusy(true);
    setError("");
    try {
      const sel: SyncedFolder[] = folders().map((f) => ({
        accountId: acc.id,
        serverName: f.serverName,
        localName: f.localName,
        kind: f.kind,
        enabled: enabledFolders().has(f.serverName),
      }));
      await setSyncedFolders(acc.id, sel);
      for (const c of calendars()) {
        if (!enabledCalendars().has(c.href)) {
          await removeCalendarSource(acc.id, c.href).catch(() => {});
        }
      }
      setPassword("");
      await refreshMail();
      setStep("sync");
      await runInitialSync(acc.id);
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  const runInitialSync = async (accountId: number) => {
    setSyncState("syncing");
    setDownloaded(0);
    let unsub: (() => void) | null = null;
    try {
      unsub = await onStoreEvent((event) => {
        if (event.kind === "mailChanged") setDownloaded((n) => n + 1);
      });
      await syncAccountNow(accountId);
      await refreshMail();
      // Let the "syncing" state paint before flipping to done.
      await new Promise((r) => setTimeout(r, 400));
      setSyncState("done");
    } finally {
      unsub?.();
    }
  };

  const skip = () => {
    try {
      localStorage.setItem("quill_setup_done", "1");
    } catch {
      /* ignore */
    }
    props.onDone();
  };

  const finish = () => {
    try {
      localStorage.setItem("quill_setup_done", "1");
    } catch {
      /* ignore */
    }
    void refreshMail();
    props.onDone();
  };

  const allSelectedFolders = () =>
    folders().length > 0 && enabledFolders().size === folders().length;

  // --- Render --------------------------------------------------------------

  return (
    <div class="onboarding" role="main" aria-label="Set up your accounts">
      <Show when={step() !== "welcome"}>
        <div class="onboarding__steps" aria-label="Setup progress">
          <For each={STEP_ORDER}>
            {(s, i) => (
              <span
                class="onboarding__step"
                classList={{
                  active: step() === s,
                  done: stepIndex(s) < stepIndex(step()),
                }}
              >
                {i() + 1}
              </span>
            )}
          </For>
        </div>
      </Show>

      <div class="onboarding__card">
        <Show when={step() === "welcome"}>
          <div class="onboarding__head">
            <div class="onboarding__logo" aria-hidden="true">
              Q
            </div>
            <h1 class="onboarding__title">Welcome to Quill</h1>
            <p class="onboarding__subtitle">
              Mail and calendar that live on this device.
            </p>
          </div>
          <ul class="onboarding__privacy">
            <li>
              <strong>Local-first.</strong> Your mail and calendar are stored on
              this computer, not in the cloud.
            </li>
            <li>
              <strong>Private by default.</strong> Nothing is sent anywhere
              unless you opt in — diagnostics and crash reporting stay off until
              you turn them on in Settings.
            </li>
            <li>
              <strong>Your accounts, your rules.</strong> Quill connects to your
              existing mail and calendar providers; you keep them.
            </li>
          </ul>
          <div class="onboarding__actions">
            <button
              type="button"
              class="btn btn--primary"
              onClick={gotoProvider}
            >
              Get started
            </button>
            <button type="button" class="onboarding__link-btn" onClick={skip}>
              Skip for now
            </button>
          </div>
        </Show>

        <Show when={step() === "provider"}>
          <div class="onboarding__head">
            <h1 class="onboarding__title">Add your first account</h1>
            <p class="onboarding__subtitle">
              Pick a provider — Quill will find the right servers for you.
            </p>
          </div>
          <div class="onboarding__providers">
            <For each={presets()}>
              {(p) => (
                <button
                  type="button"
                  class="onboarding__provider"
                  onClick={() => choosePreset(p)}
                >
                  <span class="onboarding__provider-name">{p.name}</span>
                  <span class="onboarding__provider-badge">{authBadge(p)}</span>
                </button>
              )}
            </For>
            <button
              type="button"
              class="onboarding__provider onboarding__provider--other"
              onClick={chooseOther}
            >
              <span class="onboarding__provider-name">
                Other / custom server
              </span>
              <span class="onboarding__provider-badge">
                Manual or autodiscover
              </span>
            </button>
          </div>
          <Show when={error()}>
            <div class="onboarding__error" role="alert">
              {error()}
            </div>
          </Show>
        </Show>

        <Show when={step() === "connect"}>
          <div class="onboarding__head">
            <h1 class="onboarding__title">
              {oauthSession()
                ? `Sign in with ${oauthSession()!.provider === "google" ? "Google" : "Microsoft 365"}`
                : preset()
                  ? `Connect ${preset()!.name}`
                  : "Connect your account"}
            </h1>
          </div>

          {/* OAuth flow */}
          <Show when={oauthSession()}>
            <p class="onboarding__desc">
              A sign-in window opened in your browser. After you approve access,
              Quill picks up the sign-in automatically.
            </p>
            <Show when={showPaste()}>
              <label class="add-field">
                <span>
                  Authorization code (paste from the browser's address bar)
                </span>
                <input
                  type="text"
                  value={oauthCode()}
                  onInput={(e) => setOauthCode(e.currentTarget.value)}
                  placeholder="4/0A…"
                  autocomplete="off"
                  spellcheck={false}
                />
              </label>
              <div class="add-account__actions">
                <button
                  type="button"
                  class="btn btn--primary"
                  disabled={!oauthCode().trim() || busy()}
                  onClick={() => void finishOAuthCode(oauthCode().trim())}
                >
                  Complete sign in
                </button>
              </div>
            </Show>
          </Show>

          {/* Password / manual flow */}
          <Show when={!oauthSession()}>
            <Show when={preset() && preset()!.auth === "app_password"}>
              <p class="onboarding__help">{preset()!.help}</p>
            </Show>
            <label class="add-field">
              <span>Email address</span>
              <input
                type="email"
                value={email()}
                onInput={(e) => {
                  setEmail(e.currentTarget.value);
                  setServerTouched(false);
                }}
                onBlur={() => void runDiscovery()}
                placeholder="you@example.com"
                autocomplete="username"
                required
              />
            </label>
            <label class="add-field">
              <span>
                {preset() && preset()!.auth === "app_password"
                  ? "App-specific password"
                  : "Password"}
              </span>
              <input
                type="password"
                value={password()}
                onInput={(e) => setPassword(e.currentTarget.value)}
                autocomplete="current-password"
                placeholder={
                  preset() && preset()!.auth === "app_password"
                    ? "xxxx xxxx xxxx xxxx"
                    : ""
                }
                required
              />
            </label>

            <Show when={discovery() && discovery()!.steps.length > 0}>
              <details class="onboarding__discovery">
                <summary>How Quill found your settings</summary>
                <ul>
                  <For each={discovery()!.steps}>
                    {(s) => (
                      <li
                        classList={{
                          ok: s.status === "ok",
                          err: s.status === "error",
                        }}
                      >
                        <span class="onboarding__discovery-src">
                          {s.source}
                        </span>{" "}
                        {s.detail}
                      </li>
                    )}
                  </For>
                </ul>
              </details>
            </Show>

            <div class="add-field--row">
              <label class="add-field add-field--grow">
                <span>IMAP server</span>
                <input
                  type="text"
                  value={server()}
                  onInput={(e) => {
                    setServer(e.currentTarget.value);
                    setServerTouched(true);
                  }}
                  placeholder="imap.example.com"
                  required
                />
              </label>
              <label class="add-field add-field--port">
                <span>Port</span>
                <input
                  type="number"
                  value={port()}
                  onInput={(e) => {
                    setPort(Number(e.currentTarget.value));
                    setServerTouched(true);
                  }}
                  required
                />
              </label>
              <label class="add-field add-field--check">
                <span>TLS</span>
                <input
                  type="checkbox"
                  checked={tls()}
                  onChange={(e) => {
                    setTls(e.currentTarget.checked);
                    setServerTouched(true);
                  }}
                />
              </label>
            </div>

            <Show when={testReport()}>
              {(report) => (
                <div
                  class={`onboarding__test ${report().ok ? "ok" : "fail"}`}
                  role="status"
                >
                  {report().ok
                    ? `Connection OK — ${report().detail}`
                    : formatIssues(report().issues)}
                </div>
              )}
            </Show>

            <div class="add-account__actions">
              <button
                type="button"
                class="btn btn--secondary"
                onClick={() => void runTest()}
                disabled={busy() || !server().trim()}
              >
                {busy() ? "Testing…" : "Test connection"}
              </button>
              <button
                type="button"
                class="btn btn--primary"
                onClick={() => void connectPassword()}
                disabled={busy() || !email().trim() || !server().trim()}
              >
                {busy() ? "Connecting…" : "Connect"}
              </button>
            </div>
          </Show>

          <Show when={error()}>
            <div class="onboarding__error" role="alert">
              {error()}
            </div>
          </Show>
        </Show>

        <Show when={step() === "select"}>
          <div class="onboarding__head">
            <h1 class="onboarding__title">What should Quill sync?</h1>
            <p class="onboarding__subtitle">
              Everything is selected by default. Turn off what you don't want
              downloaded to this device.
            </p>
          </div>

          <Show when={folders().length > 0}>
            <div class="onboarding__group-label">Mail folders</div>
            <div class="onboarding__check-list">
              <For each={folders()}>
                {(f) => (
                  <label class="onboarding__check">
                    <input
                      type="checkbox"
                      checked={enabledFolders().has(f.serverName)}
                      onChange={() => toggleFolder(f.serverName)}
                    />
                    <span>{kindLabel(f.localName)}</span>
                    <span class="onboarding__check-sub">{f.serverName}</span>
                  </label>
                )}
              </For>
            </div>
          </Show>

          <Show when={calendars().length > 0}>
            <div class="onboarding__group-label">Calendars</div>
            <div class="onboarding__check-list">
              <For each={calendars()}>
                {(c) => (
                  <label class="onboarding__check">
                    <input
                      type="checkbox"
                      checked={enabledCalendars().has(c.href)}
                      onChange={() => toggleCalendar(c.href)}
                    />
                    <span>{c.name}</span>
                    <span class="onboarding__check-sub">{c.href}</span>
                  </label>
                )}
              </For>
            </div>
          </Show>

          <Show when={folders().length === 0 && calendars().length === 0}>
            <p class="onboarding__desc">
              No folders could be listed {error() ? `(${error()})` : ""} — Quill
              will sync whatever the server exposes.
            </p>
          </Show>

          <div class="onboarding__actions">
            <button
              type="button"
              class="btn btn--primary"
              onClick={() => void saveSelection()}
              disabled={busy()}
            >
              {busy() ? "Saving…" : "Save & start syncing"}
            </button>
            <span class="onboarding__select-note">
              {allSelectedFolders()
                ? "All folders"
                : `${enabledFolders().size} folder(s) selected`}
            </span>
          </div>
          <Show when={error()}>
            <div class="onboarding__error" role="alert">
              {error()}
            </div>
          </Show>
        </Show>

        <Show when={step() === "sync"}>
          <div class="onboarding__sync">
            <div
              class="onboarding__spinner"
              classList={{ done: syncState() === "done" }}
              aria-hidden="true"
            />
            <h1 class="onboarding__title">
              {syncState() === "done" ? "You're all set" : "Syncing your mail…"}
            </h1>
            <p class="onboarding__subtitle">
              {syncState() === "done"
                ? "Quill is ready. You can start reading now."
                : `${downloaded()} update(s) so far — new mail appears as it downloads.`}
            </p>
            <Show when={syncState() === "done"}>
              <button type="button" class="btn btn--primary" onClick={finish}>
                Open your inbox
              </button>
            </Show>
          </div>
          <Show when={error()}>
            <div class="onboarding__error" role="alert">
              {error()}
            </div>
          </Show>
        </Show>
      </div>
    </div>
  );
}

function stepIndex(step: Step): number {
  return STEP_ORDER.indexOf(step);
}

function formatIssues(issues: ConnectionIssue[]): string {
  if (issues.length === 0) return "Connection failed.";
  const first = issues[0];
  const service =
    first.service === "caldav" ? "CalDAV" : first.service.toUpperCase();
  return `${service} (${first.server}): ${first.detail}${first.help ? ` — ${first.help}` : ""}`;
}
