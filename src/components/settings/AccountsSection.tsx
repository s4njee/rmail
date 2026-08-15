import { createSignal, For, onMount, Show } from "solid-js";
import { setCalendarSyncing } from "../../lib/calendar";
import { openContextMenu } from "../../lib/context-menu";
import { formatBytes } from "../../lib/format";
import type { Account } from "../../lib/ipc/Account";
import type { AccountRemovalInfo } from "../../lib/ipc/AccountRemovalInfo";
import type { QueuedAction } from "../../lib/ipc/QueuedAction";
import { openAccountEdit, refreshMail, useAccounts } from "../../lib/mail";
import { actionLabel, refreshQueued, useQueued } from "../../lib/queue";
import {
  accountRemovalInfo,
  discardQueuedAction,
  removeAccount,
  retryQueuedAction,
  syncCalendar,
} from "../../lib/tauri";
import { useTheme } from "../../lib/theme";
import { Modal } from "../Modal";
import { AddAccountForm } from "./AddAccountForm";
import "../Settings.css";

// Settings → Accounts (Epic 10.2): one row per real account — protocol · sync
// mode · folders and the real on-disk size — plus add (10.4) and remove, with
// the auth-failure state shown inline on the row, never as a modal.
export function AccountsSection() {
  const accounts = useAccounts();
  const theme = useTheme();
  const [adding, setAdding] = createSignal(false);
  const [removing, setRemoving] = createSignal<Account | null>(null);
  const [removalInfo, setRemovalInfo] = createSignal<AccountRemovalInfo | null>(
    null,
  );
  const [removalConfirmText, setRemovalConfirmText] = createSignal("");
  const [syncingId, setSyncingId] = createSignal<number | null>(null);
  const [syncMsg, setSyncMsg] = createSignal<string | null>(null);
  const queued = useQueued();
  const [confirmDiscard, setConfirmDiscard] = createSignal<QueuedAction | null>(
    null,
  );

  onMount(() => void refreshQueued());

  const retry = async (id: number) => {
    await retryQueuedAction(id);
    await refreshQueued();
  };
  const discard = async (a: QueuedAction) => {
    await discardQueuedAction(a.id);
    setConfirmDiscard(null);
    await refreshQueued();
  };

  // For a queued Send, show its subject/recipients instead of raw JSON.
  const sendDisplay = (
    a: QueuedAction,
  ): { subject: string; to: string[] } | null => {
    if (!a.payload) return null;
    try {
      const parsed = JSON.parse(a.payload);
      return { subject: parsed.subject ?? "", to: parsed.to ?? [] };
    } catch {
      return null;
    }
  };

  const startRemove = (account: Account) => {
    setRemoving(account);
    setRemovalConfirmText("");
    setRemovalInfo(null);
    void accountRemovalInfo(account.id)
      .then(setRemovalInfo)
      .catch(() => {});
  };

  const confirmRemove = async () => {
    const account = removing();
    if (account) {
      await removeAccount(account.id);
      await refreshMail();
    }
    setRemoving(null);
    setRemovalInfo(null);
  };

  const handleSyncCalendar = async (accountId: number) => {
    setSyncingId(accountId);
    setSyncMsg(null);
    setCalendarSyncing(true);
    try {
      const cols = await syncCalendar(accountId);
      setSyncMsg(`Synced ${cols.length} calendar(s)`);
    } catch (e) {
      setSyncMsg(`Calendar sync failed: ${e}`);
    } finally {
      setSyncingId(null);
      setCalendarSyncing(false);
    }
  };

  // Right-click on an account row: Edit (opens the shared dialog) or Delete
  // (reuses the existing inline confirm).
  const openAccountMenu = (account: Account, event: MouseEvent) => {
    event.preventDefault();
    openContextMenu(
      [
        { label: "Edit account…", onSelect: () => openAccountEdit(account) },
        {
          label: "Delete account…",
          danger: true,
          onSelect: () => startRemove(account),
        },
      ],
      event.clientX,
      event.clientY,
    );
  };

  return (
    <div class="settings-accounts">
      <Show when={syncMsg()}>
        <div class="settings-sync-msg">{syncMsg()}</div>
      </Show>
      <For each={accounts()}>
        {(account) => {
          const detail =
            account.folder_count > 0
              ? `${account.protocol} · ${account.sync_mode} · ${account.folder_count} folders`
              : `${account.protocol} · ${account.sync_mode}`;
          return (
            <div
              class="settings-row"
              role="listitem"
              onContextMenu={(e) => openAccountMenu(account, e)}
            >
              <span
                class="settings-row__dot"
                style={{ background: account.color }}
                aria-hidden="true"
              />
              <span class="settings-row__text">
                <span class="settings-row__address">{account.address}</span>
                <span class="settings-row__detail">
                  {detail}
                  <Show when={!account.connected}>
                    <span class="settings-row__auth">
                      {" "}
                      · {account.last_error || "not connected"}
                    </span>
                  </Show>
                </span>
              </span>
              <span
                class="settings-row__size tabular"
                classList={{ mono: theme() === "hairline" }}
              >
                {formatBytes(account.local_bytes)}
              </span>
              <button
                type="button"
                class="settings-row__action"
                onClick={() => void handleSyncCalendar(account.id)}
                disabled={syncingId() === account.id}
              >
                {syncingId() === account.id ? "Syncing…" : "Sync Cal"}
              </button>
              <button
                type="button"
                class="settings-row__action"
                onClick={() => openAccountEdit(account)}
              >
                Edit
              </button>
              <button
                type="button"
                class="settings-row__remove"
                onClick={() => startRemove(account)}
              >
                Remove
              </button>
            </div>
          );
        }}
      </For>

      {/* Remove confirm (P0.2) — names exactly what is deleted, that the
          server is untouched, and requires typing the address when unsent
          work would be discarded. */}
      <Show when={removing()}>
        {(account) => {
          const info = removalInfo();
          const hasWork =
            (info?.queuedActions ?? 0) > 0 || (info?.drafts ?? 0) > 0;
          return (
            <div
              class="account-confirm"
              role="alertdialog"
              aria-label="Remove account"
            >
              <span class="account-confirm__title">
                Remove {account().address}?
              </span>
              <span class="account-confirm__text">
                This deletes only Quill's local copy on this device — your mail
                and calendar stay on the server untouched. Removed: the local
                cache ({formatBytes(info?.localBytes ?? 0)}), cached
                attachments, and the saved password
                {account().protocol.includes("OAuth")
                  ? " and sign-in session"
                  : ""}{" "}
                for this account.
              </span>
              <Show when={hasWork}>
                <span class="account-confirm__text account-confirm__text--warn">
                  You have {(info?.queuedActions ?? 0) + (info?.drafts ?? 0)}{" "}
                  unsent or queued item(s) that will be discarded. Type the
                  address to confirm.
                </span>
                <input
                  type="text"
                  class="account-confirm__input"
                  value={removalConfirmText()}
                  onInput={(e) => setRemovalConfirmText(e.currentTarget.value)}
                  placeholder={account().address}
                  autocomplete="off"
                  spellcheck={false}
                />
              </Show>
              <div class="account-confirm__actions">
                <button
                  type="button"
                  class="btn btn--secondary"
                  onClick={() => setRemoving(null)}
                >
                  Cancel
                </button>
                <button
                  type="button"
                  class="btn btn--primary"
                  onClick={() => void confirmRemove()}
                  disabled={
                    hasWork && removalConfirmText().trim() !== account().address
                  }
                >
                  Delete local copy
                </button>
              </div>
            </div>
          );
        }}
      </Show>

      <div class="settings-footer">
        <button
          type="button"
          class="add-account-btn"
          onClick={() => setAdding(true)}
        >
          Add account
        </button>
        <span class="settings-footer__note">
          Mail is stored on this device only.
        </span>
      </div>

      {/* P0.3 Sync & queue — queued/retrying/stuck offline actions, recoverable. */}
      <Show when={queued().length > 0}>
        <h3 class="settings-note">Sync & queue</h3>
        <p class="settings-note">
          Changes made while offline wait here until they can sync. A stuck
          action shows its last error; Retry resets it, Remove discards it.
        </p>
        <div class="queue-list">
          <For each={queued()}>
            {(a) => {
              const { label, state } = actionLabel(a);
              const send = a.action_type === "send" ? sendDisplay(a) : null;
              return (
                <div class="queue-row" data-state={state}>
                  <div class="queue-row__main">
                    <span class="queue-row__label">
                      {send ? send.subject || label : label}
                    </span>
                    <span class="queue-row__meta">
                      {send && send.to.length > 0
                        ? `to ${send.to.join(", ")}`
                        : a.folder}
                      {a.retries > 0 ? ` · retried ${a.retries}×` : ""}
                      <Show when={state !== "pending"}>
                        <span class="queue-row__state">({state})</span>
                      </Show>
                    </span>
                    <Show when={a.last_error}>
                      <span class="queue-row__error">{a.last_error}</span>
                    </Show>
                  </div>
                  <div class="queue-row__actions">
                    <button
                      type="button"
                      class="btn btn--secondary btn--sm"
                      onClick={() => void retry(a.id)}
                    >
                      Retry
                    </button>
                    <button
                      type="button"
                      class="btn btn--secondary btn--sm"
                      onClick={() =>
                        a.action_type === "send"
                          ? setConfirmDiscard(a)
                          : void discard(a)
                      }
                    >
                      Remove
                    </button>
                  </div>
                </div>
              );
            }}
          </For>
        </div>

        {/* Discarding an unsent message warns explicitly. */}
        <Show when={confirmDiscard()}>
          {(a) => (
            <div
              class="account-confirm"
              role="alertdialog"
              aria-label="Discard queued action"
            >
              <span class="account-confirm__text">
                Discard the queued action
                {sendDisplay(a())?.subject
                  ? ` "${sendDisplay(a())?.subject}"`
                  : ""}
                ? An unsent message will not be delivered.
              </span>
              <div class="account-confirm__actions">
                <button
                  type="button"
                  class="btn btn--secondary"
                  onClick={() => setConfirmDiscard(null)}
                >
                  Cancel
                </button>
                <button
                  type="button"
                  class="btn btn--primary"
                  onClick={() => void discard(a())}
                >
                  Discard
                </button>
              </div>
            </div>
          )}
        </Show>
      </Show>

      <Show when={adding()}>
        <Modal title="Add account" onClose={() => setAdding(false)}>
          <AddAccountForm
            onDone={() => setAdding(false)}
            onCancel={() => setAdding(false)}
          />
        </Modal>
      </Show>
    </div>
  );
}
