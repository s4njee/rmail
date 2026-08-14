import { createSignal, For, Show } from "solid-js";
import { formatBytes } from "../../lib/format";
import type { Account } from "../../lib/ipc/Account";
import { refreshMail, useAccounts } from "../../lib/mail";
import { removeAccount } from "../../lib/tauri";
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

  const confirmRemove = async () => {
    const account = removing();
    if (account) {
      await removeAccount(account.id);
      await refreshMail();
    }
    setRemoving(null);
  };

  return (
    <div class="settings-accounts">
      <For each={accounts()}>
        {(account) => {
          const detail =
            account.folder_count > 0
              ? `${account.protocol} · ${account.sync_mode} · ${account.folder_count} folders`
              : `${account.protocol} · ${account.sync_mode}`;
          return (
            <div class="settings-row" role="listitem">
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
                    <span class="settings-row__auth"> · auth failed</span>
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
                class="settings-row__remove"
                onClick={() => setRemoving(account)}
              >
                Remove
              </button>
            </div>
          );
        }}
      </For>

      {/* Remove confirm — names exactly what will be deleted (10.4). */}
      <Show when={removing()}>
        <div
          class="account-confirm"
          role="alertdialog"
          aria-label="Remove account"
        >
          <span class="account-confirm__text">
            Delete local mail and calendar data for {removing()?.address}? This
            cannot be undone.
          </span>
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
          >
            Delete
          </button>
        </div>
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
