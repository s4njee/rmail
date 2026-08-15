import { getVersion } from "@tauri-apps/api/app";
import { check } from "@tauri-apps/plugin-updater";
import { createSignal } from "solid-js";
import { isTauri } from "./tauri";

// Auto-update + "What's new" (E2.2). The updater checks the configured endpoint
// on startup and downloads a signed update if one is available; the version
// tracker shows release notes on first launch after an upgrade.

const LAST_SEEN_KEY = "quill_last_seen_version";

/** Release notes per version, shown in the "What's new" dialog after an update. */
const RELEASE_NOTES: Record<string, string[]> = {
  "0.1.0": [
    "Local-first mail for IMAP, Google, and Microsoft accounts",
    "Calendar with month, week, 3-day, day, agenda and year views",
    "Google Calendar sync, with each calendar shown or hidden independently",
    "Keyboard-first: ⌘K search, quick navigation, message list shortcuts",
  ],
};

/** The running app version (mock/browser falls back to the known version). */
export async function currentVersion(): Promise<string> {
  if (isTauri()) {
    try {
      return await getVersion();
    } catch {
      /* fall through to the fallback */
    }
  }
  return "0.1.0";
}

export function lastSeenVersion(): string | null {
  try {
    return localStorage.getItem(LAST_SEEN_KEY);
  } catch {
    return null;
  }
}

export function markVersionSeen(version: string): void {
  try {
    localStorage.setItem(LAST_SEEN_KEY, version);
  } catch {
    /* ignore */
  }
}

/** Release notes for a version, or null if none are curated. */
export function notesFor(version: string): string[] | null {
  return RELEASE_NOTES[version] ?? null;
}

// Version of an update that has been downloaded and is ready to apply on
// restart, if any (drives the "restart to update" banner).
const [updateReadyVersion, setUpdateReadyVersion] = createSignal<string | null>(
  null,
);

export function useUpdateReadyVersion(): () => string | null {
  return updateReadyVersion;
}

/**
 * Check the configured endpoint for a signed update; if one exists, download
 * and stage it. Silent on failure — updates must never break startup. In the
 * browser preview this is a no-op.
 */
export async function checkForUpdates(): Promise<void> {
  if (!isTauri()) return;
  try {
    const update = await check();
    if (update) {
      await update.downloadAndInstall();
      setUpdateReadyVersion(update.version);
    }
  } catch {
    // No endpoint configured / offline / invalid signature — stay silent.
  }
}
