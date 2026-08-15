import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import { createEffect, createSignal, For, Show } from "solid-js";
import { requestCompose } from "../lib/compose";
import {
  avatarInitials,
  formatBytes,
  formatFullDate,
  formatRelativeTime,
  getAttachmentTypeInfo,
} from "../lib/format";
import type { Attachment } from "../lib/ipc/Attachment";
import type { MessageDetail } from "../lib/ipc/MessageDetail";
import type { MessageProgressUpdate } from "../lib/ipc/MessageProgressUpdate";
import {
  useAccounts,
  useDetail,
  useDetailLoading,
  useDetailProgress,
  useRows,
  useSelectedId,
  loadDetail,
  markMessageJunk,
  performThreadAction,
  snooze,
} from "../lib/mail";
import { useSettings, updateSettings } from "../lib/settings";
import { useTheme } from "../lib/theme";
import {
  attachmentPath,
  exportMessageEml,
  getThreadMessages,
  saveAllAttachments,
  unsubscribe,
} from "../lib/tauri";
import { exitFocused } from "../lib/ui";
import { InviteCard } from "./InviteCard";
import { MailBody } from "./MailBody";
import { Modal } from "./Modal";
import { SnoozeMenu } from "./SnoozeMenu";
import "./ReadingPane.css";

// P1.6: download a string as a file (the same Blob pattern the calendar's ICS
// export uses).
function downloadText(filename: string, text: string): void {
  const blob = new Blob([text], { type: "text/plain;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

async function exportEml(id: number, subject: string): Promise<void> {
  const text = await exportMessageEml(id);
  const safe =
    subject
      .replace(/[^\w\- ]/g, "")
      .slice(0, 60)
      .trim() || "message";
  downloadText(`${safe}.eml`, text);
}

function QuickPreviewModal(props: {
  attachment: Attachment;
  onClose: () => void;
}) {
  const [localPath, setLocalPath] = createSignal<string | null>(null);
  const typeInfo = () => getAttachmentTypeInfo(props.attachment.filename);

  createEffect(() => {
    void attachmentPath(props.attachment.id).then((p) => {
      setLocalPath(p ?? null);
    });
  });

  return (
    <Modal
      title={`Preview — ${props.attachment.filename}`}
      onClose={props.onClose}
    >
      <div class="preview-modal">
        <div class="preview-modal__body">
          <div class="preview-modal__generic">
            <span
              class={`attachment-badge attachment-badge--${typeInfo().category} attachment-badge--large`}
            >
              {typeInfo().label}
            </span>
            <p class="preview-modal__filename">{props.attachment.filename}</p>
            <p class="preview-modal__filesize">
              {formatBytes(props.attachment.size_bytes)}
            </p>
          </div>
        </div>
        <div class="preview-modal__footer">
          <span class="preview-modal__meta">
            {formatBytes(props.attachment.size_bytes)}
          </span>
          <button
            type="button"
            class="btn btn--secondary btn--sm"
            onClick={() => {
              const p = localPath();
              if (p) void openPath(p);
            }}
          >
            Open in default app
          </button>
        </div>
      </div>
    </Modal>
  );
}

function AttachmentCard(props: {
  attachment: Attachment;
  onPreview: (attachment: Attachment) => void;
}) {
  const typeInfo = () => getAttachmentTypeInfo(props.attachment.filename);

  const open = () => {
    void attachmentPath(props.attachment.id).then((path) => {
      if (path) void openPath(path);
    });
  };

  return (
    <div
      class="attachment-card"
      draggable="true"
      onDragStart={(e) => {
        void attachmentPath(props.attachment.id).then((path) => {
          if (path && e.dataTransfer) {
            e.dataTransfer.setData("text/plain", path);
            e.dataTransfer.setData("text/uri-list", `file://${path}`);
          }
        });
      }}
      onClick={open}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          open();
        }
      }}
      aria-label={`Attachment: ${props.attachment.filename}`}
    >
      <span class={`attachment-badge attachment-badge--${typeInfo().category}`}>
        {typeInfo().label}
      </span>
      <div class="attachment-info">
        <span class="attachment-name" title={props.attachment.filename}>
          {props.attachment.filename}
        </span>
        <span class="attachment-size">
          {formatBytes(props.attachment.size_bytes)}
        </span>
      </div>
      <div class="attachment-actions">
        <button
          type="button"
          class="attachment-action-btn"
          title="Quick preview"
          onClick={(e) => {
            e.stopPropagation();
            props.onPreview(props.attachment);
          }}
        >
          👁
        </button>
        <button
          type="button"
          class="attachment-action-btn"
          title="Open file"
          onClick={(e) => {
            e.stopPropagation();
            open();
          }}
        >
          ↗
        </button>
      </div>
    </div>
  );
}

function AttachmentsSection(props: {
  messageId: number;
  attachments: Attachment[];
  onPreview: (attachment: Attachment) => void;
}) {
  const [saveStatus, setSaveStatus] = createSignal<string | null>(null);

  const handleSaveAll = async () => {
    setSaveStatus("Saving...");
    try {
      const count = await saveAllAttachments(props.messageId, "~/Downloads");
      setSaveStatus(count > 0 ? `Saved ${count} files` : "Saved to Downloads");
      setTimeout(() => setSaveStatus(null), 3000);
    } catch {
      setSaveStatus("Save failed");
      setTimeout(() => setSaveStatus(null), 3000);
    }
  };

  return (
    <div class="attachments-section">
      <div class="attachments-section__header">
        <span class="attachments-section__title">
          Attachments ({props.attachments.length})
        </span>
        <Show when={props.attachments.length > 1}>
          <button
            type="button"
            class="attachments-section__save-all btn btn--secondary btn--sm"
            onClick={handleSaveAll}
          >
            {saveStatus() ?? "Save all"}
          </button>
        </Show>
      </div>
      <div class="attachments-grid">
        <For each={props.attachments}>
          {(attachment) => (
            <AttachmentCard
              attachment={attachment}
              onPreview={props.onPreview}
            />
          )}
        </For>
      </div>
    </div>
  );
}

// The message body — HTML via the sandboxed iframe (7.3) or plain paragraphs —
// plus attachment cards. Shared by the three-pane and focused layouts.
function MessageBody(props: {
  d: MessageDetail;
  allowImages: boolean;
  onLoadImages: () => void;
  onAlwaysTrustSender?: () => void;
  onOpenLink: (url: string) => void;
  onPreview: (attachment: Attachment) => void;
}) {
  return (
    <>
      <Show when={props.d.calendar_invite}>
        {(invite) => (
          <InviteCard
            invite={invite()}
            accountId={props.d.row.account_id}
            messageId={props.d.row.id}
          />
        )}
      </Show>
      {props.d.body_html ? (
        <MailBody
          detail={props.d}
          allowImages={props.allowImages}
          onLoadImages={props.onLoadImages}
          onAlwaysTrustSender={props.onAlwaysTrustSender}
          onOpenLink={props.onOpenLink}
        />
      ) : (
        <For each={props.d.body}>
          {(paragraph) => <p class="reading-paragraph">{paragraph}</p>}
        </For>
      )}
      <Show when={props.d.attachments.length > 0}>
        <AttachmentsSection
          messageId={props.d.row.id}
          attachments={props.d.attachments}
          onPreview={props.onPreview}
        />
      </Show>
    </>
  );
}

// Collapsible earlier message in a conversation thread (Roadmap 3.2)
function CollapsedThreadItem(props: {
  item: MessageDetail;
  allowImages: boolean;
  onLoadImages: () => void;
  onAlwaysTrustSender?: () => void;
  onOpenLink: (url: string) => void;
  onPreview: (attachment: Attachment) => void;
}) {
  const [expanded, setExpanded] = createSignal(false);

  return (
    <div
      class="thread-item"
      classList={{ "thread-item--expanded": expanded() }}
    >
      <button
        type="button"
        class="thread-item__header"
        onClick={() => setExpanded(!expanded())}
      >
        <span class="thread-item__sender">{props.item.row.sender_name}</span>
        <span class="thread-item__snippet">{props.item.row.snippet}</span>
        <span class="thread-item__date tabular">
          {formatRelativeTime(props.item.row.received_at_ms)}
        </span>
      </button>
      <Show when={expanded()}>
        <div class="thread-item__body">
          <div class="thread-item__full-date">
            {formatFullDate(props.item.row.received_at_ms)} · to{" "}
            {props.item.to.map((t) => t.name).join(", ")}
          </div>
          <MessageBody
            d={props.item}
            allowImages={props.allowImages}
            onLoadImages={props.onLoadImages}
            onAlwaysTrustSender={props.onAlwaysTrustSender}
            onOpenLink={props.onOpenLink}
            onPreview={props.onPreview}
          />
        </div>
      </Show>
    </div>
  );
}

// Loading screen (Epic 7.2) — shown while the reading pane fetches a message
// body on demand. The bar is determinate once Rust reports a total (byte
// progress streamed during the download) and indeterminate for the
// connect/parse phases where no byte count exists yet.
function ReadingLoading(props: {
  progress: MessageProgressUpdate | null;
  subject: string | null;
}) {
  const phase = () => props.progress?.phase;
  const pct = () => {
    const p = props.progress;
    if (!p) return null;
    // "parsing" events carry no byte counters — the download just finished, so
    // keep the bar pinned at 100% instead of flickering back to indeterminate.
    if (p.phase === "parsing") return 100;
    if (p.total_bytes === 0) return null;
    return Math.min(100, Math.round((p.received_bytes / p.total_bytes) * 100));
  };
  const label = () => {
    switch (phase()) {
      case "connecting":
        return "Connecting to server…";
      case "fetching":
        return pct() != null
          ? `Downloading message… ${pct()}%`
          : "Downloading message…";
      case "parsing":
        return "Preparing message…";
      default:
        return "Loading message…";
    }
  };
  return (
    <div class="reading-loading" role="status" aria-live="polite">
      <div class="reading-loading__card">
        <Show when={props.subject}>
          <p
            class="reading-loading__subject"
            title={props.subject ?? undefined}
          >
            {props.subject}
          </p>
        </Show>
        <span class="reading-loading__spinner" aria-hidden="true" />
        <div
          class="reading-loading__bar"
          classList={{
            "reading-loading__bar--indeterminate": pct() == null,
          }}
        >
          <div
            class="reading-loading__fill"
            style={{ width: pct() != null ? `${pct()}%` : undefined }}
          />
        </div>
        <p class="reading-loading__label">{label()}</p>
      </div>
    </div>
  );
}

// The reading pane (Epic 7) and its focused single-column variant (Epic 8).
// In focused mode the sidebar and list are hidden by the workspace, this pane
// fills the window, and a "← Inbox" header strip (8.1) replaces the three-pane
// header. The body is shared via <MessageBody/>.
export function ReadingPane(props: { focused?: boolean }) {
  const focused = () => props.focused ?? false;
  const theme = useTheme();
  const detail = useDetail();
  const selectedId = useSelectedId();
  const detailLoading = useDetailLoading();
  const detailProgress = useDetailProgress();
  const rows = useRows();
  const accounts = useAccounts();
  const settings = useSettings();
  const [pendingLink, setPendingLink] = createSignal<{ url: string } | null>(
    null,
  );
  const [threadMessages, setThreadMessages] = createSignal<MessageDetail[]>([]);
  const [previewAttachment, setPreviewAttachment] =
    createSignal<Attachment | null>(null);
  const [unsubConfirmOpen, setUnsubConfirmOpen] = createSignal(false);
  const [unsubToast, setUnsubToast] = createSignal<string | null>(null);
  const [isUnsubscribing, setIsUnsubscribing] = createSignal(false);
  const [sessionAllowedImages, setSessionAllowedImages] = createSignal<
    Set<number>
  >(new Set());

  // Fetch the full body only when a message is selected (Epic 3.3).
  createEffect(() => {
    const id = selectedId();
    if (id != null) void loadDetail(id);
  });

  // Fetch all thread messages when a message is selected in threaded mode (Roadmap 3.2)
  createEffect(() => {
    const d = detail();
    const isThreaded = settings()?.conversationThreading ?? true;
    if (d && d.thread_id && isThreaded) {
      const currentThreadId = d.thread_id;
      void getThreadMessages(d.row.account_id, d.thread_id).then((msgs) => {
        // Guard: discard if thread changed during fetch
        if (detail()?.thread_id === currentThreadId) {
          setThreadMessages(msgs);
        }
      });
    } else {
      setThreadMessages([]);
    }
  });

  const accountAddress = (id: number) =>
    accounts().find((a) => a.id === id)?.address ?? "";

  const recipientsLine = (d: MessageDetail) => {
    const others = d.to
      .filter((r) => r.name !== "me")
      .map((r) => r.name)
      .join(", ");
    const sep = theme() === "banded" ? " · " : " — ";
    return `to me${others ? `, ${others}` : ""}${sep}via ${accountAddress(d.row.account_id)}`;
  };

  // Per-sender remote-image trust (Epic 7.3 / Roadmap 3.7).
  const allowImagesFor = (d: MessageDetail) => {
    if (settings()?.blockRemoteImages === false) return true;
    if ((settings()?.trustedImageSenders ?? []).includes(d.row.sender_address))
      return true;
    return sessionAllowedImages().has(d.row.id);
  };

  const loadImagesOnceFor = (d: MessageDetail) => {
    setSessionAllowedImages((prev) => new Set(prev).add(d.row.id));
  };

  const alwaysTrustSender = (address: string) => {
    const trusted = settings()?.trustedImageSenders ?? [];
    if (!trusted.includes(address)) {
      void updateSettings({
        trustedImageSenders: [...trusted, address],
      });
    }
  };

  const handleUnsubscribe = async (messageId: number) => {
    setIsUnsubscribing(true);
    try {
      const res = await unsubscribe(messageId);
      setUnsubConfirmOpen(false);
      setUnsubToast(res);
      setTimeout(() => setUnsubToast(null), 4000);
    } catch (err) {
      setUnsubConfirmOpen(false);
      setUnsubToast(`Unsubscribe failed: ${err}`);
      setTimeout(() => setUnsubToast(null), 4000);
    } finally {
      setIsUnsubscribing(false);
    }
  };

  const confirmOpenLink = () => {
    const link = pendingLink();
    if (link) void openUrl(link.url);
    setPendingLink(null);
  };

  const earlierThreadMessages = () => {
    const d = detail();
    if (!d) return [];
    return threadMessages().filter((m) => m.row.id !== d.row.id);
  };

  // Loading gate (Epic 7.2): while the selected message's body is being
  // fetched, drop the stale detail (the previously selected message) and show
  // the loading screen instead. Progress events carry the message id they
  // belong to, so a leftover from an earlier selection is ignored.
  const selectedSubject = () => {
    const id = selectedId();
    return id != null ? (rows.find((r) => r.id === id)?.subject ?? null) : null;
  };
  const isLoading = () => {
    const id = selectedId();
    return id != null && detailLoading() && detail()?.row.id !== id;
  };
  const progress = () =>
    detailProgress()?.message_id === selectedId() ? detailProgress() : null;

  return (
    <section
      class="reading-pane"
      classList={{ "reading-pane--focused": focused() }}
      aria-label="Reading pane"
    >
      <Show
        when={!isLoading()}
        fallback={
          <ReadingLoading progress={progress()} subject={selectedSubject()} />
        }
      >
        <Show
          when={detail()}
          keyed
          fallback={
            <div class="reading-empty">Select a message to read it</div>
          }
        >
          {(d) => (
            <>
              <Show when={focused()}>
                {/* Focused header strip (8.1) */}
                <div class="focused-header">
                  <button
                    type="button"
                    class="focused-header__back"
                    onClick={exitFocused}
                  >
                    ← Inbox
                  </button>
                  <span class="focused-header__label">Focused reading</span>
                </div>
                <div class="focused-scroll">
                  <div class="reading-header__title-row">
                    <h1 class="focused-subject">{d.row.subject}</h1>
                    <Show when={d.list_unsubscribe}>
                      <button
                        type="button"
                        class="reading-unsub-btn"
                        onClick={() => setUnsubConfirmOpen(true)}
                        title="Unsubscribe from this mailing list"
                      >
                        ⊘ Unsubscribe
                      </button>
                    </Show>
                  </div>
                  <div class="focused-byline">
                    {d.row.sender_name} · {formatFullDate(d.row.received_at_ms)}
                  </div>
                  <Show when={d.row.answered || d.row.forwarded}>
                    <div class="reading-reply-status reading-reply-status--focused">
                      <Show when={d.row.answered}>
                        <span class="reading-reply-status__tag">
                          <span aria-hidden="true">↩</span> You replied to this
                          message
                        </span>
                      </Show>
                      <Show when={d.row.forwarded}>
                        <span class="reading-reply-status__tag">
                          <span aria-hidden="true">↪</span> You forwarded this
                          message
                        </span>
                      </Show>
                    </div>
                  </Show>

                  {/* Earlier thread messages */}
                  <Show when={earlierThreadMessages().length > 0}>
                    <div class="thread-stack">
                      <span class="thread-stack__title">
                        {earlierThreadMessages().length} earlier message
                        {earlierThreadMessages().length > 1 ? "s" : ""} in
                        thread
                      </span>
                      <For each={earlierThreadMessages()}>
                        {(item) => (
                          <CollapsedThreadItem
                            item={item}
                            allowImages={allowImagesFor(item)}
                            onLoadImages={() => loadImagesOnceFor(item)}
                            onAlwaysTrustSender={() =>
                              alwaysTrustSender(item.row.sender_address)
                            }
                            onOpenLink={(url) => setPendingLink({ url })}
                            onPreview={(att) => setPreviewAttachment(att)}
                          />
                        )}
                      </For>
                    </div>
                  </Show>

                  <MessageBody
                    d={d}
                    allowImages={allowImagesFor(d)}
                    onLoadImages={() => loadImagesOnceFor(d)}
                    onAlwaysTrustSender={() =>
                      alwaysTrustSender(d.row.sender_address)
                    }
                    onOpenLink={(url) => setPendingLink({ url })}
                    onPreview={(att) => setPreviewAttachment(att)}
                  />
                </div>
              </Show>

              <Show when={!focused()}>
                <header class="reading-header">
                  <div class="reading-header__title-row">
                    <h1 class="reading-subject">{d.row.subject}</h1>
                    <Show when={d.list_unsubscribe}>
                      <button
                        type="button"
                        class="reading-unsub-btn"
                        onClick={() => setUnsubConfirmOpen(true)}
                        title="Unsubscribe from this mailing list"
                      >
                        ⊘ Unsubscribe
                      </button>
                    </Show>
                  </div>
                  {theme() === "banded" ? (
                    <div class="reading-meta">
                      <span class="reading-avatar" aria-hidden="true">
                        {avatarInitials(d.row.sender_name)}
                      </span>
                      <span class="reading-meta__text">
                        <span class="reading-sender">{d.row.sender_name}</span>
                        <span class="reading-recipients">
                          {recipientsLine(d)}
                        </span>
                      </span>
                      <span class="reading-date">
                        {formatFullDate(d.row.received_at_ms)}
                      </span>
                    </div>
                  ) : (
                    <>
                      <div class="reading-meta">
                        <span class="reading-sender">{d.row.sender_name}</span>
                        <span class="reading-address">
                          {d.row.sender_address}
                        </span>
                        <span class="reading-date">
                          {formatFullDate(d.row.received_at_ms)}
                        </span>
                      </div>
                      <div class="reading-recipients-line">
                        {recipientsLine(d)}
                      </div>
                    </>
                  )}
                  <Show when={d.row.answered || d.row.forwarded}>
                    <div class="reading-reply-status">
                      <Show when={d.row.answered}>
                        <span class="reading-reply-status__tag">
                          <span aria-hidden="true">↩</span> You replied to this
                          message
                        </span>
                      </Show>
                      <Show when={d.row.forwarded}>
                        <span class="reading-reply-status__tag">
                          <span aria-hidden="true">↪</span> You forwarded this
                          message
                        </span>
                      </Show>
                    </div>
                  </Show>
                </header>

                <div class="reading-body">
                  {/* Earlier thread messages */}
                  <Show when={earlierThreadMessages().length > 0}>
                    <div class="thread-stack">
                      <span class="thread-stack__title">
                        {earlierThreadMessages().length} earlier message
                        {earlierThreadMessages().length > 1 ? "s" : ""} in
                        thread
                      </span>
                      <For each={earlierThreadMessages()}>
                        {(item) => (
                          <CollapsedThreadItem
                            item={item}
                            allowImages={allowImagesFor(item)}
                            onLoadImages={() => loadImagesOnceFor(item)}
                            onAlwaysTrustSender={() =>
                              alwaysTrustSender(item.row.sender_address)
                            }
                            onOpenLink={(url) => setPendingLink({ url })}
                            onPreview={(att) => setPreviewAttachment(att)}
                          />
                        )}
                      </For>
                    </div>
                  </Show>

                  <MessageBody
                    d={d}
                    allowImages={allowImagesFor(d)}
                    onLoadImages={() => loadImagesOnceFor(d)}
                    onAlwaysTrustSender={() =>
                      alwaysTrustSender(d.row.sender_address)
                    }
                    onOpenLink={(url) => setPendingLink({ url })}
                    onPreview={(att) => setPreviewAttachment(att)}
                  />
                </div>
              </Show>

              <Show when={previewAttachment()}>
                <QuickPreviewModal
                  attachment={previewAttachment()!}
                  onClose={() => setPreviewAttachment(null)}
                />
              </Show>

              <Show when={unsubConfirmOpen()}>
                <Modal
                  title="Unsubscribe from Mailing List"
                  onClose={() => setUnsubConfirmOpen(false)}
                >
                  <div class="unsub-dialog">
                    <p class="unsub-dialog__msg">
                      Are you sure you want to unsubscribe from emails from{" "}
                      <strong>
                        {d.row.sender_name || d.row.sender_address}
                      </strong>
                      ?
                    </p>
                    <Show when={d.list_unsubscribe_post}>
                      <p class="unsub-dialog__msg">
                        Quill will send an automated RFC 8058 One-Click POST
                        unsubscribe request to the sender's endpoint.
                      </p>
                    </Show>
                    <div class="unsub-dialog__details">
                      {d.list_unsubscribe}
                    </div>
                    <div class="unsub-dialog__footer">
                      <button
                        type="button"
                        class="btn btn--secondary"
                        onClick={() => setUnsubConfirmOpen(false)}
                        disabled={isUnsubscribing()}
                      >
                        Cancel
                      </button>
                      <button
                        type="button"
                        class="btn btn--primary"
                        onClick={() => void handleUnsubscribe(d.row.id)}
                        disabled={isUnsubscribing()}
                      >
                        {isUnsubscribing() ? "Unsubscribing..." : "Unsubscribe"}
                      </button>
                    </div>
                  </div>
                </Modal>
              </Show>

              <Show when={unsubToast()}>
                <div class="reading-toast" role="status">
                  {unsubToast()}
                </div>
              </Show>

              <Show when={pendingLink()}>
                <div class="link-confirm" role="dialog" aria-label="Open link">
                  <span class="link-confirm__url">{pendingLink()?.url}</span>
                  <button
                    type="button"
                    class="btn btn--secondary"
                    onClick={() => setPendingLink(null)}
                  >
                    Cancel
                  </button>
                  <button
                    type="button"
                    class="btn btn--primary"
                    onClick={confirmOpenLink}
                  >
                    Open
                  </button>
                </div>
              </Show>

              <footer class="reading-actions">
                <div class="reading-actions__left">
                  <button
                    type="button"
                    class="btn btn--primary"
                    onClick={() => requestCompose("reply")}
                  >
                    Reply
                  </button>
                  <button
                    type="button"
                    class="btn btn--secondary"
                    onClick={() => requestCompose("replyAll")}
                  >
                    Reply all
                  </button>
                  <button
                    type="button"
                    class="btn btn--secondary"
                    onClick={() => requestCompose("forward")}
                  >
                    Forward
                  </button>
                  <button
                    type="button"
                    class="btn btn--secondary"
                    onClick={() =>
                      void markMessageJunk(d.row.id, d.row.folder !== "Junk")
                    }
                    title={
                      d.row.folder === "Junk"
                        ? "Move message back to Inbox"
                        : "Mark message as Junk"
                    }
                  >
                    {d.row.folder === "Junk" ? "Not Junk" : "Junk"}
                  </button>
                  <SnoozeMenu
                    onSnooze={(untilMs) => void snooze([d.row.id], untilMs)}
                  />
                  <button
                    type="button"
                    class="btn btn--secondary"
                    onClick={() => window.print()}
                    title="Print this message (⌘P)"
                  >
                    Print
                  </button>
                  <button
                    type="button"
                    class="btn btn--secondary"
                    onClick={() => void exportEml(d.row.id, d.row.subject)}
                    title="Export this message as .eml"
                  >
                    Export .eml
                  </button>
                  <Show when={theme() === "hairline"}>
                    <span class="reading-hint mono">r · a · f</span>
                  </Show>
                </div>

                {/* Thread-level bulk actions */}
                <Show
                  when={
                    d.thread_id && (settings()?.conversationThreading ?? true)
                  }
                >
                  <div class="reading-actions__thread">
                    <button
                      type="button"
                      class="btn btn--secondary btn--sm"
                      title="Archive whole thread"
                      onClick={() =>
                        d.thread_id &&
                        void performThreadAction(
                          d.row.account_id,
                          d.thread_id,
                          "archive",
                        )
                      }
                    >
                      Archive thread
                    </button>
                    <button
                      type="button"
                      class="btn btn--secondary btn--sm"
                      title="Delete whole thread"
                      onClick={() =>
                        d.thread_id &&
                        void performThreadAction(
                          d.row.account_id,
                          d.thread_id,
                          "delete",
                        )
                      }
                    >
                      Delete thread
                    </button>
                  </div>
                </Show>
              </footer>
            </>
          )}
        </Show>
      </Show>
    </section>
  );
}
