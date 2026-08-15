import { createSignal, For, Show, untrack } from "solid-js";
import type { Account } from "../../lib/ipc/Account";
import type { ConnectionIssue } from "../../lib/ipc/ConnectionIssue";
import type { ConnectionTestReport } from "../../lib/ipc/ConnectionTestReport";
import { refreshMail } from "../../lib/mail";
import {
  addAccount,
  discoverSettings,
  exchangeOAuthCode,
  getOAuthInit,
  testConnectionSettings,
  updateAccount,
  waitOAuthCode,
} from "../../lib/tauri";
import "../Settings.css";

const SYNC_MODES = ["every 2 min", "on open", "manual"];

// Pulls the `code` out of a pasted redirect URL (e.g.
// http://127.0.0.1:8080?code=4/0A…&scope=…) so users can paste the whole
// address-bar URL instead of fishing out the code by hand. Falls back to the
// raw input for already-extracted codes. `URL.searchParams` also handles
// percent-decoding of the code.
function extractAuthCode(raw: string): string {
  const trimmed = raw.trim();
  if (trimmed.startsWith("http://") || trimmed.startsWith("https://")) {
    try {
      const code = new URL(trimmed).searchParams.get("code");
      if (code) return code;
    } catch {
      /* not a parseable URL — fall through to raw input */
    }
  }
  return trimmed;
}

// Render one connection issue as an actionable line, with provider help when
// the server tagged it (P0.2).
function issueText(issue: ConnectionIssue): string {
  const service =
    issue.service === "caldav" ? "CalDAV" : issue.service.toUpperCase();
  return `${service} (${issue.server}): ${issue.detail}${
    issue.help ? ` — ${issue.help}` : ""
  }`;
}

// The account add form (Epic 10.4 & Roadmap 3.1) — supports standard IMAP,
// Bridge, CalDAV, and OAuth2 quick-connect for Google and Microsoft 365. When
// passed an `account`, it becomes the edit form (address/protocol immutable,
// password optional). P0.2 additions: autodiscovery prefill, automatic OAuth
// redirect capture, structured connection tests, and provider app-password
// help at the point of failure.
export function AddAccountForm(props: {
  onDone: () => void;
  onCancel: () => void;
  account?: Account;
}) {
  // The modal is re-created per account, so snapshot the prop once (no tracking).
  const initialAccount = untrack(() => props.account);
  const editing = () => initialAccount != null;
  const [address, setAddress] = createSignal(initialAccount?.address ?? "");
  const [password, setPassword] = createSignal("");
  const [protocol, setProtocol] = createSignal<string>(
    initialAccount?.protocol ?? "IMAP",
  );
  const [server, setServer] = createSignal(initialAccount?.server ?? "");
  const [port, setPort] = createSignal(initialAccount?.port ?? 993);
  const [tls, setTls] = createSignal(initialAccount?.tls ?? true);
  const [syncMode, setSyncMode] = createSignal(
    initialAccount?.sync_mode ?? SYNC_MODES[0],
  );
  const [color, setColor] = createSignal(initialAccount?.color ?? "#3b5bdb");
  const [testing, setTesting] = createSignal<
    "idle" | "testing" | "ok" | "fail"
  >("idle");
  const [testReport, setTestReport] = createSignal<ConnectionTestReport | null>(
    null,
  );
  const [saving, setSaving] = createSignal(false);
  const [saveError, setSaveError] = createSignal("");

  // Autodiscovery (P0.2): prefills server/port/TLS from the address's domain
  // and surfaces provider app-password help.
  const [providerHelp, setProviderHelp] = createSignal("");
  const [discovering, setDiscovering] = createSignal(false);
  const [serverTouched, setServerTouched] = createSignal(false);

  // OAuth state
  const [oauthStep, setOauthStep] = createSignal<
    "idle" | "awaiting_code" | "exchanging"
  >("idle");
  const [oauthProvider, setOauthProvider] = createSignal<
    "google" | "microsoft365"
  >("google");
  const [oauthVerifier, setOauthVerifier] = createSignal("");
  const [oauthRedirect, setOauthRedirect] = createSignal("");
  const [oauthCodeInput, setOauthCodeInput] = createSignal("");
  const [oauthAuthUrl, setOauthAuthUrl] = createSignal("");
  const [oauthClientId, setOauthClientId] = createSignal("");
  const [oauthClientSecret, setOauthClientSecret] = createSignal("");
  const [oauthClientIdUsed, setOauthClientIdUsed] = createSignal("");
  const [oauthWaiting, setOauthWaiting] = createSignal(false);

  // Autodiscover the address's domain and prefill the manual fields (create
  // mode only — editing an existing account must not change its servers).
  const autodiscover = async () => {
    const addr = address().trim();
    if (!addr.includes("@") || editing()) return;
    setDiscovering(true);
    try {
      const d = await discoverSettings(addr);
      if (d.imap && !serverTouched()) {
        setServer(d.imap.host);
        setPort(d.imap.port);
        setTls(d.imap.tls);
      }
      if (d.provider) {
        setProviderHelp(d.provider.help);
      }
    } catch {
      /* discovery failure just leaves the manual fields as-is */
    } finally {
      setDiscovering(false);
    }
  };

  const startOAuth = async (provider: "google" | "microsoft365") => {
    setSaveError("");
    try {
      setOauthProvider(provider);
      const init = await getOAuthInit(
        provider,
        oauthClientId().trim() || undefined,
      );
      setOauthVerifier(init.code_verifier);
      setOauthRedirect(init.redirect_uri);
      setOauthAuthUrl(init.auth_url);
      setOauthClientIdUsed(init.client_id);
      setOauthStep("awaiting_code");

      // Open auth url in browser if window.open is available
      if (typeof window !== "undefined" && window.open) {
        window.open(init.auth_url, "_blank");
      }

      // P0.2: the loopback listener captures the redirect automatically; the
      // paste box is the fallback when the browser didn't complete in time.
      setOauthWaiting(true);
      const result = await waitOAuthCode(init.redirect_uri, init.state);
      setOauthWaiting(false);
      if (result.ok && result.code) {
        await finishOAuthWithCode(provider, result.code);
      } else {
        setSaveError(
          result.error ||
            "Waiting for the browser sign-in timed out — paste the code below.",
        );
      }
    } catch (e) {
      setOauthWaiting(false);
      setSaveError(String(e));
    }
  };

  const finishOAuthWithCode = async (
    provider: "google" | "microsoft365",
    code: string,
  ) => {
    setOauthStep("exchanging");
    setSaveError("");
    try {
      await exchangeOAuthCode(
        provider,
        extractAuthCode(code),
        oauthVerifier(),
        oauthRedirect(),
        oauthClientId().trim() || undefined,
        oauthClientSecret().trim() || undefined,
      );
      await refreshMail();
      props.onDone();
    } catch (e) {
      setSaveError(String(e));
      setOauthStep("awaiting_code");
    }
  };

  const finishOAuth = async () => {
    await finishOAuthWithCode(oauthProvider(), oauthCodeInput());
  };

  const handleProtocolChange = (next: "IMAP" | "Bridge" | "CalDAV") => {
    setProtocol(next);
    if (next === "CalDAV") {
      setPort(443);
      setTls(true);
    } else if (next === "IMAP") {
      setPort(993);
      setTls(true);
    } else {
      setPort(1143);
      setTls(false);
    }
  };

  // Full connection test (P0.2): resolve → TCP → TLS → greeting → auth, with
  // per-issue kind + provider help. Inputs stay editable and a Retry re-runs.
  const runTest = async () => {
    setTesting("testing");
    setTestReport(null);
    try {
      const report = await testConnectionSettings(
        {
          email: address().trim(),
          protocol: "imap",
          server: server().trim(),
          port: port(),
          tls: tls(),
        },
        password(),
      );
      setTesting(report.ok ? "ok" : "fail");
      setTestReport(report);
    } catch (error) {
      setTesting("fail");
      setTestReport({
        ok: false,
        authed: false,
        issues: [
          {
            service: "imap",
            server: server().trim(),
            kind: "protocol",
            detail: String(error),
            help: null,
          },
        ],
        detail: "",
      });
    }
  };

  const save = async () => {
    setSaving(true);
    setSaveError("");
    try {
      const acc = initialAccount;
      if (acc) {
        await updateAccount(
          {
            id: acc.id,
            server: server(),
            port: port(),
            tls: tls(),
            syncMode: syncMode(),
            color: color(),
          },
          password(), // empty = keep the current stored credential
        );
      } else {
        await addAccount(
          {
            address: address(),
            protocol: protocol(),
            server: server(),
            port: port(),
            tls: tls(),
            sync_mode: syncMode(),
          },
          password(), // straight into the keychain command
        );
      }
      setPassword(""); // never linger in JS state
      await refreshMail();
      props.onDone();
    } catch (error) {
      setSaveError(String(error));
      setSaving(false);
    }
  };

  return (
    <div class="add-account-container">
      <Show
        when={oauthStep() !== "idle"}
        fallback={
          <form
            class="add-account"
            onSubmit={(event) => {
              event.preventDefault();
              void save();
            }}
          >
            {/* Quick OAuth Connect (create only — editing an existing account
                doesn't re-run the OAuth flow) */}
            <Show when={!editing()}>
              <div class="add-oauth-section">
                <span class="add-oauth-label">Sign in with provider</span>
                <div class="add-oauth-buttons">
                  <button
                    type="button"
                    class="btn btn--secondary add-oauth-btn"
                    onClick={() => void startOAuth("google")}
                    disabled={oauthWaiting()}
                  >
                    Sign in with Google
                  </button>
                  <button
                    type="button"
                    class="btn btn--secondary add-oauth-btn"
                    onClick={() => void startOAuth("microsoft365")}
                    disabled={oauthWaiting()}
                  >
                    Sign in with Microsoft 365
                  </button>
                </div>
                <label class="add-field add-oauth-client-id">
                  <span>OAuth Client ID (optional)</span>
                  <input
                    type="text"
                    value={oauthClientId()}
                    onInput={(e) => setOauthClientId(e.currentTarget.value)}
                    placeholder="your-client-id.apps.googleusercontent.com"
                    autocomplete="off"
                    spellcheck={false}
                  />
                </label>
                <label class="add-field">
                  <span>OAuth Client Secret (optional)</span>
                  <input
                    type="password"
                    value={oauthClientSecret()}
                    onInput={(e) => setOauthClientSecret(e.currentTarget.value)}
                    placeholder="GOCSPX-…"
                    autocomplete="off"
                  />
                </label>
                <p class="add-oauth-hint">
                  Leave blank to use the test credentials from{" "}
                  <code>oauth-config.json</code> at the project root (gitignored
                  — see <code>oauth-config.example.json</code>). Or paste a
                  Google <b>Desktop app</b> OAuth client's ID and secret here to
                  override them.
                </p>
              </div>
            </Show>
            <div class="add-divider">
              <span>{editing() ? "edit account" : "or connect manually"}</span>
            </div>
            <label class="add-field">
              <span>Address</span>
              <input
                type="email"
                value={address()}
                onInput={(e) => {
                  setAddress(e.currentTarget.value);
                  setServerTouched(false);
                }}
                onBlur={() => void autodiscover()}
                disabled={editing()}
                required
              />
            </label>
            <label class="add-field">
              <span>Password</span>
              <input
                type="password"
                value={password()}
                onInput={(e) => setPassword(e.currentTarget.value)}
                autocomplete="off"
                placeholder={editing() ? "Leave blank to keep current" : ""}
                required={!editing()}
              />
            </label>
            <Show when={discovering()}>
              <p class="add-oauth-hint">
                Looking up settings for this address…
              </p>
            </Show>
            <Show when={providerHelp() && !editing()}>
              <p class="add-app-help">{providerHelp()}</p>
            </Show>
            <div class="add-field--row">
              <label class="add-field">
                <span>Protocol</span>
                <select
                  value={protocol()}
                  disabled={editing()}
                  onChange={(e) =>
                    handleProtocolChange(
                      e.currentTarget.value as "IMAP" | "Bridge" | "CalDAV",
                    )
                  }
                >
                  <option>IMAP</option>
                  <option>CalDAV</option>
                  <option>Bridge</option>
                </select>
              </label>
              <label class="add-field">
                <span>Sync</span>
                <select
                  value={syncMode()}
                  onChange={(e) => setSyncMode(e.currentTarget.value)}
                >
                  <For each={SYNC_MODES}>
                    {(mode) => <option>{mode}</option>}
                  </For>
                </select>
              </label>
            </div>
            <Show when={editing()}>
              <label class="add-field">
                <span>Color</span>
                <input
                  type="text"
                  value={color()}
                  onInput={(e) => setColor(e.currentTarget.value)}
                  pattern="^#[0-9a-fA-F]{6}$"
                  placeholder="#3b5bdb"
                />
              </label>
            </Show>
            <div class="add-field--row">
              <label class="add-field add-field--grow">
                <span>Server</span>
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

            <div class="add-account__actions">
              <button
                type="button"
                class="btn btn--secondary"
                onClick={() => void runTest()}
                disabled={testing() === "testing"}
              >
                {testing() === "testing" ? "Testing…" : "Test connection"}
              </button>
              <Show when={testing() === "ok" && testReport()?.ok}>
                <span class="add-account__test add-account__test--ok">
                  {testReport()?.detail || "Connection OK"}
                </span>
              </Show>
              <Show when={testReport() && !testReport()?.ok}>
                <span class="add-account__test add-account__test--fail">
                  {testReport()?.issues.map(issueText).join(" ")}
                </span>
                <button
                  type="button"
                  class="btn btn--secondary"
                  onClick={() => void runTest()}
                >
                  Retry
                </button>
              </Show>
            </div>

            <Show when={saveError()}>
              <div class="add-account__test add-account__test--fail">
                {saveError()}
              </div>
            </Show>

            <div class="add-account__actions">
              <button
                type="submit"
                class="btn btn--primary"
                disabled={saving()}
              >
                {editing() ? "Save changes" : "Save account"}
              </button>
              <button
                type="button"
                class="btn btn--secondary"
                onClick={() => props.onCancel()}
              >
                Cancel
              </button>
            </div>
          </form>
        }
      >
        {/* Awaiting OAuth Authorization Code */}
        <div class="add-oauth-flow">
          <div class="add-oauth-title">
            Sign in with{" "}
            {oauthProvider() === "google" ? "Google" : "Microsoft 365"}
          </div>
          <p class="add-oauth-desc">
            {oauthWaiting()
              ? "A sign-in window has been opened in your browser. Quill picks up the sign-in automatically — no need to paste anything."
              : "A sign-in window has been opened in your browser. (If it didn't open, "}
            {!oauthWaiting() && (
              <>
                <a
                  href={oauthAuthUrl()}
                  target="_blank"
                  rel="noopener noreferrer"
                  class="link"
                >
                  click here to open
                </a>
                ). After approving access, paste the authorization code or
                redirect URL below:
              </>
            )}
          </p>

          <p class="add-oauth-clientid-used">
            Using OAuth client: <code>{oauthClientIdUsed()}</code>
          </p>

          <Show when={!oauthWaiting()}>
            <label class="add-field">
              <span>Authorization Code / URL</span>
              <input
                type="text"
                value={oauthCodeInput()}
                onInput={(e) => setOauthCodeInput(e.currentTarget.value)}
                placeholder="Paste code or redirect URL here…"
                disabled={oauthStep() === "exchanging"}
                required
              />
            </label>
          </Show>

          <Show when={saveError()}>
            <div class="add-account__test add-account__test--fail">
              {saveError()}
            </div>
          </Show>

          <div class="add-account__actions">
            <Show when={!oauthWaiting()}>
              <button
                type="button"
                class="btn btn--primary"
                onClick={() => void finishOAuth()}
                disabled={
                  !oauthCodeInput().trim() || oauthStep() === "exchanging"
                }
              >
                {oauthStep() === "exchanging"
                  ? "Connecting…"
                  : "Complete sign in"}
              </button>
            </Show>
            <button
              type="button"
              class="btn btn--secondary"
              onClick={() => {
                setOauthStep("idle");
                setOauthWaiting(false);
              }}
              disabled={oauthStep() === "exchanging"}
            >
              Back
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
}
