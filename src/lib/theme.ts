import { createSignal } from "solid-js";

/** The two shipped treatments (Epic 2). A third requires only a new
 * `[data-theme]` block in tokens.css — see scripts/check-token-usage.mjs. */
export type ThemeName = "hairline" | "banded";

export const THEMES: readonly ThemeName[] = ["hairline", "banded"];

export function isThemeName(value: unknown): value is ThemeName {
  return value === "hairline" || value === "banded";
}

// The single DOM read of data-theme: the Rust side injects an initialization
// script that sets it before first paint (so there is no flash of the wrong
// theme), and the signal seeds from it synchronously. Components never read
// data-theme themselves — this module owns the attribute.
const initial = isThemeName(document.documentElement.dataset.theme)
  ? document.documentElement.dataset.theme
  : "hairline";

const [theme, setTheme] = createSignal<ThemeName>(initial);

/** Reactive theme accessor. Components branch on `theme() === "banded"` here,
 * in one place per component — never by reading data-theme from the DOM
 * (Epic 2.2). */
export function useTheme(): () => ThemeName {
  return theme;
}

/** Swap the treatment: one root attribute change; no remount, no reload, no
 * flash. All app state survives because nothing is torn down. */
export function applyTheme(next: ThemeName): void {
  setTheme(next);
  document.documentElement.dataset.theme = next;
}

// P1.5 dark palettes: a `data-color-scheme` attribute flips the color tokens
// for BOTH treatments (the `[data-color-scheme="dark"]` override block in
// tokens.css). Seeded from the Rust init script (no flash); persisted so it
// survives restart.
const darkInitial =
  document.documentElement.dataset.colorScheme === "dark" ||
  (() => {
    try {
      return localStorage.getItem("quill_dark") === "1";
    } catch {
      return false;
    }
  })();

const [dark, setDark] = createSignal(darkInitial);

/** Reactive dark-mode accessor. */
export function useDark(): () => boolean {
  return dark;
}

export function applyDark(next: boolean): void {
  setDark(next);
  document.documentElement.dataset.colorScheme = next ? "dark" : "light";
  try {
    localStorage.setItem("quill_dark", next ? "1" : "0");
  } catch {
    /* ignore */
  }
}

export function toggleDark(): void {
  applyDark(!dark());
}
