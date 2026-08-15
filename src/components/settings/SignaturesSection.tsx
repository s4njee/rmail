import { createEffect, createSignal, For, Show } from "solid-js";
import type { AccountIdentity } from "../../lib/ipc/AccountIdentity";
import type { AccountSignature } from "../../lib/ipc/AccountSignature";
import { useAccounts } from "../../lib/mail";
import { updateSettings, useSettings } from "../../lib/settings";
import "../Settings.css";

export function SignaturesSection() {
  const settings = useSettings();
  const accounts = useAccounts();

  const [selectedIdentityId, setSelectedIdentityId] = createSignal<
    string | null
  >(null);
  const [editingName, setEditingName] = createSignal("");
  const [editingEmail, setEditingEmail] = createSignal("");
  const [editingReplyTo, setEditingReplyTo] = createSignal("");
  const [isDefault, setIsDefault] = createSignal(false);

  const [plainText, setPlainText] = createSignal("");
  const [htmlText, setHtmlText] = createSignal("");
  const [includeInNew, setIncludeInNew] = createSignal(true);
  const [includeInReplies, setIncludeInReplies] = createSignal(true);
  const [replyPlacement, setReplyPlacement] =
    createSignal<string>("above_quote");

  const [addingIdentity, setAddingIdentity] = createSignal(false);
  const [newAccountId, setNewAccountId] = createSignal<number>(1);
  const [newName, setNewName] = createSignal("");
  const [newEmail, setNewEmail] = createSignal("");
  const [newReplyTo, setNewReplyTo] = createSignal("");

  const currentIdentities = (): AccountIdentity[] => {
    const s = settings();
    const accts = accounts();
    const list = s?.identities || [];
    if (list.length > 0) return list;

    // Fallback default identities from accounts
    return accts.map((a) => ({
      id: `default_${a.id}`,
      accountId: a.id,
      name: "",
      email: a.address,
      replyTo: null,
      signature: {
        plainText: `-- \n${a.address}`,
        html: `<br>-- <br><b>${a.address}</b>`,
        includeInNewMail: true,
        includeInReplies: true,
        replyPlacement: "above_quote",
      },
      isDefault: true,
    }));
  };

  createEffect(() => {
    const idents = currentIdentities();
    if (!selectedIdentityId() && idents.length > 0) {
      setSelectedIdentityId(idents[0].id);
    }
  });

  createEffect(() => {
    const id = selectedIdentityId();
    const ident = currentIdentities().find((i) => i.id === id);
    if (ident) {
      setEditingName(ident.name || "");
      setEditingEmail(ident.email || "");
      setEditingReplyTo(ident.replyTo || "");
      setIsDefault(ident.isDefault);

      const sig = ident.signature;
      setPlainText(sig?.plainText || "");
      setHtmlText(sig?.html || "");
      setIncludeInNew(sig?.includeInNewMail ?? true);
      setIncludeInReplies(sig?.includeInReplies ?? true);
      setReplyPlacement(sig?.replyPlacement || "above_quote");
    }
  });

  const saveCurrentIdentity = async () => {
    const id = selectedIdentityId();
    if (!id) return;

    const list = [...currentIdentities()];
    const idx = list.findIndex((i) => i.id === id);
    if (idx === -1) return;

    const updatedSig: AccountSignature = {
      plainText: plainText(),
      html: htmlText() ? htmlText() : null,
      includeInNewMail: includeInNew(),
      includeInReplies: includeInReplies(),
      replyPlacement: replyPlacement(),
    };

    const targetAcctId = list[idx].accountId;
    const defaultVal = isDefault();

    const updatedList: AccountIdentity[] = list.map((item, i) => {
      if (i === idx) {
        return {
          ...item,
          name: editingName(),
          email: editingEmail(),
          replyTo: editingReplyTo().trim() ? editingReplyTo().trim() : null,
          signature: updatedSig,
          isDefault: defaultVal,
        };
      }
      if (defaultVal && item.accountId === targetAcctId) {
        return { ...item, isDefault: false };
      }
      return item;
    });

    await updateSettings({ identities: updatedList });
  };

  const handleAddIdentity = async () => {
    if (!newEmail().trim()) return;

    const newId = `id_${Date.now()}`;
    const newSig: AccountSignature = {
      plainText: `-- \n${newName().trim() || newEmail().trim()}`,
      html: null,
      includeInNewMail: true,
      includeInReplies: true,
      replyPlacement: "above_quote",
    };

    const newIdent: AccountIdentity = {
      id: newId,
      accountId: newAccountId(),
      name: newName().trim(),
      email: newEmail().trim(),
      replyTo: newReplyTo().trim() ? newReplyTo().trim() : null,
      signature: newSig,
      isDefault: false,
    };

    const list: AccountIdentity[] = [...currentIdentities(), newIdent];
    await updateSettings({ identities: list });

    setAddingIdentity(false);
    setNewName("");
    setNewEmail("");
    setNewReplyTo("");
    setSelectedIdentityId(newId);
  };

  const handleDeleteIdentity = async (id: string) => {
    const list = currentIdentities().filter((i) => i.id !== id);
    await updateSettings({ identities: list });
    if (selectedIdentityId() === id && list.length > 0) {
      setSelectedIdentityId(list[0].id);
    }
  };

  const selectedIdent = () =>
    currentIdentities().find((i) => i.id === selectedIdentityId());
  const selectedAccount = () =>
    accounts().find((a) => a.id === selectedIdent()?.accountId);

  return (
    <div class="settings-signatures">
      <div class="signatures-layout">
        <div class="signatures-sidebar">
          <div class="signatures-sidebar__header">
            <span class="signatures-sidebar__title">Identities & Aliases</span>
            <button
              type="button"
              class="btn btn--secondary btn--sm"
              onClick={() => {
                setNewAccountId(accounts()[0]?.id ?? 1);
                setAddingIdentity(true);
              }}
            >
              + Add
            </button>
          </div>

          <div class="signatures-list">
            <For each={currentIdentities()}>
              {(ident) => (
                <button
                  type="button"
                  class="signatures-list__item"
                  classList={{ "is-active": selectedIdentityId() === ident.id }}
                  onClick={() => setSelectedIdentityId(ident.id)}
                >
                  <span class="signatures-list__name">
                    {ident.name ? ident.name : ident.email}
                  </span>
                  <span class="signatures-list__email">{ident.email}</span>
                  <Show when={ident.isDefault}>
                    <span class="signatures-list__badge">Default</span>
                  </Show>
                </button>
              )}
            </For>
          </div>
        </div>

        <div class="signatures-detail">
          <Show when={addingIdentity()}>
            <div class="signatures-add-card">
              <h3 class="signatures-add-title">New Send-As Identity / Alias</h3>
              <div class="settings-field">
                <label>Account</label>
                <select
                  class="settings-select"
                  value={newAccountId()}
                  onChange={(e) =>
                    setNewAccountId(Number(e.currentTarget.value))
                  }
                >
                  <For each={accounts()}>
                    {(a) => <option value={a.id}>{a.address}</option>}
                  </For>
                </select>
              </div>
              <div class="settings-field">
                <label>Display Name</label>
                <input
                  type="text"
                  class="settings-input"
                  placeholder="e.g. Sanjee Support"
                  value={newName()}
                  onInput={(e) => setNewName(e.currentTarget.value)}
                />
              </div>
              <div class="settings-field">
                <label>Email Address</label>
                <input
                  type="email"
                  class="settings-input"
                  placeholder="support@quill.app"
                  value={newEmail()}
                  onInput={(e) => setNewEmail(e.currentTarget.value)}
                />
              </div>
              <div class="settings-field">
                <label>Reply-To Address (optional)</label>
                <input
                  type="email"
                  class="settings-input"
                  placeholder="replies@quill.app"
                  value={newReplyTo()}
                  onInput={(e) => setNewReplyTo(e.currentTarget.value)}
                />
              </div>
              <div class="signatures-add-actions">
                <button
                  type="button"
                  class="btn btn--secondary btn--sm"
                  onClick={() => setAddingIdentity(false)}
                >
                  Cancel
                </button>
                <button
                  type="button"
                  class="btn btn--primary btn--sm"
                  onClick={() => void handleAddIdentity()}
                  disabled={!newEmail().trim()}
                >
                  Create Identity
                </button>
              </div>
            </div>
          </Show>

          <Show when={selectedIdent() && !addingIdentity()}>
            <div class="signatures-editor">
              <div class="signatures-editor__header">
                <div>
                  <h3 class="signatures-editor__title">
                    {selectedIdent()!.name
                      ? selectedIdent()!.name
                      : selectedIdent()!.email}
                  </h3>
                  <span class="signatures-editor__subtitle">
                    Account: {selectedAccount()?.address || "Default"}
                  </span>
                </div>
                <Show when={currentIdentities().length > 1}>
                  <button
                    type="button"
                    class="btn btn--secondary btn--sm signatures-delete-btn"
                    onClick={() =>
                      void handleDeleteIdentity(selectedIdentityId()!)
                    }
                  >
                    Delete Alias
                  </button>
                </Show>
              </div>

              <div class="signatures-form-grid">
                <div class="settings-field">
                  <label>Sender Name</label>
                  <input
                    type="text"
                    class="settings-input"
                    value={editingName()}
                    onInput={(e) => {
                      setEditingName(e.currentTarget.value);
                      void saveCurrentIdentity();
                    }}
                  />
                </div>
                <div class="settings-field">
                  <label>From Address</label>
                  <input
                    type="email"
                    class="settings-input"
                    value={editingEmail()}
                    onInput={(e) => {
                      setEditingEmail(e.currentTarget.value);
                      void saveCurrentIdentity();
                    }}
                  />
                </div>
              </div>

              <div class="settings-field">
                <label>Reply-To Address (optional)</label>
                <input
                  type="email"
                  class="settings-input"
                  placeholder="Same as from address"
                  value={editingReplyTo()}
                  onInput={(e) => {
                    setEditingReplyTo(e.currentTarget.value);
                    void saveCurrentIdentity();
                  }}
                />
              </div>

              <div class="signatures-checkbox-row">
                <label class="signatures-checkbox-label">
                  <input
                    type="checkbox"
                    checked={isDefault()}
                    onChange={(e) => {
                      setIsDefault(e.currentTarget.checked);
                      void saveCurrentIdentity();
                    }}
                  />
                  <span>Default identity for this account</span>
                </label>
              </div>

              <hr class="signatures-divider" />

              <div class="signatures-section-title">Signature</div>

              <div class="settings-field">
                <label>Plain Text Signature</label>
                <textarea
                  class="settings-textarea"
                  rows={4}
                  placeholder="-- &#10;Your Name&#10;Company / Title"
                  value={plainText()}
                  onInput={(e) => {
                    setPlainText(e.currentTarget.value);
                    void saveCurrentIdentity();
                  }}
                />
              </div>

              <div class="settings-field">
                <label>HTML Signature (optional formatted version)</label>
                <textarea
                  class="settings-textarea"
                  rows={4}
                  placeholder="<br>-- <br><b>Your Name</b><br>Company / Title"
                  value={htmlText()}
                  onInput={(e) => {
                    setHtmlText(e.currentTarget.value);
                    void saveCurrentIdentity();
                  }}
                />
              </div>

              <div class="signatures-options-grid">
                <label class="signatures-checkbox-label">
                  <input
                    type="checkbox"
                    checked={includeInNew()}
                    onChange={(e) => {
                      setIncludeInNew(e.currentTarget.checked);
                      void saveCurrentIdentity();
                    }}
                  />
                  <span>Include in new messages</span>
                </label>

                <label class="signatures-checkbox-label">
                  <input
                    type="checkbox"
                    checked={includeInReplies()}
                    onChange={(e) => {
                      setIncludeInReplies(e.currentTarget.checked);
                      void saveCurrentIdentity();
                    }}
                  />
                  <span>Include in replies and forwards</span>
                </label>
              </div>

              <Show when={includeInReplies()}>
                <div class="settings-field">
                  <label>Placement in replies</label>
                  <select
                    class="settings-select"
                    value={replyPlacement()}
                    onChange={(e) => {
                      setReplyPlacement(e.currentTarget.value);
                      void saveCurrentIdentity();
                    }}
                  >
                    <option value="above_quote">
                      Insert before quoted text (above quote)
                    </option>
                    <option value="bottom">
                      Insert at the bottom of the email
                    </option>
                  </select>
                </div>
              </Show>
            </div>
          </Show>
        </div>
      </div>
    </div>
  );
}
