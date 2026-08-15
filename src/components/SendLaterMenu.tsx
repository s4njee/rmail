import { createSignal, For, Show } from "solid-js";
import { tomorrowMorning, tonight } from "./SnoozeMenu";
import "./SnoozeMenu.css";

// P1.1 "Send later" picker in the composer footer. Same local-wall-clock epoch
// math as snooze (DST-safe), with send-appropriate presets + a custom time.
export function SendLaterMenu(props: {
  onSchedule: (sendAtMs: number) => void;
  disabled?: boolean;
}) {
  const [open, setOpen] = createSignal(false);
  const [custom, setCustom] = createSignal("");

  const pick = (sendAtMs: number) => {
    setOpen(false);
    props.onSchedule(sendAtMs);
  };

  const pickCustom = () => {
    if (!custom()) return;
    const ms = new Date(custom()).getTime();
    if (!Number.isNaN(ms)) pick(ms);
  };

  const presets: { label: string; at: number }[] = [
    { label: "Tonight at 6pm", at: tonight() },
    { label: "Tomorrow at 9am", at: tomorrowMorning() },
    { label: "Tomorrow at 6pm", at: tomorrowMorning() + 9 * 3600_000 },
  ];

  return (
    <div class="snooze">
      <button
        type="button"
        class="btn btn--secondary"
        disabled={props.disabled}
        onClick={() => setOpen(!open())}
        aria-haspopup="menu"
        aria-expanded={open()}
      >
        Send later…
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
                onClick={() => pick(p.at)}
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
              aria-label="Custom send time"
            />
            <button
              type="button"
              class="btn btn--primary btn--sm"
              disabled={!custom()}
              onClick={pickCustom}
            >
              Schedule
            </button>
          </div>
          <p class="snooze__note">Quill must be running at the send time.</p>
        </div>
      </Show>
    </div>
  );
}
