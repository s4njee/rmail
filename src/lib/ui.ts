import { createSignal } from "solid-js";
import { useSelectedId } from "./mail";

// Reading mode (Epic 8): three-pane vs focused single-column. The mode can be
// chosen manually (Enter / Esc / ← Inbox) or forced by the responsive
// breakpoint below ~1100px. A manual choice is respected until the window
// crosses the breakpoint (8.3 — no override within the same width regime).

export type ReadingMode = "three-pane" | "focused";

// Strictly below 1100px: at exactly 1100 the three-pane layout still fits, so
// the persisted 1100×… window must not auto-collapse.
export const NARROW_BREAKPOINT = 1100;

const [narrow, setNarrow] = createSignal(false);
const [manual, setManual] = createSignal<{
  choice: ReadingMode;
  narrow: boolean;
} | null>(null);

/** Reactive effective mode — call inside JSX. */
export function effectiveMode(): ReadingMode {
  const m = manual();
  if (m && m.narrow === narrow()) return m.choice;
  if (!narrow()) return "three-pane";
  // Narrow windows collapse to focused reading, but only when there is a
  // message to read — with nothing selected the panes stay so the user can
  // pick a message first (the collapsed view would otherwise strand them).
  return useSelectedId()() != null ? "focused" : "three-pane";
}

export function enterFocused(): void {
  setManual({ choice: "focused", narrow: narrow() });
}

export function exitFocused(): void {
  setManual({ choice: "three-pane", narrow: narrow() });
}

// Settings view (Epic 10): replaces the workspace. Opened with ⌘,.
const [settingsOpen, setSettingsOpen] = createSignal(false);

export function useSettingsOpen(): () => boolean {
  return settingsOpen;
}

export function openSettings(): void {
  setSettingsOpen(true);
}

export function closeSettings(): void {
  setSettingsOpen(false);
}

// Section: Mail or Calendar (Epic 14.6) — a sidebar-level switch.
export type Section = "mail" | "calendar";

const [section, setSection] = createSignal<Section>("mail");

export function useSection(): () => Section {
  return section;
}

export function switchSection(next: Section): void {
  setSection(next);
}

/** Wire the responsive collapse: below NARROW_BREAKPOINT the reading pane
 * fills the window. Call once at startup. */
export function initResponsive(): void {
  const mq = window.matchMedia(`(max-width: ${NARROW_BREAKPOINT - 1}px)`);
  setNarrow(mq.matches);
  mq.addEventListener("change", (event) => setNarrow(event.matches));
}
