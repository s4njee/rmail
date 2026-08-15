import { Show } from "solid-js";
import { useCalendarSyncing } from "../lib/calendar";
import { useDetailLoading, useDetailProgress } from "../lib/mail";
import { useConnectivity } from "../lib/store-events";
import "./StatusBar.css";

// Bottom status bar (feature request): appears while the app is connecting to
// the server, downloading a message body, or syncing calendars, and hides when
// idle. Driven entirely by signals the rest of the app already maintains.
export function StatusBar() {
  const connectivity = useConnectivity();
  const detailLoading = useDetailLoading();
  const detailProgress = useDetailProgress();
  const calendarSyncing = useCalendarSyncing();

  const pct = () => {
    const p = detailProgress();
    if (!p) return null;
    if (p.phase === "parsing") return 100;
    if (p.total_bytes === 0) return null;
    return Math.min(100, Math.round((p.received_bytes / p.total_bytes) * 100));
  };

  const label = () => {
    if (calendarSyncing()) return "Syncing calendar…";
    if (detailLoading()) {
      switch (detailProgress()?.phase) {
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
    }
    if (connectivity().state === "syncing") return "Syncing mail…";
    return null;
  };

  return (
    <Show when={label()} keyed>
      {(text) => (
        <div class="status-bar" role="status" aria-live="polite">
          <span class="status-bar__dot" aria-hidden="true" />
          <span class="status-bar__label">{text}</span>
        </div>
      )}
    </Show>
  );
}
