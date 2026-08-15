import { createSignal } from "solid-js";

// P1.5: the keyboard-shortcut reference modal, opened from the keymap (`?`)
// and a toolbar button so it's reachable from every primary surface.
const [open, setOpen] = createSignal(false);

export function useShortcutsOpen(): () => boolean {
  return open;
}

export function openShortcuts(): void {
  setOpen(true);
}

export function closeShortcuts(): void {
  setOpen(false);
}
