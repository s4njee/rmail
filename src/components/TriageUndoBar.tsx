import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import { clearTriageUndo, undoTriage, useTriageUndo } from "../lib/mail";
import "./TriageUndoBar.css";

// P1.1 consistent undo for triage (archive/move/junk/star/read/delete/snooze):
// a short-window bar that reverses the last action and cancels its queued
// server action. Pattern: UndoSendBar.
const UNDO_WINDOW_S = 7;

export function TriageUndoBar() {
  const undo = useTriageUndo();
  const [secondsLeft, setSecondsLeft] = createSignal(0);
  let timer: ReturnType<typeof setInterval> | undefined;

  createEffect(() => {
    const u = undo();
    if (timer) clearInterval(timer);
    if (!u) {
      setSecondsLeft(0);
      return;
    }
    setSecondsLeft(UNDO_WINDOW_S);
    timer = setInterval(() => {
      setSecondsLeft((s) => {
        if (s <= 1) {
          clearInterval(timer);
          clearTriageUndo();
          return 0;
        }
        return s - 1;
      });
    }, 1000);
  });

  onCleanup(() => {
    if (timer) clearInterval(timer);
  });

  const percent = () => (secondsLeft() / UNDO_WINDOW_S) * 100;

  return (
    <Show when={undo()}>
      <div class="triage-undo" role="status" aria-live="polite">
        <div class="triage-undo__progress" style={{ width: `${percent()}%` }} />
        <div class="triage-undo__content">
          <span class="triage-undo__text">{undo()!.label}</span>
          <div class="triage-undo__actions">
            <button
              type="button"
              class="triage-undo__btn triage-undo__btn--undo"
              onClick={() => void undoTriage()}
            >
              Undo
            </button>
            <button
              type="button"
              class="triage-undo__btn"
              onClick={clearTriageUndo}
            >
              Dismiss
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}
