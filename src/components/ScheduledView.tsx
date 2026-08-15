import { createSignal, For, onMount, Show } from "solid-js";
import { reopenComposerFromSnapshot } from "../lib/compose";
import { formatRelativeTime } from "../lib/format";
import type { ScheduledMessage } from "../lib/ipc/ScheduledMessage";
import { cancelScheduled, listScheduled } from "../lib/tauri";
import { Modal } from "./Modal";
import "../components/Settings.css";
import "./ScheduledView.css";

// P1.1 send-later list (the durable Outbox): shows each scheduled send with
// Edit (reopens the composer from its snapshot — the old schedule is dropped
// so a re-schedule replaces it) and Cancel. The flush happens on the app's
// housekeeping loop, so the app must be running at the send time.
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
    await cancelScheduled(id);
    await reload();
  };

  const edit = (m: ScheduledMessage) => {
    reopenComposerFromSnapshot(m.draft);
    void cancel(m.id); // the re-scheduled send replaces this one
    props.onClose();
  };

  return (
    <Modal title="Scheduled messages" onClose={props.onClose}>
      <p class="scheduled-note" role="status">
        Quill must be running when a scheduled message is due — it isn't sent
        otherwise.
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
                Cancel
              </button>
            </div>
          )}
        </For>
        <Show when={rows().length === 0}>
          <p class="scheduled-empty">No scheduled messages.</p>
        </Show>
      </div>
    </Modal>
  );
}
