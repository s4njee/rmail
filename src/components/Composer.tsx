import { createEffect, createSignal, For, Show } from "solid-js";
import {
  addComposerAttachment,
  closeComposer,
  discardComposer,
  persistDraft,
  removeComposerAttachment,
  scheduleComposer,
  sendComposer,
  switchComposerIdentity,
  updateDraft,
  useComposer,
} from "../lib/compose";
import { formatBytes } from "../lib/format";
import type { AccountIdentity } from "../lib/ipc/AccountIdentity";
import type { ContactSuggestion } from "../lib/ipc/ContactSuggestion";
import { useAccounts } from "../lib/mail";
import {
  getSettings,
  hideRecipient,
  recentRecipients,
  suggestRecipients,
} from "../lib/tauri";
import { Modal } from "./Modal";
import { SendLaterMenu } from "./SendLaterMenu";
import "./Composer.css";

const EMAIL_REGEX = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
const MAX_ATTACHMENT_SIZE = 25 * 1024 * 1024; // 25 MB warning threshold

function isValidEmail(email: string): boolean {
  const trimmed = email.trim();
  if (EMAIL_REGEX.test(trimmed)) return true;
  const match = /<([^>]+)>/.exec(trimmed);
  if (match && EMAIL_REGEX.test(match[1].trim())) return true;
  return false;
}

const fileToBase64 = (file: File): Promise<string> => {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result as string;
      const base64 = result.split(",")[1] || "";
      resolve(base64);
    };
    reader.onerror = reject;
    reader.readAsDataURL(file);
  });
};

interface AddressInputProps {
  label: string;
  values: string[];
  onChange: (values: string[]) => void;
  placeholder?: string;
}

/** Split a pasted/typed recipient list on unquoted separators, honoring `"…"`
 * quoted display names and `<addr>` angle forms so `"Doe, John" <john@x.com>`
 * stays one recipient instead of being split on the comma inside the name. */
function splitRecipients(raw: string): string[] {
  const out: string[] = [];
  let current = "";
  let inQuote = false;
  let inAngle = false;
  for (const ch of raw) {
    if (ch === '"') {
      inQuote = !inQuote;
      current += ch;
    } else if (ch === "<" && !inQuote) {
      inAngle = true;
      current += ch;
    } else if (ch === ">" && inAngle) {
      inAngle = false;
      current += ch;
    } else if (/[,;\n\r]/.test(ch) && !inQuote && !inAngle) {
      const trimmed = current.trim();
      if (trimmed) out.push(trimmed);
      current = "";
    } else {
      current += ch;
    }
  }
  const trimmed = current.trim();
  if (trimmed) out.push(trimmed);
  return out;
}

function AddressInput(props: AddressInputProps) {
  const [inputVal, setInputVal] = createSignal("");
  // P1.2 autocomplete: suggestions from mail history (or recent when empty),
  // with a highlighted index for arrow-key navigation.
  const [suggestions, setSuggestions] = createSignal<ContactSuggestion[]>([]);
  const [open, setOpen] = createSignal(false);
  const [selIdx, setSelIdx] = createSignal(-1);
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  const addAddresses = (raw: string) => {
    const parts = splitRecipients(raw)
      .map((s) => s.trim())
      .filter(Boolean);
    if (parts.length === 0) return;
    const next = [...props.values];
    for (const part of parts) {
      if (!next.includes(part)) {
        next.push(part);
      }
    }
    props.onChange(next);
    setInputVal("");
    setOpen(false);
    setSelIdx(-1);
  };

  const insertSuggestion = (s: ContactSuggestion) => {
    const display =
      s.name && s.name.trim() ? `${s.name} <${s.address}>` : s.address;
    addAddresses(display);
  };

  const dismissSuggestion = async (s: ContactSuggestion) => {
    await hideRecipient(s.address);
    setSuggestions((prev) => prev.filter((x) => x.address !== s.address));
  };

  const loadSuggestions = (raw: string) => {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(async () => {
      const q = raw.trim();
      const list = q ? await suggestRecipients(q) : await recentRecipients();
      setSuggestions(list);
      setSelIdx(-1);
      setOpen(list.length > 0);
    }, 120);
  };

  const removeAddress = (idx: number) => {
    const next = props.values.filter((_, i) => i !== idx);
    props.onChange(next);
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    const list = suggestions();
    if (e.key === "ArrowDown" && open() && list.length > 0) {
      e.preventDefault();
      setSelIdx((i) => (i + 1) % list.length);
    } else if (e.key === "ArrowUp" && open() && list.length > 0) {
      e.preventDefault();
      setSelIdx((i) => (i <= 0 ? list.length - 1 : i - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (open() && selIdx() >= 0 && list[selIdx()]) {
        insertSuggestion(list[selIdx()]);
      } else {
        addAddresses(inputVal());
      }
    } else if (e.key === "Tab" && open() && selIdx() >= 0 && list[selIdx()]) {
      e.preventDefault();
      insertSuggestion(list[selIdx()]);
    } else if (e.key === "Escape") {
      setOpen(false);
    } else if (e.key === "," || e.key === ";") {
      e.preventDefault();
      addAddresses(inputVal());
    } else if (
      e.key === "Backspace" &&
      inputVal() === "" &&
      props.values.length > 0
    ) {
      removeAddress(props.values.length - 1);
    }
  };

  const handlePaste = (e: ClipboardEvent) => {
    const pasted = e.clipboardData?.getData("text");
    if (
      pasted &&
      (pasted.includes(",") || pasted.includes(";") || pasted.includes("\n"))
    ) {
      e.preventDefault();
      addAddresses(pasted);
    }
  };

  return (
    <div class="composer__field composer__field--address">
      <span class="composer__label">{props.label}</span>
      <div class="composer__pills-container">
        <For each={props.values}>
          {(addr, idx) => (
            <span
              class="composer__pill"
              classList={{ "composer__pill--invalid": !isValidEmail(addr) }}
            >
              <span class="composer__pill-text">{addr}</span>
              <button
                type="button"
                class="composer__pill-remove"
                onClick={() => removeAddress(idx())}
                aria-label={`Remove ${addr}`}
              >
                ×
              </button>
            </span>
          )}
        </For>
        <input
          type="text"
          class="composer__address-input"
          value={inputVal()}
          onInput={(e) => {
            setInputVal(e.currentTarget.value);
            loadSuggestions(e.currentTarget.value);
          }}
          onFocus={() => loadSuggestions(inputVal())}
          onKeyDown={handleKeyDown}
          onPaste={handlePaste}
          onBlur={() => {
            if (inputVal().trim()) {
              addAddresses(inputVal());
            }
            setOpen(false);
          }}
          placeholder={props.values.length === 0 ? props.placeholder : ""}
          role="combobox"
          aria-expanded={open()}
          aria-autocomplete="list"
        />
        <Show when={open()}>
          <ul
            class="composer__autocomplete"
            role="listbox"
            onMouseDown={(e) => e.preventDefault() /* keep the input focused */}
          >
            <For each={suggestions()}>
              {(s, i) => (
                <li
                  class="composer__autocomplete-row"
                  classList={{ "is-selected": selIdx() === i() }}
                  role="option"
                  aria-selected={selIdx() === i()}
                >
                  <button
                    type="button"
                    class="composer__autocomplete-insert"
                    onClick={() => insertSuggestion(s)}
                  >
                    <span class="composer__autocomplete-name">
                      {s.name || s.address}
                    </span>
                    <span class="composer__autocomplete-address">
                      {s.name ? s.address : ""}
                    </span>
                  </button>
                  <button
                    type="button"
                    class="composer__autocomplete-hide"
                    aria-label={`Never suggest ${s.address}`}
                    onClick={() => void dismissSuggestion(s)}
                  >
                    ×
                  </button>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </div>
    </div>
  );
}

const ATTACHMENT_KEYWORDS = [
  "attach",
  "attached",
  "attaching",
  "attachment",
  "attachments",
  "pdf",
  "file",
  "files",
  "doc",
  "docx",
  "zip",
];

function checkMissingAttachmentIntent(
  subject: string,
  body: string,
  attachmentCount: number,
): boolean {
  if (attachmentCount > 0) return false;
  const text = `${subject} ${body}`.toLowerCase();
  return ATTACHMENT_KEYWORDS.some((kw) => {
    const reg = new RegExp(`\\b${kw}\\b`, "i");
    return reg.test(text);
  });
}

export function Composer() {
  const { open, draft: d, attachments, sendError } = useComposer();
  const accounts = useAccounts();
  const [showCcBcc, setShowCcBcc] = createSignal(false);
  const [showMissingAttachmentPrompt, setShowMissingAttachmentPrompt] =
    createSignal(false);
  const [isDragging, setIsDragging] = createSignal(false);
  const [identities, setIdentities] = createSignal<AccountIdentity[]>([]);
  // P1.2 wrong-account warning — resets whenever the from/original pairing
  // changes, so dismissing it doesn't silence a later mismatch.
  const [warnDismissed, setWarnDismissed] = createSignal(false);
  let warnPair = "";
  createEffect(() => {
    const current = d();
    const pair = current
      ? `${current.accountId}:${current.originalAccountId ?? ""}`
      : "";
    if (pair !== warnPair) {
      warnPair = pair;
      setWarnDismissed(false);
    }
  });
  let fileInputRef: HTMLInputElement | undefined;

  createEffect(() => {
    if (open()) {
      void getSettings().then((s) => {
        setIdentities(s.identities || []);
      });
    }
  });

  const onClose = () => {
    void persistDraft();
    setShowMissingAttachmentPrompt(false);
    closeComposer();
  };

  const handleFiles = async (files: FileList | null) => {
    if (!files) return;
    for (const file of Array.from(files)) {
      const dataBase64 = await fileToBase64(file);
      addComposerAttachment({
        name: file.name,
        size: file.size,
        type: file.type || "application/octet-stream",
        dataBase64,
      });
    }
    setShowMissingAttachmentPrompt(false);
  };

  const handleSubmit = (event: Event) => {
    event.preventDefault();
    const current = d();
    if (
      current &&
      checkMissingAttachmentIntent(
        current.subject,
        current.body,
        attachments().length,
      ) &&
      !showMissingAttachmentPrompt()
    ) {
      setShowMissingAttachmentPrompt(true);
      return;
    }
    setShowMissingAttachmentPrompt(false);
    void sendComposer();
  };

  const handlePasteInBody = async (e: ClipboardEvent) => {
    const items = e.clipboardData?.items;
    if (!items) return;
    for (const item of Array.from(items)) {
      if (item.type.startsWith("image/")) {
        const file = item.getAsFile();
        if (file) {
          e.preventDefault();
          const base64 = await fileToBase64(file);
          const ext = file.type.split("/")[1] || "png";
          const name = `Pasted-Image-${Date.now()}.${ext}`;
          addComposerAttachment({
            name,
            size: file.size,
            type: file.type,
            dataBase64: base64,
          });
        }
      }
    }
  };

  const title = () =>
    d()?.intent === "forward"
      ? "Forward"
      : d()?.intent === "replyAll"
        ? "Reply all"
        : d()?.intent === "new"
          ? "New message"
          : "Reply";

  const totalAttachmentSize = () =>
    attachments().reduce((acc, a) => acc + a.size, 0);

  const hasCcOrBcc = () => {
    const current = d();
    if (!current) return false;
    return current.cc.length > 0 || current.bcc.length > 0;
  };

  const availableIdentities = () => {
    const current = d();
    if (!current) return [];
    const all = identities();
    const accts = accounts();
    if (all.length > 0) return all;
    return accts.map((a) => ({
      id: `acct_${a.id}`,
      account_id: a.id,
      name: "",
      email: a.address,
      reply_to: null,
      signature: null,
      is_default: true,
    }));
  };

  return (
    <Show when={open() && d()}>
      <Modal title={title()} onClose={onClose}>
        <form
          class="composer"
          classList={{ "composer--dragging": isDragging() }}
          onSubmit={handleSubmit}
          onDragOver={(event) => {
            event.preventDefault();
            setIsDragging(true);
          }}
          onDragLeave={(event) => {
            if (event.currentTarget === event.target) {
              setIsDragging(false);
            }
          }}
          onDrop={(event) => {
            event.preventDefault();
            setIsDragging(false);
            void handleFiles(event.dataTransfer?.files ?? null);
          }}
        >
          {/* From identity — always visible (P1.2): exactly who will send. The
              <select> appears only when there is more than one identity. */}
          <div class="composer__field composer__field--from">
            <span class="composer__label">From</span>
            <Show
              when={availableIdentities().length > 1}
              fallback={
                <span class="composer__from-static">
                  {d()!.fromName
                    ? `${d()!.fromName} <${d()!.fromAddress ?? ""}>`
                    : (d()!.fromAddress ?? "")}
                </span>
              }
            >
              <select
                class="composer__select-from"
                value={d()!.identityId || `acct_${d()!.accountId}`}
                onChange={(e) => {
                  const val = e.currentTarget.value;
                  const ident = identities().find((i) => i.id === val);
                  if (ident) {
                    switchComposerIdentity(ident);
                  } else if (val.startsWith("acct_")) {
                    const acctId = Number(val.replace("acct_", ""));
                    const acct = accounts().find((a) => a.id === acctId);
                    if (acct) {
                      updateDraft({
                        accountId: acct.id,
                        identityId: null,
                        fromName: null,
                        fromAddress: acct.address,
                        replyTo: null,
                      });
                    }
                  }
                }}
              >
                <For each={availableIdentities()}>
                  {(ident) => (
                    <option value={ident.id}>
                      {ident.name
                        ? `${ident.name} <${ident.email}>`
                        : ident.email}
                    </option>
                  )}
                </For>
              </select>
            </Show>
          </div>

          {/* P1.2 wrong-account warning: the From switched to a different
              account than the original message's account. */}
          <Show
            when={
              !warnDismissed() &&
              d()!.originalAccountId != null &&
              d()!.accountId !== d()!.originalAccountId
            }
          >
            <div class="composer__warn" role="alert">
              <span>
                Replying from a different account than the original message —
                this reply will come from {d()!.fromAddress}.
              </span>
              <button
                type="button"
                class="composer__warn-dismiss"
                aria-label="Dismiss warning"
                onClick={() => setWarnDismissed(true)}
              >
                ×
              </button>
            </div>
          </Show>

          <div class="composer__row-to">
            <AddressInput
              label="To"
              values={d()!.to}
              onChange={(to) => updateDraft({ to })}
              placeholder="recipient@example.com"
            />
            <Show when={!showCcBcc() && !hasCcOrBcc()}>
              <button
                type="button"
                class="composer__toggle-cc"
                onClick={() => setShowCcBcc(true)}
              >
                Cc / Bcc
              </button>
            </Show>
          </div>

          <Show when={showCcBcc() || hasCcOrBcc()}>
            <AddressInput
              label="Cc"
              values={d()!.cc}
              onChange={(cc) => updateDraft({ cc })}
              placeholder="cc@example.com"
            />
            <AddressInput
              label="Bcc"
              values={d()!.bcc}
              onChange={(bcc) => updateDraft({ bcc })}
              placeholder="bcc@example.com"
            />
          </Show>

          <label class="composer__field">
            <span class="composer__label">Subject</span>
            <input
              type="text"
              value={d()!.subject}
              onInput={(e) => updateDraft({ subject: e.currentTarget.value })}
              placeholder="Subject"
            />
          </label>

          <textarea
            class="composer__body"
            value={d()!.body}
            onInput={(e) => updateDraft({ body: e.currentTarget.value })}
            onPaste={handlePasteInBody}
            placeholder="Write your message…"
          />

          <Show when={attachments().length > 0}>
            <ul class="composer__attachments">
              <For each={attachments()}>
                {(attachment) => (
                  <li class="composer__attachment">
                    <span class="composer__attachment-name">
                      {attachment.name}
                    </span>
                    <span class="composer__attachment-size">
                      {formatBytes(attachment.size)}
                    </span>
                    <button
                      type="button"
                      class="composer__attachment-remove"
                      onClick={() => removeComposerAttachment(attachment.name)}
                      aria-label={`Remove attachment ${attachment.name}`}
                    >
                      Remove
                    </button>
                  </li>
                )}
              </For>
            </ul>
          </Show>

          <Show when={showMissingAttachmentPrompt()}>
            <div class="composer__warning composer__guard" role="alert">
              <span>
                You mentioned an attachment, but haven't attached any files.
              </span>
              <div class="composer__guard-actions">
                <button
                  type="button"
                  class="btn btn--secondary btn--sm"
                  onClick={() => fileInputRef?.click()}
                >
                  Add files
                </button>
                <button
                  type="button"
                  class="btn btn--primary btn--sm"
                  onClick={() => {
                    setShowMissingAttachmentPrompt(false);
                    void sendComposer();
                  }}
                >
                  Send anyway
                </button>
              </div>
            </div>
          </Show>

          <Show when={totalAttachmentSize() > MAX_ATTACHMENT_SIZE}>
            <div class="composer__warning" role="alert">
              Total attachments exceed 25 MB (
              {formatBytes(totalAttachmentSize())}). Some servers may reject
              large messages.
            </div>
          </Show>

          <Show when={sendError()}>
            <div class="composer__error" role="alert">
              {sendError()}
            </div>
          </Show>

          <div class="composer__actions">
            <label class="composer__attach">
              Attach files
              <input
                ref={fileInputRef}
                type="file"
                multiple
                hidden
                onChange={(event) =>
                  void handleFiles(event.currentTarget.files)
                }
              />
            </label>
            <span class="composer__hint">Autosaves to Drafts</span>
            <button
              type="button"
              class="btn btn--secondary"
              onClick={() => void discardComposer()}
            >
              Discard
            </button>
            <SendLaterMenu onSchedule={(at) => void scheduleComposer(at)} />
            <button type="submit" class="btn btn--primary">
              Send
            </button>
          </div>
        </form>
      </Modal>
    </Show>
  );
}
