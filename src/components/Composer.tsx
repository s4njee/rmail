import { For, Show } from "solid-js";
import {
  addComposerAttachment,
  closeComposer,
  discardComposer,
  persistDraft,
  removeComposerAttachment,
  sendComposer,
  updateDraft,
  useComposer,
} from "../lib/compose";
import { formatBytes } from "../lib/format";
import { Modal } from "./Modal";
import "./Composer.css";

// The composer (Epic 13), opened as a modal by Reply / Reply all / Forward
// (buttons + r/a/f). Plain-text editing; drafts autosave to the Drafts folder
// within 2s of typing stopping (13.2). The form is extrapolated from the
// tokens and flagged for design review.
export function Composer() {
  const { open, draft, attachments, sendError } = useComposer();
  const d = () => draft();

  const onClose = () => {
    // Closing with content saves it as a draft (13.2).
    const current = d();
    if (current && (current.subject || current.body || current.to.length > 0)) {
      void persistDraft();
    }
    closeComposer();
  };

  const handleFiles = (files: FileList | null) => {
    for (const file of files ?? []) {
      addComposerAttachment({ name: file.name, size: file.size });
    }
  };

  const title = () =>
    d()?.intent === "forward"
      ? "Forward"
      : d()?.intent === "replyAll"
        ? "Reply all"
        : "Reply";

  return (
    <Show when={open() && d()}>
      <Modal title={title()} onClose={onClose}>
        <form
          class="composer"
          onSubmit={(event) => {
            event.preventDefault();
            void sendComposer();
          }}
          onDragOver={(event) => event.preventDefault()}
          onDrop={(event) => {
            event.preventDefault();
            handleFiles(event.dataTransfer?.files ?? null);
          }}
        >
          <label class="composer__field">
            <span>To</span>
            <input
              type="text"
              value={d()!.to.join(", ")}
              onInput={(e) =>
                updateDraft({
                  to: e.currentTarget.value
                    .split(",")
                    .map((s) => s.trim())
                    .filter(Boolean),
                })
              }
              placeholder="recipient@example.com"
            />
          </label>
          <label class="composer__field">
            <span>Cc</span>
            <input
              type="text"
              value={d()!.cc.join(", ")}
              onInput={(e) =>
                updateDraft({
                  cc: e.currentTarget.value
                    .split(",")
                    .map((s) => s.trim())
                    .filter(Boolean),
                })
              }
              placeholder="cc@example.com"
            />
          </label>
          <label class="composer__field">
            <span>Subject</span>
            <input
              type="text"
              value={d()!.subject}
              onInput={(e) => updateDraft({ subject: e.currentTarget.value })}
            />
          </label>
          <textarea
            class="composer__body"
            value={d()!.body}
            onInput={(e) => updateDraft({ body: e.currentTarget.value })}
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
                    >
                      Remove
                    </button>
                  </li>
                )}
              </For>
            </ul>
          </Show>

          <Show when={sendError()}>
            <div class="composer__error">{sendError()}</div>
          </Show>

          <div class="composer__actions">
            <label class="composer__attach">
              Attach files
              <input
                type="file"
                multiple
                hidden
                onChange={(event) => handleFiles(event.currentTarget.files)}
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
            <button type="submit" class="btn btn--primary">
              Send
            </button>
          </div>
        </form>
      </Modal>
    </Show>
  );
}
