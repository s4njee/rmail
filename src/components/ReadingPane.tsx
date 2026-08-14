import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import { createEffect, createSignal, For, Show } from "solid-js";
import { requestCompose } from "../lib/compose";
import { avatarInitials, formatBytes, formatFullDate } from "../lib/format";
import type { Attachment } from "../lib/ipc/Attachment";
import type { MessageDetail } from "../lib/ipc/MessageDetail";
import { useAccounts, useDetail, useSelectedId, loadDetail } from "../lib/mail";
import { useSettings, updateSettings } from "../lib/settings";
import { useTheme } from "../lib/theme";
import { attachmentPath } from "../lib/tauri";
import { exitFocused } from "../lib/ui";
import { MailBody } from "./MailBody";
import "./ReadingPane.css";

function AttachmentCard(props: { attachment: Attachment }) {
  const open = () => {
    void attachmentPath(props.attachment.id).then((path) => {
      if (path) void openPath(path);
    });
  };

  return (
    <button type="button" class="attachment-card" onClick={open}>
      <span class="attachment-thumb" aria-hidden="true" />
      <span class="attachment-info">
        <span class="attachment-name">{props.attachment.filename}</span>
        <span class="attachment-meta">
          {props.attachment.on_disk
            ? `${formatBytes(props.attachment.size_bytes)} · cached locally`
            : formatBytes(props.attachment.size_bytes)}
        </span>
      </span>
    </button>
  );
}

// The message body — HTML via the sandboxed iframe (7.3) or plain paragraphs —
// plus attachment cards. Shared by the three-pane and focused layouts.
function MessageBody(props: {
  d: MessageDetail;
  allowImages: boolean;
  onLoadImages: () => void;
  onOpenLink: (url: string) => void;
}) {
  return (
    <>
      {props.d.body_html ? (
        <MailBody
          detail={props.d}
          allowImages={props.allowImages}
          onLoadImages={props.onLoadImages}
          onOpenLink={props.onOpenLink}
        />
      ) : (
        <For each={props.d.body}>
          {(paragraph) => <p class="reading-paragraph">{paragraph}</p>}
        </For>
      )}
      <For each={props.d.attachments}>
        {(attachment) => <AttachmentCard attachment={attachment} />}
      </For>
    </>
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
  const accounts = useAccounts();
  const settings = useSettings();
  const [pendingLink, setPendingLink] = createSignal<{ url: string } | null>(
    null,
  );

  // Fetch the full body only when a message is selected (Epic 3.3).
  createEffect(() => {
    const id = selectedId();
    if (id != null) void loadDetail(id);
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

  // Per-sender remote-image trust (Epic 7.3) — never global-only.
  const allowImagesFor = (d: MessageDetail) =>
    (settings()?.trustedImageSenders ?? []).includes(d.row.sender_address);

  const loadImagesFor = (d: MessageDetail) => {
    const trusted = settings()?.trustedImageSenders ?? [];
    void updateSettings({
      trustedImageSenders: [...trusted, d.row.sender_address],
    });
  };

  const confirmOpenLink = () => {
    const link = pendingLink();
    if (link) void openUrl(link.url);
    setPendingLink(null);
  };

  return (
    <section
      class="reading-pane"
      classList={{ "reading-pane--focused": focused() }}
      aria-label="Reading pane"
    >
      <Show
        when={detail()}
        keyed
        fallback={<div class="reading-empty">Select a message to read it</div>}
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
                <h1 class="focused-subject">{d.row.subject}</h1>
                <div class="focused-byline">
                  {d.row.sender_name} · {formatFullDate(d.row.received_at_ms)}
                </div>
                <MessageBody
                  d={d}
                  allowImages={allowImagesFor(d)}
                  onLoadImages={() => loadImagesFor(d)}
                  onOpenLink={(url) => setPendingLink({ url })}
                />
              </div>
            </Show>

            <Show when={!focused()}>
              <header class="reading-header">
                <h1 class="reading-subject">{d.row.subject}</h1>
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
              </header>

              <div class="reading-body">
                <MessageBody
                  d={d}
                  allowImages={allowImagesFor(d)}
                  onLoadImages={() => loadImagesFor(d)}
                  onOpenLink={(url) => setPendingLink({ url })}
                />
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
              <Show when={theme() === "hairline"}>
                <span class="reading-hint mono">r · a · f</span>
              </Show>
            </footer>
          </>
        )}
      </Show>
    </section>
  );
}
