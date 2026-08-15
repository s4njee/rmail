import { createEffect, createSignal, For, onMount, Show } from "solid-js";
import type { SearchIndexUpdate } from "../../lib/ipc/SearchIndexUpdate";
import { loadRows } from "../../lib/mail";
import { useSettings, updateSettings } from "../../lib/settings";
import { useSearchIndex } from "../../lib/store-events";
import {
  cancelSearchRebuild,
  isLaunchAtLogin,
  rebuildSearchIndexProgress,
  searchIndexStatus,
  setLaunchAtLogin,
} from "../../lib/tauri";
import "../Settings.css";

export function GeneralSection() {
  const settings = useSettings();
  const [threading, setThreading] = createSignal(true);
  const [undoDelay, setUndoDelay] = createSignal(10);
  const [blockImages, setBlockImages] = createSignal(true);
  const [trustedSenders, setTrustedSenders] = createSignal<string[]>([]);
  const [newSenderInput, setNewSenderInput] = createSignal("");
  // P1.3 search-index freshness readout + rebuild progress.
  const [indexStatus, setIndexStatus] = createSignal<SearchIndexUpdate | null>(
    null,
  );
  const searchIndexEvent = useSearchIndex();
  onMount(() => void searchIndexStatus().then(setIndexStatus));
  createEffect(() => {
    const ev = searchIndexEvent();
    if (ev) setIndexStatus(ev);
  });
  // P1.5 launch at login (tauri-plugin-autostart).
  const [launchAtLogin, setLaunchAtLoginState] = createSignal(false);
  onMount(() => void isLaunchAtLogin().then(setLaunchAtLoginState));
  const toggleLaunchAtLogin = async (next: boolean) => {
    setLaunchAtLoginState(next);
    try {
      await setLaunchAtLogin(next);
    } catch {
      setLaunchAtLoginState(!next);
    }
  };

  // Same hydration pattern as NotificationsSection: settings may not be loaded
  // when the section first renders.
  const [hydrated, setHydrated] = createSignal(false);
  createEffect(() => {
    if (hydrated()) return;
    const s = settings();
    if (s) {
      if (s.conversationThreading != null) {
        setThreading(s.conversationThreading);
      }
      if (s.undoSendDelaySec != null) {
        setUndoDelay(s.undoSendDelaySec);
      }
      if (s.blockRemoteImages != null) {
        setBlockImages(s.blockRemoteImages);
      }
      if (s.trustedImageSenders != null) {
        setTrustedSenders(s.trustedImageSenders);
      }
      setHydrated(true);
    }
  });

  const toggleThreading = async (enabled: boolean) => {
    setThreading(enabled);
    await updateSettings({ conversationThreading: enabled });
    await loadRows();
  };

  const updateUndoDelay = async (sec: number) => {
    setUndoDelay(sec);
    await updateSettings({ undoSendDelaySec: sec });
  };

  const toggleBlockImages = async (enabled: boolean) => {
    setBlockImages(enabled);
    await updateSettings({ blockRemoteImages: enabled });
  };

  const handleAddTrustedSender = async (e: Event) => {
    e.preventDefault();
    const email = newSenderInput().trim().toLowerCase();
    if (!email || !email.includes("@")) return;
    if (!trustedSenders().includes(email)) {
      const updated = [...trustedSenders(), email];
      setTrustedSenders(updated);
      setNewSenderInput("");
      await updateSettings({ trustedImageSenders: updated });
    }
  };

  const handleRemoveTrustedSender = async (email: string) => {
    const updated = trustedSenders().filter((s) => s !== email);
    setTrustedSenders(updated);
    await updateSettings({ trustedImageSenders: updated });
  };

  const handleClearAllTrustedSenders = async () => {
    setTrustedSenders([]);
    await updateSettings({ trustedImageSenders: [] });
  };

  return (
    <div class="general-settings">
      <div class="settings-row general-option">
        <label class="general-option__label">
          <input
            type="checkbox"
            checked={threading()}
            onChange={(e) => void toggleThreading(e.currentTarget.checked)}
          />
          <span class="general-option__text">
            <span class="general-option__title">Conversation threading</span>
            <span class="general-option__desc">
              Group replies into threads with collapsed history and bulk actions
            </span>
          </span>
        </label>
      </div>

      <div class="settings-row general-option general-option--undo">
        <div class="general-option__text">
          <span class="general-option__title">
            Undo Send cancellation period
          </span>
          <span class="general-option__desc">
            Provides a delay before submitting messages to SMTP, allowing you to
            recall and edit
          </span>
        </div>
        <select
          class="settings-select"
          value={undoDelay()}
          onChange={(e) => void updateUndoDelay(Number(e.currentTarget.value))}
        >
          <option value="0">Off (Send immediately)</option>
          <option value="5">5 seconds</option>
          <option value="10">10 seconds (Recommended)</option>
          <option value="15">15 seconds</option>
          <option value="20">20 seconds</option>
          <option value="30">30 seconds</option>
        </select>
      </div>

      {/* Remote Content & Privacy (Roadmap 3.7) */}
      <div class="privacy-settings-group">
        <h3 class="privacy-settings-group__title">Remote Content & Privacy</h3>
        <p class="privacy-settings-group__desc">
          Protect your IP address and online privacy by controlling when
          external images and web beacons load.
        </p>

        <div class="settings-row general-option">
          <label class="general-option__label">
            <input
              type="checkbox"
              checked={blockImages()}
              onChange={(e) => void toggleBlockImages(e.currentTarget.checked)}
            />
            <span class="general-option__text">
              <span class="general-option__title">
                Block remote images by default
              </span>
              <span class="general-option__desc">
                Requires manual confirmation or sender allowlist to load remote
                graphics and tracking pixels
              </span>
            </span>
          </label>
        </div>

        <div
          class="settings-row"
          style={{ "flex-direction": "column", "align-items": "stretch" }}
        >
          <div
            style={{
              display: "flex",
              "justify-content": "space-between",
              "align-items": "center",
              "margin-bottom": "var(--space-8px)",
            }}
          >
            <span class="general-option__title">
              Trusted Senders ({trustedSenders().length})
            </span>
            <Show when={trustedSenders().length > 0}>
              <button
                type="button"
                class="btn btn--secondary btn--sm"
                onClick={() => void handleClearAllTrustedSenders()}
              >
                Clear all
              </button>
            </Show>
          </div>

          <Show when={trustedSenders().length > 0}>
            <table class="trusted-senders-table">
              <thead>
                <tr>
                  <th>Sender Email Address</th>
                  <th style={{ width: "60px", "text-align": "right" }}>
                    Action
                  </th>
                </tr>
              </thead>
              <tbody>
                <For each={trustedSenders()}>
                  {(sender) => (
                    <tr>
                      <td>{sender}</td>
                      <td style={{ "text-align": "right" }}>
                        <button
                          type="button"
                          class="btn btn--secondary btn--sm"
                          title={`Remove ${sender} from trusted senders`}
                          onClick={() => void handleRemoveTrustedSender(sender)}
                        >
                          Remove
                        </button>
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </Show>

          <form
            class="trusted-senders-add-form"
            onSubmit={(e) => void handleAddTrustedSender(e)}
          >
            <input
              type="email"
              class="trusted-senders-input"
              placeholder="trustedsender@domain.com"
              value={newSenderInput()}
              onInput={(e) => setNewSenderInput(e.currentTarget.value)}
            />
            <button
              type="submit"
              class="btn btn--secondary btn--sm"
              disabled={!newSenderInput().trim()}
            >
              + Add sender
            </button>
          </form>
        </div>
      </div>

      {/* P1.5 launch at login */}
      <div class="privacy-settings-group">
        <h3 class="privacy-settings-group__title">Startup</h3>
        <div class="settings-row general-option">
          <label class="general-option__label">
            <input
              type="checkbox"
              checked={launchAtLogin()}
              onChange={(e) => void toggleLaunchAtLogin(e.currentTarget.checked)}
            />
            <span class="general-option__text">
              <span class="general-option__title">Launch Quill at login</span>
              <span class="general-option__desc">
                Start Quill when you sign in to your computer, so sync and
                reminders run in the background (the system tray keeps it
                accessible).
              </span>
            </span>
          </label>
        </div>
      </div>

      {/* P1.3 search index — freshness + a safe, cancellable rebuild. */}
      <div class="privacy-settings-group">
        <h3 class="privacy-settings-group__title">Search index</h3>
        <p class="privacy-settings-group__desc">
          Message search uses a live local index that stays up to date as mail
          arrives. A rebuild only repairs it.
        </p>
        <div class="settings-row general-option">
          <span class="general-option__text">
            <span class="general-option__title">Index freshness</span>
            <span class="general-option__desc">
              {indexStatus()
                ? indexStatus()!.state === "fresh"
                  ? "Up to date"
                  : indexStatus()!.state === "rebuilding"
                    ? `Rebuilding… ${indexStatus()!.indexed} of ${indexStatus()!.total}`
                    : `${indexStatus()!.indexed} of ${indexStatus()!.total} indexed`
                : "Checking…"}
            </span>
          </span>
          <Show when={indexStatus()?.state === "rebuilding"}>
            <button
              type="button"
              class="btn btn--secondary btn--sm"
              onClick={() => void cancelSearchRebuild()}
            >
              Cancel
            </button>
          </Show>
          <Show when={indexStatus()?.state !== "rebuilding"}>
            <button
              type="button"
              class="btn btn--secondary btn--sm"
              onClick={() => void rebuildSearchIndexProgress()}
            >
              Rebuild
            </button>
          </Show>
        </div>
      </div>
    </div>
  );
}
