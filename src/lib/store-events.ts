import { createSignal } from "solid-js";
import { openMailto } from "./compose";
import { formatClock } from "./format";
import type { ConnectivityUpdate } from "./ipc/ConnectivityUpdate";
import type { SearchIndexUpdate } from "./ipc/SearchIndexUpdate";
import { loadRows, refreshMail, setDetailProgress } from "./mail";
import { refreshQueued } from "./queue";
import { getFootprint, onStoreEvent } from "./tauri";

// Push events from Rust (Epic 3.2) — the frontend never polls. Connectivity
// and footprint arrive as deltas; the real sources land in Epics 11/12 (for
// now the demo pushes a "synced" update and the footprint shortly after
// launch).

const initial: ConnectivityUpdate = {
  state: "offline",
  last_synced_at_ms: null,
};
const [connectivity, setConnectivity] =
  createSignal<ConnectivityUpdate>(initial);
const [footprintBytes, setFootprintBytes] = createSignal<number>(0);
const [searchIndex, setSearchIndex] = createSignal<SearchIndexUpdate | null>(
  null,
);

/** Reactive connectivity state for the status readouts (Epic 4.3 / 11.1). */
export function useConnectivity(): () => ConnectivityUpdate {
  return connectivity;
}

/** Human text for a connectivity state — `Synced 14:03`, `Offline — synced
 * 11:42`, `Syncing…`. Shown in the sidebar footer. */
export function connectivityText(c: ConnectivityUpdate): string {
  if (c.state === "syncing") return "Syncing…";
  const clock =
    c.last_synced_at_ms != null ? formatClock(c.last_synced_at_ms) : "";
  return c.state === "synced" ? `Synced ${clock}` : `Offline — synced ${clock}`;
}

/** Reactive on-disk cache size, used by the footprint readouts (4.3 / 11.2). */
export function useFootprintBytes(): () => number {
  return footprintBytes;
}

/** Reactive search-index rebuild status (P1.3). */
export function useSearchIndex(): () => SearchIndexUpdate | null {
  return searchIndex;
}

/** Subscribe the app to the store event stream. Call once at startup. */
export function initStoreEvents(): void {
  // Seed the footprint from the store; pushes keep it live.
  void getFootprint().then(setFootprintBytes);
  void onStoreEvent((event) => {
    if (event.kind === "connectivity") {
      setConnectivity(event);
      // A sync cycle finished or connection state changed: refresh folder and
      // account counts so the sidebar and account rows reflect it. Mid-sync
      // progress arrives as `mailChanged` and only touches the message list.
      if (event.state === "synced") {
        void loadRows();
        void refreshMail();
        // P0.3: after a sync attempt, surface any queued failures (a failed
        // replay recorded a last_error the StatusBar can flag).
        void refreshQueued();
      } else if (event.state === "offline") {
        void refreshMail();
      }
    } else if (event.kind === "mailChanged") {
      // A message was streamed in mid-sync — reload so it appears now instead
      // of waiting for the whole sync to finish (the frontend never polls).
      void loadRows();
    } else if (event.kind === "messageProgress") {
      // Body-download progress for the reading pane's loading screen (Epic
      // 7.2). The pane filters by `message_id` against the current selection.
      setDetailProgress(event);
    } else if (event.kind === "footprint") {
      setFootprintBytes(event.on_disk_bytes);
    } else if (event.kind === "searchIndex") {
      setSearchIndex(event);
    } else if (event.kind === "mailto") {
      void openMailto(event);
    }
  });
}
