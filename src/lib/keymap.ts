import { requestCompose } from "./compose";
import {
  focusSearch,
  messageListHasFocus,
  selectRelative,
  useSelectedId,
} from "./mail";
import {
  closeSettings,
  effectiveMode,
  enterFocused,
  exitFocused,
  openSettings,
  useSettingsOpen,
} from "./ui";

// The app's one keymap (Epic 9.1) — every binding lives here, not scattered
// in onKeyDown handlers. Keys never fire while a text input is focused.
//
//   j / k / ↑ / ↓   move the message selection
//   Enter            open focused reading      Esc      return
//   r / a / f        reply / reply all / forward (composer lands in Epic 13)
//   /                focus search
//
// Browser-inherited shortcuts that make no sense in an app (reload,
// find-in-page, zoom) are suppressed so they can't lose state.

/** Register the global key + wheel handlers; returns a cleanup. */
export function initKeymap(): () => void {
  const onKeyDown = (event: KeyboardEvent) => {
    const target = event.target as HTMLElement | null;
    const inEditable =
      target && typeof target.closest === "function"
        ? target.closest("input, textarea, [contenteditable='true']")
        : null;

    // Suppress find-in-page (⌘F — the app's search is `/`), reload (⌘R) and
    // zoom (⌘±0 / ⌃scroll) — none belong in a mail client.
    if (
      (event.metaKey || event.ctrlKey) &&
      (event.key === "r" ||
        event.key === "f" ||
        event.key === "+" ||
        event.key === "=" ||
        event.key === "-" ||
        event.key === "0")
    ) {
      event.preventDefault();
      return;
    }

    if (inEditable) return;

    switch (event.key) {
      case ",":
        if (event.metaKey || event.ctrlKey) {
          event.preventDefault();
          if (useSettingsOpen()()) closeSettings();
          else openSettings();
        }
        break;
      case "Enter":
        if (useSelectedId()() != null && effectiveMode() === "three-pane") {
          enterFocused();
        }
        break;
      case "Escape":
        if (useSettingsOpen()()) closeSettings();
        else if (effectiveMode() === "focused") exitFocused();
        break;
      case "j":
        selectRelative(1);
        break;
      case "k":
        selectRelative(-1);
        break;
      // ↑/↓ only move the selection when the message list itself is focused
      // (the listbox pattern); elsewhere they scroll as usual.
      case "ArrowDown":
        if (messageListHasFocus()) selectRelative(1);
        break;
      case "ArrowUp":
        if (messageListHasFocus()) selectRelative(-1);
        break;
      case "/":
        event.preventDefault();
        focusSearch();
        break;
      case "r":
        requestCompose("reply");
        break;
      case "a":
        requestCompose("replyAll");
        break;
      case "f":
        requestCompose("forward");
        break;
    }
  };

  const onWheel = (event: WheelEvent) => {
    if (event.ctrlKey) event.preventDefault(); // ctrl+scroll zoom
  };

  window.addEventListener("keydown", onKeyDown);
  window.addEventListener("wheel", onWheel, { passive: false });
  return () => {
    window.removeEventListener("keydown", onKeyDown);
    window.removeEventListener("wheel", onWheel);
  };
}
