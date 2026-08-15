import { For, Show } from "solid-js";
import { formatClock } from "../lib/format";
import { dismissAlarm, snoozeAlarm, useActiveAlarms } from "../lib/alarms";
import "./AlarmBanner.css";

export function AlarmBanner() {
  const alarms = useActiveAlarms();

  return (
    <Show when={alarms().length > 0}>
      <div class="alarm-container" aria-live="polite">
        <For each={alarms()}>
          {(alarm) => (
            <div class="alarm-banner">
              <div class="alarm-banner__icon">🔔</div>
              <div class="alarm-banner__content">
                <div class="alarm-banner__header">
                  <span class="alarm-banner__title">{alarm.title}</span>
                  <span class="alarm-banner__time">
                    Starts at {formatClock(alarm.startMs)}
                  </span>
                </div>
                <Show when={alarm.location}>
                  <span class="alarm-banner__loc">{alarm.location}</span>
                </Show>
              </div>
              <div class="alarm-banner__actions">
                <button
                  type="button"
                  class="btn btn--sm btn--secondary alarm-btn"
                  onClick={() => snoozeAlarm(alarm.eventId, 5)}
                >
                  Snooze 5m
                </button>
                <button
                  type="button"
                  class="btn btn--sm btn--secondary alarm-btn"
                  onClick={() => snoozeAlarm(alarm.eventId, 10)}
                >
                  Snooze 10m
                </button>
                <button
                  type="button"
                  class="btn btn--sm btn--primary alarm-btn"
                  onClick={() => dismissAlarm(alarm.eventId)}
                >
                  Dismiss
                </button>
              </div>
            </div>
          )}
        </For>
      </div>
    </Show>
  );
}
