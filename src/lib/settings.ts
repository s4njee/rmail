import { createSignal } from "solid-js";
import type { AppSettings } from "./ipc/AppSettings";
import { getSettings, patchSettings } from "./tauri";

// App settings surfaced to components (theme lives in theme.ts; this store
// carries the rest — pane widths aren't needed here, per-sender image trust
// is). Loaded once at startup and patched read-modify-write.

const [settings, setSettings] = createSignal<AppSettings | null>(null);

export function useSettings(): () => AppSettings | null {
  return settings;
}

export async function initSettings(): Promise<void> {
  setSettings(await getSettings());
}

/** Patch a few settings and reflect the change locally. */
export async function updateSettings(
  patch: Partial<AppSettings>,
): Promise<void> {
  await patchSettings(patch);
  setSettings((current) => (current ? { ...current, ...patch } : current));
}
