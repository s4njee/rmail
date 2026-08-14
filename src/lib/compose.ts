import { createSignal } from "solid-js";
import type { MessageDetail } from "./ipc/MessageDetail";
import { refreshMail, useDetail } from "./mail";
import { deleteMessage, saveDraft, sendMessage } from "./tauri";

// Compose state (Epic 13): opened by Reply / Reply all / Forward (buttons and
// the r/a/f keybindings), pre-filled from the selected message, autosaved to
// the Drafts folder within 2s of typing stopping, and sent over SMTP.

export type ComposeIntent = "reply" | "replyAll" | "forward";

export type ComposerDraft = {
  intent: ComposeIntent;
  accountId: number;
  to: string[];
  cc: string[];
  subject: string;
  body: string;
};

export type ComposerAttachment = { name: string; size: number };

const [open, setOpen] = createSignal(false);
const [draft, setDraft] = createSignal<ComposerDraft | null>(null);
const [draftId, setDraftId] = createSignal<number | null>(null);
const [attachments, setAttachments] = createSignal<ComposerAttachment[]>([]);
const [sendError, setSendError] = createSignal("");

export function useComposer() {
  return { open, draft, draftId, attachments, sendError };
}

/** `r`/`a`/`f` and the action-bar buttons — open the composer pre-filled. */
export function requestCompose(intent: ComposeIntent): void {
  const detail = useDetail()();
  if (detail) openComposer(intent, detail);
}

export function openComposer(
  intent: ComposeIntent,
  detail: MessageDetail,
): void {
  const original = detail.row;
  const recipients = [...detail.to, ...detail.cc];

  let to: string[] = [];
  let cc: string[] = [];
  if (intent === "reply") {
    to = [original.sender_address];
  } else if (intent === "replyAll") {
    to = recipients
      .filter((r) => r.name !== "me")
      .map((r) => r.address)
      .filter((a, i, arr) => arr.indexOf(a) === i);
    cc = detail.cc.filter((r) => r.name !== "me").map((r) => r.address);
  }

  setDraft({
    intent,
    accountId: original.account_id,
    to,
    cc,
    subject: `${intent === "forward" ? "Fwd: " : "Re: "}${original.subject}`,
    body: quotedBody(detail),
  });
  setDraftId(null);
  setAttachments([]);
  setSendError("");
  setOpen(true);
}

function quotedBody(detail: MessageDetail): string {
  const date = new Date(detail.row.received_at_ms).toLocaleString();
  const from = detail.row.sender_address;
  const quote = detail.body.map((p) => `> ${p}`).join("\n");
  if (detail.row.subject.startsWith("Re:")) {
    return `\n\nOn ${date}, ${from} wrote:\n${quote}`;
  }
  return `\n\nOn ${date}, ${from} wrote:\n${quote}`;
}

export function closeComposer(): void {
  setOpen(false);
  setDraft(null);
  setDraftId(null);
  setAttachments([]);
}

let autosaveTimer: ReturnType<typeof setTimeout> | undefined;

export function updateDraft(patch: Partial<ComposerDraft>): void {
  setDraft((d) => (d ? { ...d, ...patch } : d));
  if (autosaveTimer) clearTimeout(autosaveTimer);
  autosaveTimer = setTimeout(() => void persistDraft(), 2000);
}

export function addComposerAttachment(attachment: ComposerAttachment): void {
  setAttachments((a) => [...a, attachment]);
}

export function removeComposerAttachment(name: string): void {
  setAttachments((a) => a.filter((x) => x.name !== name));
}

/** Save the current draft to the Drafts folder (Epic 13.2). */
export async function persistDraft(): Promise<void> {
  const d = draft();
  if (!d) return;
  if (!d.subject && !d.body && d.to.length === 0) return; // nothing to save
  const id = await saveDraft({
    id: draftId(),
    account_id: d.accountId,
    to: d.to,
    cc: d.cc,
    subject: d.subject,
    body: d.body,
  });
  setDraftId(id);
}

export async function sendComposer(): Promise<void> {
  const d = draft();
  if (!d) return;
  setSendError("");
  try {
    await persistDraft(); // ensure the latest keystrokes are in the store
    await sendMessage({
      account_id: d.accountId,
      to: d.to,
      cc: d.cc,
      subject: d.subject,
      body: d.body,
    });
    if (draftId() != null) await deleteMessage(draftId()!);
    closeComposer();
    await refreshMail(); // Drafts count moves back down
  } catch (error) {
    setSendError(String(error));
  }
}

export async function discardComposer(): Promise<void> {
  if (draftId() != null) await deleteMessage(draftId()!);
  closeComposer();
  await refreshMail();
}
