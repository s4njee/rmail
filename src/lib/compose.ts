import { createSignal } from "solid-js";
import type { AccountIdentity } from "./ipc/AccountIdentity";
import type { AppSettings } from "./ipc/AppSettings";
import type { Draft } from "./ipc/Draft";
import type { MessageDetail } from "./ipc/MessageDetail";
import type { OutgoingMessage } from "./ipc/OutgoingMessage";
import { refreshMail, useAccounts, useDetail } from "./mail";
import {
  deleteMessage,
  getSettings,
  latestDraft,
  saveDraft,
  scheduleSend,
  sendMessage,
} from "./tauri";

// Compose state (Epic 13 & Roadmap 3.5):
// - Opened by Reply / Reply all / Forward / New
// - Per-account & per-identity signatures (plain + HTML, reply vs new placement)
// - Aliases / send-as identities per account
// - Undo send (configurable 5-30s delay)
// - Autosaved to Drafts folder within 2s of typing stopping

export type ComposeIntent = "reply" | "replyAll" | "forward" | "new";

export type ComposerAttachment = {
  name: string;
  size: number;
  type: string;
  dataBase64: string;
};

export type ComposerDraft = {
  intent: ComposeIntent;
  accountId: number;
  identityId: string | null;
  fromName: string | null;
  fromAddress: string | null;
  replyTo: string | null;
  to: string[];
  cc: string[];
  bcc: string[];
  subject: string;
  body: string;
  bodyHtml: string | null;
  inReplyTo: string | null;
  references: string | null;
  originalMessageId: number | null;
  isForward: boolean | null;
  /** The account the original message lived in (reply/forward) — used to warn
   * when the From identity switches to a different account (P1.2). */
  originalAccountId: number | null;
};

export type PendingSend = {
  id: string;
  outgoing: OutgoingMessage;
  draftSnapshot: {
    draft: ComposerDraft;
    draftId: number | null;
    attachments: ComposerAttachment[];
  };
  secondsRemaining: number;
  totalSeconds: number;
};

const [open, setOpen] = createSignal(false);
const [draft, setDraft] = createSignal<ComposerDraft | null>(null);
const [draftId, setDraftId] = createSignal<number | null>(null);
const [attachments, setAttachments] = createSignal<ComposerAttachment[]>([]);
const [sendError, setSendError] = createSignal("");
const [pendingSend, setPendingSend] = createSignal<PendingSend | null>(null);

let pendingSendInterval: ReturnType<typeof setInterval> | undefined;

export function useComposer() {
  return { open, draft, draftId, attachments, sendError, pendingSend };
}

/** `r`/`a`/`f` and the action-bar buttons — open the composer pre-filled. */
export function requestCompose(intent: ComposeIntent): void {
  if (intent === "new") {
    void openNewComposer();
    return;
  }
  const detail = useDetail()();
  if (detail) void openComposer(intent, detail);
}

function cleanSubject(subject: string, prefix: "Re: " | "Fwd: "): string {
  const trimmed = subject.trim();
  const cleaned = trimmed.replace(/^(?:Re:\s*|Fwd:\s*)+/i, "");
  return `${prefix}${cleaned}`;
}

export function resolveIdentity(
  settings: AppSettings,
  accountId: number,
  identityId?: string | null,
): AccountIdentity | null {
  const identities = settings.identities || [];
  if (identityId) {
    const found = identities.find((i) => i.id === identityId);
    if (found) return found;
  }
  const matching = identities.filter((i) => i.accountId === accountId);
  return matching.find((i) => i.isDefault) || matching[0] || null;
}

export async function openNewComposer(
  preferredAccountId?: number,
): Promise<void> {
  const accounts = useAccounts()();
  const acct = preferredAccountId
    ? accounts.find((a) => a.id === preferredAccountId) || accounts[0]
    : accounts[0];
  const accountId = acct ? acct.id : 1;

  const settings = await getSettings();
  const identity = resolveIdentity(settings, accountId);

  let initialBody = "";
  let initialBodyHtml: string | null = null;

  if (identity?.signature && identity.signature.includeInNewMail) {
    if (identity.signature.plainText) {
      initialBody = `\n\n${identity.signature.plainText}`;
    }
    if (identity.signature.html) {
      initialBodyHtml = `<p><br></p>${identity.signature.html}`;
    }
  }

  setDraft({
    intent: "new",
    accountId,
    identityId: identity?.id || null,
    fromName: identity?.name || null,
    fromAddress: identity?.email || acct?.address || null,
    replyTo: identity?.replyTo || null,
    to: [],
    cc: [],
    bcc: [],
    subject: "",
    body: initialBody,
    bodyHtml: initialBodyHtml,
    inReplyTo: null,
    references: null,
    originalMessageId: null,
    isForward: null,
    originalAccountId: null,
  });
  setDraftId(null);
  setAttachments([]);
  setSendError("");
  setOpen(true);
}

export async function openComposer(
  intent: ComposeIntent,
  detail: MessageDetail,
): Promise<void> {
  const original = detail.row;
  const accounts = useAccounts()();
  const myAccount = accounts.find((a) => a.id === original.account_id);
  const myAddress = (myAccount?.address || "").toLowerCase();

  const settings = await getSettings();
  // P1.2: if the original was addressed to one of our aliases, reply from
  // that alias (its signature included) instead of the account default.
  const addressedIdentity =
    (settings.identities || []).find(
      (i) =>
        i.accountId === original.account_id &&
        detail.to
          .concat(detail.cc)
          .some((r) => r.address.toLowerCase() === i.email.toLowerCase()),
    ) ?? null;
  const identity =
    addressedIdentity || resolveIdentity(settings, original.account_id);

  let to: string[] = [];
  let cc: string[] = [];
  if (intent === "reply") {
    to = [original.sender_address];
  } else if (intent === "replyAll") {
    const rawTo = [original.sender_address, ...detail.to.map((r) => r.address)];
    to = rawTo
      .filter((addr) => addr.toLowerCase() !== myAddress)
      .filter(
        (a, i, arr) =>
          arr.findIndex((x) => x.toLowerCase() === a.toLowerCase()) === i,
      );

    if (to.length === 0 && original.sender_address) {
      to = [original.sender_address];
    }

    cc = detail.cc
      .map((r) => r.address)
      .filter(
        (addr) =>
          addr.toLowerCase() !== myAddress &&
          !to.some((t) => t.toLowerCase() === addr.toLowerCase()),
      )
      .filter(
        (a, i, arr) =>
          arr.findIndex((x) => x.toLowerCase() === a.toLowerCase()) === i,
      );
  }

  const prefix = intent === "forward" ? "Fwd: " : "Re: ";
  const subject = cleanSubject(original.subject, prefix);

  let inReplyTo: string | null = null;
  let references: string | null = null;

  if (intent === "reply" || intent === "replyAll") {
    inReplyTo = detail.message_id_header || null;
    if (detail.references && detail.message_id_header) {
      references = `${detail.references} ${detail.message_id_header}`.trim();
    } else {
      references = detail.references || detail.message_id_header || null;
    }
  } else if (intent === "forward") {
    references = detail.references || detail.message_id_header || null;
  }

  const quote = quotedBody(detail);
  let body = `\n\n${quote}`;
  let bodyHtml: string | null = null;

  if (identity?.signature && identity.signature.includeInReplies) {
    const sig = identity.signature;
    if (sig.plainText) {
      if (sig.replyPlacement === "bottom") {
        body = `\n\n${quote}\n\n${sig.plainText}`;
      } else {
        body = `\n\n${sig.plainText}\n\n${quote}`;
      }
    }
    if (sig.html) {
      if (sig.replyPlacement === "bottom") {
        bodyHtml = `<p><br></p><blockquote>${quote}</blockquote><br>${sig.html}`;
      } else {
        bodyHtml = `<p><br></p>${sig.html}<br><blockquote>${quote}</blockquote>`;
      }
    }
  }

  setDraft({
    intent,
    accountId: original.account_id,
    identityId: identity?.id || null,
    fromName: identity?.name || null,
    fromAddress: identity?.email || myAccount?.address || null,
    replyTo: identity?.replyTo || null,
    to,
    cc,
    bcc: [],
    subject,
    body,
    bodyHtml,
    inReplyTo,
    references,
    originalMessageId: original.id,
    isForward: intent === "forward",
    originalAccountId: original.account_id,
  });
  setDraftId(null);
  setAttachments([]);
  setSendError("");
  setOpen(true);
}

export function switchComposerIdentity(identity: AccountIdentity): void {
  const current = draft();
  if (!current) return;

  setDraft({
    ...current,
    accountId: identity.accountId,
    identityId: identity.id,
    fromName: identity.name,
    fromAddress: identity.email,
    replyTo: identity.replyTo,
  });
  if (autosaveTimer) clearTimeout(autosaveTimer);
  autosaveTimer = setTimeout(() => void persistDraft(), 1000);
}

function quotedBody(detail: MessageDetail): string {
  const date = new Date(detail.row.received_at_ms).toLocaleString();
  const from = detail.row.sender_name
    ? `${detail.row.sender_name} <${detail.row.sender_address}>`
    : detail.row.sender_address;
  const quote = detail.body.map((p) => `> ${p}`).join("\n");
  return `On ${date}, ${from} wrote:\n${quote}`;
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
    bcc: d.bcc,
    subject: d.subject,
    body: d.body,
    in_reply_to: d.inReplyTo,
    references: d.references,
  });
  setDraftId(id);
}

async function executeSend(
  outgoing: OutgoingMessage,
  draftIdToRemove: number | null,
): Promise<void> {
  try {
    await sendMessage(outgoing);
    if (draftIdToRemove != null) {
      await deleteMessage(draftIdToRemove);
    }
    await refreshMail();
  } catch (error) {
    console.error("Send failed:", error);
    setSendError(String(error));
  }
}

export async function sendComposer(): Promise<void> {
  const d = draft();
  if (!d) return;
  setSendError("");
  try {
    await persistDraft(); // ensure the latest keystrokes are in the store

    const outgoing: OutgoingMessage = {
      account_id: d.accountId,
      from_name: d.fromName,
      from_address: d.fromAddress,
      reply_to: d.replyTo,
      to: d.to,
      cc: d.cc,
      bcc: d.bcc,
      subject: d.subject,
      body: d.body,
      body_html: d.bodyHtml,
      in_reply_to: d.inReplyTo,
      references: d.references,
      attachments: attachments().map((a) => ({
        filename: a.name,
        content_type: a.type || "application/octet-stream",
        data_base64: a.dataBase64,
      })),
      original_message_id: d.originalMessageId,
      is_forward: d.isForward,
    };

    const currentDraftId = draftId();
    const currentAttachments = [...attachments()];
    const draftSnapshot = {
      draft: { ...d },
      draftId: currentDraftId,
      attachments: currentAttachments,
    };

    const settings = await getSettings();
    const delaySec = Math.max(0, settings.undoSendDelaySec ?? 0);

    if (delaySec > 0) {
      // Close composer UI immediately and start Undo countdown
      closeComposer();

      const sendId = `send_${Date.now()}`;
      setPendingSend({
        id: sendId,
        outgoing,
        draftSnapshot,
        secondsRemaining: delaySec,
        totalSeconds: delaySec,
      });

      if (pendingSendInterval) clearInterval(pendingSendInterval);
      pendingSendInterval = setInterval(() => {
        const ps = pendingSend();
        if (!ps || ps.id !== sendId) {
          if (pendingSendInterval) clearInterval(pendingSendInterval);
          return;
        }
        if (ps.secondsRemaining <= 1) {
          clearInterval(pendingSendInterval);
          setPendingSend(null);
          void executeSend(ps.outgoing, ps.draftSnapshot.draftId);
        } else {
          setPendingSend({
            ...ps,
            secondsRemaining: ps.secondsRemaining - 1,
          });
        }
      }, 1000);
    } else {
      closeComposer();
      await executeSend(outgoing, currentDraftId);
    }
  } catch (error) {
    setSendError(String(error));
  }
}

/** Build the OutgoingMessage for the current draft + attachments. */
function currentOutgoing(): OutgoingMessage | null {
  const d = draft();
  if (!d) return null;
  return {
    account_id: d.accountId,
    from_name: d.fromName,
    from_address: d.fromAddress,
    reply_to: d.replyTo,
    to: d.to,
    cc: d.cc,
    bcc: d.bcc,
    subject: d.subject,
    body: d.body,
    body_html: d.bodyHtml,
    in_reply_to: d.inReplyTo,
    references: d.references,
    attachments: attachments().map((a) => ({
      filename: a.name,
      content_type: a.type || "application/octet-stream",
      data_base64: a.dataBase64,
    })),
    original_message_id: d.originalMessageId,
    is_forward: d.isForward,
  };
}

/** Snapshot the composer state so Edit can reopen it from the Scheduled view. */
function composerSnapshot(): string {
  return JSON.stringify({
    draft: { ...draft() },
    draftId: draftId(),
    attachments: [...attachments()],
  });
}

/** Reopen the composer from a stored Draft message (P1.5 resume). */
export function openDraftMessage(d: Draft): void {
  setDraft({
    intent: "new",
    accountId: d.account_id,
    identityId: null,
    fromName: null,
    fromAddress:
      useAccounts()().find((a) => a.id === d.account_id)?.address ?? null,
    replyTo: null,
    to: d.to,
    cc: d.cc,
    bcc: d.bcc,
    subject: d.subject,
    body: d.body,
    bodyHtml: null,
    inReplyTo: d.in_reply_to,
    references: d.references,
    originalMessageId: null,
    isForward: null,
    originalAccountId: null,
  });
  setDraftId(d.id ?? null);
  setAttachments([]);
  setSendError("");
  setOpen(true);
}

/** Open the composer from a `mailto:` deep link or tray action (P1.5). */
export async function openMailto(p: {
  to: string;
  subject: string;
  body: string;
}): Promise<void> {
  await openNewComposer();
  const d = draft();
  if (!d) return;
  updateDraft({
    to: p.to
      ? p.to
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean)
      : d.to,
    subject: p.subject || d.subject,
    body: p.body ? `${p.body}${d.body ? `\n\n${d.body}` : ""}` : d.body,
  });
}

/** Resume the most recent unfinished draft into the composer (P1.5). */
export async function resumeDraft(): Promise<void> {
  try {
    const d = await latestDraft();
    if (d && (d.subject || d.body || d.to.length > 0)) {
      openDraftMessage(d);
    }
  } catch {
    /* non-fatal */
  }
}

/** Reopen the composer from a snapshot produced by `composerSnapshot`. */
export function reopenComposerFromSnapshot(snapshot: string): void {
  try {
    const parsed = JSON.parse(snapshot);
    if (parsed.draft) setDraft(parsed.draft);
    setDraftId(parsed.draftId ?? null);
    setAttachments(Array.isArray(parsed.attachments) ? parsed.attachments : []);
    setSendError("");
    setOpen(true);
  } catch {
    /* a corrupt snapshot just doesn't reopen */
  }
}

/** Schedule the current draft to send later (P1.1) — closes the composer and
 * persists the message in the durable Outbox. The app must be running at the
 * send time. */
export async function scheduleComposer(sendAtMs: number): Promise<void> {
  const d = draft();
  if (!d) return;
  setSendError("");
  try {
    await persistDraft();
    const outgoing = currentOutgoing();
    if (!outgoing) return;
    await scheduleSend(outgoing, sendAtMs, composerSnapshot());
    closeComposer();
  } catch (error) {
    setSendError(String(error));
  }
}

export function undoPendingSend(): void {
  const ps = pendingSend();
  if (!ps) return;
  if (pendingSendInterval) clearInterval(pendingSendInterval);
  setPendingSend(null);

  // Restore the composer modal with the snapshot
  setDraft(ps.draftSnapshot.draft);
  setDraftId(ps.draftSnapshot.draftId);
  setAttachments(ps.draftSnapshot.attachments);
  setSendError("");
  setOpen(true);
}

export function sendPendingNow(): void {
  const ps = pendingSend();
  if (!ps) return;
  if (pendingSendInterval) clearInterval(pendingSendInterval);
  setPendingSend(null);
  void executeSend(ps.outgoing, ps.draftSnapshot.draftId);
}

export async function discardComposer(): Promise<void> {
  if (draftId() != null) await deleteMessage(draftId()!);
  closeComposer();
  await refreshMail();
}
