import { createSignal } from "solid-js";
import type { QueuedAction } from "./ipc/QueuedAction";
import { listQueuedActions } from "./tauri";

// P0.3: the offline action queue — visible and recoverable in Settings →
// Accounts → "Sync & queue", and flagged in the StatusBar when anything is
// stuck (retries ≥ 5). Refreshed after enqueue/retry/remove.
const [queued, setQueued] = createSignal<QueuedAction[]>([]);

export function useQueued(): () => QueuedAction[] {
  return queued;
}

export async function refreshQueued(): Promise<void> {
  try {
    setQueued(await listQueuedActions());
  } catch {
    /* non-fatal */
  }
}

/** A human label + state badge for a queued action. */
export function actionLabel(a: QueuedAction): { label: string; state: string } {
  const labels: Record<string, string> = {
    markRead: "Mark read",
    markUnread: "Mark unread",
    star: "Star",
    unstar: "Unstar",
    archive: "Archive",
    delete: "Delete",
    move: `Move to ${a.folder}`,
    markJunk: "Mark junk",
    markNotJunk: "Not junk",
    markAnswered: "Mark answered",
    markForwarded: "Mark forwarded",
    send: "Send message",
  };
  const state =
    a.retries === 0 ? "pending" : a.retries >= 5 ? "stuck" : "retrying";
  return { label: labels[a.action_type] ?? a.action_type, state };
}
