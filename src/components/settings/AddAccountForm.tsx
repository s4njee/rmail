import { createSignal, For, Show } from "solid-js";
import { refreshMail } from "../../lib/mail";
import { addAccount, testConnection } from "../../lib/tauri";
import "../Settings.css";

const SYNC_MODES = ["every 2 min", "on open", "manual"];

// The account add form (Epic 10.4) — extrapolated from the tokens, flagged
// for design review. The password is sent straight into the keychain command
// and cleared here immediately after; it is never kept in app state.
export function AddAccountForm(props: {
  onDone: () => void;
  onCancel: () => void;
}) {
  const [address, setAddress] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [protocol, setProtocol] = createSignal<"IMAP" | "Bridge">("IMAP");
  const [server, setServer] = createSignal("");
  const [port, setPort] = createSignal(993);
  const [tls, setTls] = createSignal(true);
  const [syncMode, setSyncMode] = createSignal(SYNC_MODES[0]);
  const [testing, setTesting] = createSignal<
    "idle" | "testing" | "ok" | "fail"
  >("idle");
  const [testError, setTestError] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [saveError, setSaveError] = createSignal("");

  const runTest = async () => {
    setTesting("testing");
    try {
      await testConnection(server(), port());
      setTesting("ok");
      setTestError("");
    } catch (error) {
      setTesting("fail");
      setTestError(String(error));
    }
  };

  const save = async () => {
    setSaving(true);
    setSaveError("");
    try {
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
      setPassword(""); // never linger in JS state
      await refreshMail();
      props.onDone();
    } catch (error) {
      setSaveError(String(error));
      setSaving(false);
    }
  };

  return (
    <form
      class="add-account"
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      <label class="add-field">
        <span>Address</span>
        <input
          type="email"
          value={address()}
          onInput={(e) => setAddress(e.currentTarget.value)}
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
          required
        />
      </label>
      <div class="add-field--row">
        <label class="add-field">
          <span>Protocol</span>
          <select
            value={protocol()}
            onChange={(e) =>
              setProtocol(e.currentTarget.value as "IMAP" | "Bridge")
            }
          >
            <option>IMAP</option>
            <option>Bridge</option>
          </select>
        </label>
        <label class="add-field">
          <span>Sync</span>
          <select
            value={syncMode()}
            onChange={(e) => setSyncMode(e.currentTarget.value)}
          >
            <For each={SYNC_MODES}>{(mode) => <option>{mode}</option>}</For>
          </select>
        </label>
      </div>
      <div class="add-field--row">
        <label class="add-field add-field--grow">
          <span>Server</span>
          <input
            type="text"
            value={server()}
            onInput={(e) => setServer(e.currentTarget.value)}
            placeholder="imap.example.com"
            required
          />
        </label>
        <label class="add-field add-field--port">
          <span>Port</span>
          <input
            type="number"
            value={port()}
            onInput={(e) => setPort(Number(e.currentTarget.value))}
            required
          />
        </label>
        <label class="add-field add-field--check">
          <span>TLS</span>
          <input
            type="checkbox"
            checked={tls()}
            onChange={(e) => setTls(e.currentTarget.checked)}
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
        <Show when={testing() === "ok"}>
          <span class="add-account__test add-account__test--ok">
            Connection OK
          </span>
        </Show>
        <Show when={testing() === "fail"}>
          <span class="add-account__test add-account__test--fail">
            {testError()}
          </span>
        </Show>
      </div>

      <Show when={saveError()}>
        <div class="add-account__test add-account__test--fail">
          {saveError()}
        </div>
      </Show>

      <div class="add-account__actions">
        <button type="submit" class="btn btn--primary" disabled={saving()}>
          Save account
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
  );
}
