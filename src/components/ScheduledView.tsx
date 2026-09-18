import { createSignal, For, onMount, Show } from "solid-js";
import { reopenComposerFromSnapshot } from "../lib/compose";
import { formatRelativeTime } from "../lib/format";
import type { ScheduledMessage } from "../lib/ipc/ScheduledMessage";
import {
  cancelScheduled,
  listScheduled,
  retryOutboxMessage,
} from "../lib/tauri";
import { Modal } from "./Modal";
import "../components/Settings.css";
import "./ScheduledView.css";

// P1.1/C0.8 Outbox: each row is a durable SMTP submission lifecycle. Sent
// rows are a record, never a source of another SMTP submission.
export function ScheduledView(props: { onClose: () => void }) {
  const [rows, setRows] = createSignal<ScheduledMessage[]>([]);
  const [error, setError] = createSignal("");

  const reload = async () => {
    try {
      setRows(await listScheduled());
    } catch (e) {
      setError(String(e));
    }
  };

  onMount(() => void reload());

  const cancel = async (id: number) => {
    try {
      await cancelScheduled(id);
      await reload();
    } catch (e) {
      setError(String(e));
    }
  };

  const edit = async (m: ScheduledMessage) => {
    try {
      await cancelScheduled(m.id);
      reopenComposerFromSnapshot(m.draft);
      props.onClose();
    } catch (e) {
      setError(String(e));
    }
  };

  const retry = async (id: number) => {
    try {
      await retryOutboxMessage(id);
    } catch (e) {
      setError(String(e));
    } finally {
      await reload();
    }
  };

  const canChange = (m: ScheduledMessage) =>
    m.status === "queued" || m.status === "failed";

  const label = (m: ScheduledMessage) => {
    if (m.status === "failed") return "Failed";
    if (m.status === "sending") return "Sending";
    if (m.status === "sent") return "Sent";
    return m.sendAtMs > Date.now() ? "Scheduled" : "Queued";
  };

  return (
    <Modal title="Outbox" onClose={props.onClose}>
      <p class="scheduled-note" role="status">
        Messages are stored locally before sending. If Quill quits during an
        undo countdown, the message stays in Outbox and sends on next launch.
      </p>
      <Show when={error()}>
        <p class="scheduled-note scheduled-note--error" role="alert">
          {error()}
        </p>
      </Show>
      <div class="scheduled-list">
        <For each={rows()}>
          {(m) => (
            <div class="scheduled-row">
              <div class="scheduled-row__main">
                <span class="scheduled-row__subject">
                  {m.subject || "(no subject)"}
                </span>
                <span class="scheduled-row__to">{m.to.join(", ")}</span>
              </div>
              <span class="scheduled-row__time tabular">
                {formatRelativeTime(m.sendAtMs)}
              </span>
              <span class="scheduled-row__time">{label(m)}</span>
              <Show when={m.status === "failed"}>
                <button
                  type="button"
                  class="btn btn--secondary btn--sm"
                  onClick={() => void retry(m.id)}
                >
                  Retry
                </button>
              </Show>
              <Show when={canChange(m)}>
                <button
                  type="button"
                  class="btn btn--secondary btn--sm"
                  onClick={() => void edit(m)}
                >
                  Edit
                </button>
                <button
                  type="button"
                  class="btn btn--secondary btn--sm"
                  onClick={() => void cancel(m.id)}
                >
                  Discard
                </button>
              </Show>
              <Show when={m.lastError}>
                <p class="scheduled-note scheduled-note--error" role="alert">
                  {m.lastError}
                </p>
              </Show>
            </div>
          )}
        </For>
        <Show when={rows().length === 0}>
          <p class="scheduled-empty">Outbox is empty.</p>
        </Show>
      </div>
    </Modal>
  );
}
