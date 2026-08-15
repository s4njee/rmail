import { createSignal, For, Show } from "solid-js";
import "./SnoozeMenu.css";

// P1.1 snooze picker: a menu of presets plus a custom local datetime. Times
// are computed as epoch ms from the local wall clock, so DST transitions are
// handled by the platform (a "9am" stays 9am wall-clock).
const HOUR = 3600_000;

function atLocal(hour: number, minute: number, dayOffset = 0): number {
  const d = new Date();
  d.setDate(d.getDate() + dayOffset);
  d.setHours(hour, minute, 0, 0);
  return d.getTime();
}

export function tonight(): number {
  const t = atLocal(18, 0);
  return t > Date.now() ? t : atLocal(18, 0, 1);
}

export function tomorrowMorning(): number {
  return atLocal(9, 0, 1);
}

function thisWeekend(): number {
  const d = new Date();
  const daysToSaturday = (6 - d.getDay() + 7) % 7 || 7; // ≥1: next Saturday
  return atLocal(9, 0, daysToSaturday);
}

export function SnoozeMenu(props: {
  onSnooze: (untilMs: number) => void;
  disabled?: boolean;
}) {
  const [open, setOpen] = createSignal(false);
  const [custom, setCustom] = createSignal("");

  const pick = (untilMs: number) => {
    setOpen(false);
    props.onSnooze(untilMs);
  };

  const pickCustom = () => {
    if (!custom()) return;
    const ms = new Date(custom()).getTime();
    if (!Number.isNaN(ms)) pick(ms);
  };

  const presets: { label: string; until: number }[] = [
    { label: "In 1 hour", until: Date.now() + HOUR },
    { label: "Tonight at 6pm", until: tonight() },
    { label: "Tomorrow at 9am", until: tomorrowMorning() },
    { label: "This weekend", until: thisWeekend() },
  ];

  return (
    <div class="snooze">
      <button
        type="button"
        class="btn btn--secondary btn--sm"
        disabled={props.disabled}
        onClick={() => setOpen(!open())}
        aria-haspopup="menu"
        aria-expanded={open()}
      >
        Snooze…
      </button>
      <Show when={open()}>
        <div
          class="snooze__menu"
          role="menu"
          onMouseDown={(e) => e.stopPropagation()}
        >
          <For each={presets}>
            {(p) => (
              <button
                type="button"
                role="menuitem"
                class="snooze__item"
                onClick={() => pick(p.until)}
              >
                {p.label}
              </button>
            )}
          </For>
          <div class="snooze__custom">
            <input
              type="datetime-local"
              value={custom()}
              onInput={(e) => setCustom(e.currentTarget.value)}
              aria-label="Custom snooze time"
            />
            <button
              type="button"
              class="btn btn--primary btn--sm"
              disabled={!custom()}
              onClick={pickCustom}
            >
              Snooze
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
}
