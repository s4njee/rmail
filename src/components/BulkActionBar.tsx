import { createSignal, For, Show } from "solid-js";
import type { BulkActionResult } from "../lib/ipc/BulkActionResult";
import type { Folder } from "../lib/ipc/Folder";
import {
  clearMultiSelect,
  snooze,
  triage,
  triageIds,
  useMultiSelectedIds,
} from "../lib/mail";
import { SnoozeMenu } from "./SnoozeMenu";
import "./BulkActionBar.css";

// The multi-select triage bar (P1.1): shown over the message list when >1
// messages are selected. Runs a bulk action with partial-failure reporting.
// The "Move" menu lists real destination folders (the derived Starred view
// isn't a mailbox).
function MoveMenu(props: {
  folders: Folder[];
  disabled: boolean;
  onMove: (folder: string) => void;
}) {
  const [open, setOpen] = createSignal(false);
  const destinations = () =>
    props.folders.filter((f) => f.name !== "Starred" && f.name !== "Snoozed");
  return (
    <div class="bulk-move">
      <button
        type="button"
        class="btn btn--secondary btn--sm"
        disabled={props.disabled}
        onClick={() => setOpen(!open())}
        aria-haspopup="menu"
        aria-expanded={open()}
      >
        Move…
      </button>
      <Show when={open()}>
        <div
          class="bulk-move__menu"
          role="menu"
          onMouseDown={(e) => e.stopPropagation()}
        >
          <For each={destinations()}>
            {(folder) => (
              <button
                type="button"
                role="menuitem"
                class="bulk-move__item"
                onClick={() => {
                  setOpen(false);
                  props.onMove(folder.name);
                }}
              >
                {folder.name}
              </button>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}

export function BulkActionBar(props: { folders: Folder[] }) {
  const multi = useMultiSelectedIds();
  const [running, setRunning] = createSignal(false);
  const [status, setStatus] = createSignal<BulkActionResult | null>(null);

  const count = () => multi().size;
  const busy = () => running() || count() === 0;

  const run = async (
    action:
      | "markRead"
      | "markUnread"
      | "star"
      | "unstar"
      | "archive"
      | "delete"
      | "markJunk"
      | "move",
    destination?: string,
  ) => {
    const ids = triageIds();
    if (ids.length === 0) return;
    setRunning(true);
    setStatus(null);
    try {
      const result = await triage(ids, action, destination);
      setStatus(result);
    } finally {
      setRunning(false);
    }
  };

  const runSnooze = async (untilMs: number) => {
    const ids = triageIds();
    if (ids.length === 0) return;
    setRunning(true);
    setStatus(null);
    try {
      await snooze(ids, untilMs);
      setStatus({ ok: ids.length, failed: 0, errors: [] });
    } finally {
      setRunning(false);
    }
  };

  return (
    <div class="bulk-bar" role="toolbar" aria-label="Bulk actions">
      <span class="bulk-bar__count">
        {count()} selected
        {running() ? "…" : ""}
      </span>
      <div class="bulk-bar__actions">
        <button
          type="button"
          class="btn btn--secondary btn--sm"
          disabled={busy()}
          onClick={() => void run("markRead")}
        >
          Read
        </button>
        <button
          type="button"
          class="btn btn--secondary btn--sm"
          disabled={busy()}
          onClick={() => void run("markUnread")}
        >
          Unread
        </button>
        <button
          type="button"
          class="btn btn--secondary btn--sm"
          disabled={busy()}
          onClick={() => void run("star")}
        >
          Star
        </button>
        <button
          type="button"
          class="btn btn--secondary btn--sm"
          disabled={busy()}
          onClick={() => void run("archive")}
        >
          Archive
        </button>
        <button
          type="button"
          class="btn btn--secondary btn--sm"
          disabled={busy()}
          onClick={() => void run("delete")}
        >
          Delete
        </button>
        <button
          type="button"
          class="btn btn--secondary btn--sm"
          disabled={busy()}
          onClick={() => void run("markJunk")}
        >
          Junk
        </button>
        <MoveMenu
          folders={props.folders}
          disabled={busy()}
          onMove={(f) => void run("move", f)}
        />
        <SnoozeMenu onSnooze={runSnooze} disabled={busy()} />
      </div>
      <button
        type="button"
        class="bulk-bar__clear"
        onClick={clearMultiSelect}
        disabled={running()}
      >
        Clear
      </button>
      <Show when={status()}>
        {(s) => (
          <span class="bulk-bar__status" role="status" aria-live="polite">
            {s().failed > 0
              ? `${s().ok} done, ${s().failed} failed`
              : `${s().ok} done`}
            <Show when={s().errors.length > 0}>
              <span class="bulk-bar__errors">
                {s().errors.slice(0, 3).join(" · ")}
              </span>
            </Show>
          </span>
        )}
      </Show>
    </div>
  );
}
