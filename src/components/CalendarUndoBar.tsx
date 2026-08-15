import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import {
  clearCalendarUndo,
  undoCalendar,
  useCalendarUndo,
} from "../lib/calendar";
import "./CalendarUndoBar.css";

// P1.4 calendar undo: a short-window bar that reverses an event delete, edit,
// drag, resize, or create. Pattern: TriageUndoBar.
const UNDO_WINDOW_S = 7;

export function CalendarUndoBar() {
  const undo = useCalendarUndo();
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
          clearCalendarUndo();
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
      <div class="calendar-undo" role="status" aria-live="polite">
        <div
          class="calendar-undo__progress"
          style={{ width: `${percent()}%` }}
        />
        <div class="calendar-undo__content">
          <span class="calendar-undo__text">{undo()!.label}</span>
          <div class="calendar-undo__actions">
            <button
              type="button"
              class="calendar-undo__btn calendar-undo__btn--undo"
              onClick={() => void undoCalendar()}
            >
              Undo
            </button>
            <button
              type="button"
              class="calendar-undo__btn"
              onClick={clearCalendarUndo}
            >
              Dismiss
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}
