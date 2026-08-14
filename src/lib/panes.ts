import { createSignal } from "solid-js";
import type { ThemeName } from "./theme";
import { getSettings, patchSettings } from "./tauri";

// Pane widths (Epic 4.2). Defaults mirror --sidebar-w / --list-w in
// tokens.css; a drag persists the widths to the Rust settings store
// (AppSettings.sidebarWidth / listWidth). `null` = use the theme default.
export const SIDEBAR_MIN = 180;
export const SIDEBAR_MAX = 420;
export const LIST_MIN = 300;
export const LIST_MAX = 560;

// Keep in sync with tokens.css (`--sidebar-w` / `--list-w`). The clamp math
// needs numeric values; reading computed style on every drag is not worth it.
const THEME_SIDEBAR_W: Record<ThemeName, number> = {
  hairline: 220,
  banded: 232,
};
const THEME_LIST_W: Record<ThemeName, number> = {
  hairline: 372,
  banded: 384,
};

const [sidebarWidth, setSidebarWidth] = createSignal<number | null>(null);
const [listWidth, setListWidth] = createSignal<number | null>(null);

const clamp = (value: number, min: number, max: number) =>
  Math.min(Math.max(value, min), max);

/** Effective width (px): the persisted width, or the theme default. Call from
 * JSX (a tracking scope) so a resize or theme switch re-renders the pane. */
export function effectiveSidebarWidth(theme: () => ThemeName): number {
  return sidebarWidth() ?? THEME_SIDEBAR_W[theme()];
}

export function effectiveListWidth(theme: () => ThemeName): number {
  return listWidth() ?? THEME_LIST_W[theme()];
}

/** Apply a horizontal drag delta (positive = right) to the pane left of the
 * divider, clamped to usable bounds. */
export function resizeSidebar(theme: () => ThemeName, delta: number): void {
  setSidebarWidth(
    clamp(effectiveSidebarWidth(theme) + delta, SIDEBAR_MIN, SIDEBAR_MAX),
  );
}

export function resizeList(theme: () => ThemeName, delta: number): void {
  setListWidth(clamp(effectiveListWidth(theme) + delta, LIST_MIN, LIST_MAX));
}

/** Persist the widths on drag end. */
export function persistPaneWidths(): void {
  void patchSettings({ sidebarWidth: sidebarWidth(), listWidth: listWidth() });
}

/** Load persisted widths at startup (no-op when unset). */
export async function initPanes(): Promise<void> {
  const settings = await getSettings();
  setSidebarWidth(settings.sidebarWidth);
  setListWidth(settings.listWidth);
}
