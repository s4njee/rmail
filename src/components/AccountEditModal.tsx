import { createSignal, Show } from "solid-js";
import { closeAccountEdit, refreshMail, useEditingAccount } from "../lib/mail";
import { getOAuthInit, reauthorizeAccount, waitOAuthCode } from "../lib/tauri";
import { Modal } from "./Modal";
import { AddAccountForm } from "./settings/AddAccountForm";
import "./Settings.css";

// Shared account-edit dialog, rendered once at the App root so it can open
// from Settings or a right-click menu on an account row. State lives in
// lib/mail.ts (`editingAccount`). OAuth accounts get a "Reconnect sign-in"
// action (P0.2): a revoked/expired credential is refreshed by re-running the
// OAuth flow and updating the stored tokens — local data is untouched.
export function AccountEditModal() {
  const editing = useEditingAccount();
  const [reconnecting, setReconnecting] = createSignal(false);
  const [error, setError] = createSignal("");

  const reconnect = async () => {
    const acc = editing();
    if (!acc) return;
    setReconnecting(true);
    setError("");
    try {
      const provider: "google" | "microsoft365" = acc.protocol.includes(
        "Microsoft",
      )
        ? "microsoft365"
        : "google";
      const init = await getOAuthInit(provider);
      if (typeof window !== "undefined" && window.open) {
        window.open(init.auth_url, "_blank");
      }
      const result = await waitOAuthCode(init.redirect_uri, init.state);
      if (!result.ok || !result.code) {
        setError(
          result.error ||
            "Timed out waiting for the browser sign-in — try again.",
        );
        return;
      }
      await reauthorizeAccount(
        acc.id,
        provider,
        result.code,
        init.code_verifier,
        init.redirect_uri,
        init.client_id,
      );
      await refreshMail();
      closeAccountEdit();
    } catch (e) {
      setError(String(e));
    } finally {
      setReconnecting(false);
    }
  };

  return (
    <Show when={editing()}>
      {(account) => (
        <Modal
          title={`Edit account — ${account().address}`}
          onClose={closeAccountEdit}
        >
          <Show when={account().protocol.includes("OAuth")}>
            <div class="account-reauth">
              <span class="account-reauth__text">
                Sign-in expired or revoked? Reconnect without touching the mail
                already stored on this device.
              </span>
              <button
                type="button"
                class="btn btn--secondary btn--sm"
                onClick={() => void reconnect()}
                disabled={reconnecting()}
              >
                {reconnecting() ? "Reconnecting…" : "Reconnect sign-in"}
              </button>
            </div>
            <Show when={error()}>
              <div class="account-reauth__error" role="alert">
                {error()}
              </div>
            </Show>
          </Show>
          <AddAccountForm
            account={account()}
            onDone={closeAccountEdit}
            onCancel={closeAccountEdit}
          />
        </Modal>
      )}
    </Show>
  );
}
