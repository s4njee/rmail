import { createSignal } from "solid-js";
import { formatClock } from "./format";
import type { ConnectivityUpdate } from "./ipc/ConnectivityUpdate";
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

/** Reactive connectivity state for the status readouts (Epic 4.3 / 11.1). */
export function useConnectivity(): () => ConnectivityUpdate {
  return connectivity;
}

/** Human text for a connectivity state — `Synced 14:03`, `Offline — synced
 * 11:42`, `Syncing…`. Shared by the Banded titlebar pill and the Hairline
 * sidebar footer. */
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

/** Subscribe the app to the store event stream. Call once at startup. */
export function initStoreEvents(): void {
  // Seed the footprint from the store; pushes keep it live.
  void getFootprint().then(setFootprintBytes);
  void onStoreEvent((event) => {
    if (event.kind === "connectivity") setConnectivity(event);
    else if (event.kind === "footprint") setFootprintBytes(event.on_disk_bytes);
  });
}
