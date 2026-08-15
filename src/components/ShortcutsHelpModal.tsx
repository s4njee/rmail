import { For, Show } from "solid-js";
import { closeShortcuts, useShortcutsOpen } from "../lib/shortcuts";
import { Modal } from "./Modal";
import "./ShortcutsHelpModal.css";

// P1.5 keyboard-shortcut reference, reachable from every primary surface
// (the keymap `?` and a toolbar button). Notes the browser shortcuts the app
// deliberately suppresses so nothing is silently shadowed.
const BINDINGS: [string, string][] = [
  ["j / k", "Move up / down the message list"],
  ["Shift+j / k", "Extend multi-select range"],
  ["Enter", "Open focused reading"],
  ["Esc", "Back / close"],
  ["s", "Star / unstar"],
  ["e", "Archive"],
  ["#", "Delete"],
  ["!", "Junk / not junk"],
  ["r / a / f", "Reply / reply all / forward"],
  ["c", "New message"],
  ["/", "Focus search"],
  [",", "Open Settings"],
  ["⌘P", "Print"],
  ["?", "This shortcut reference"],
];

const SUPPRESSED = "⌘R reload · ⌘F find-in-page · ⌘+ / ⌘- / ⌘0 zoom";

export function ShortcutsHelpModal() {
  const open = useShortcutsOpen();
  return (
    <Show when={open()}>
      <Modal title="Keyboard shortcuts" onClose={closeShortcuts}>
        <div class="shortcuts-help">
          <ul class="shortcuts-help__list">
            <For each={BINDINGS}>
              {([keys, desc]) => (
                <li class="shortcuts-help__row">
                  <kbd class="shortcuts-help__keys">{keys}</kbd>
                  <span>{desc}</span>
                </li>
              )}
            </For>
          </ul>
          <p class="shortcuts-help__conflict">
            The app suppresses the browser shortcuts that would lose state:
            {" "}{SUPPRESSED}. Bindings shown here are app-wide.
          </p>
        </div>
      </Modal>
    </Show>
  );
}
