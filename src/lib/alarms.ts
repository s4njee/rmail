import { createSignal } from "solid-js";
import type { CalendarEvent } from "./ipc/CalendarEvent";
import { showNotification } from "./notifications";
import { getSettings } from "./tauri";

export interface FiredAlarm {
  eventId: number;
  title: string;
  startMs: number;
  location: string | null;
  firedAtMs: number;
}

const [activeAlarms, setActiveAlarms] = createSignal<FiredAlarm[]>([]);
// Dedup keyed by (event id, start, alarm): an event that is rescheduled (its id
// is kept by updateEvent) must alarm again at the new time, not be suppressed
// for the whole session.
const firedEventKeys = new Set<string>();
let schedulerInterval: ReturnType<typeof setInterval> | undefined;

export function useActiveAlarms(): () => FiredAlarm[] {
  return activeAlarms;
}

export function dismissAlarm(eventId: number): void {
  setActiveAlarms((prev) => prev.filter((a) => a.eventId !== eventId));
}

export function snoozeAlarm(eventId: number, snoozeMinutes: number): void {
  const current = activeAlarms().find((a) => a.eventId === eventId);
  dismissAlarm(eventId);
  if (!current) return;

  const snoozedTriggerMs = Date.now() + snoozeMinutes * 60_000;
  setTimeout(() => {
    fireAlarm({
      ...current,
      firedAtMs: Date.now(),
    });
  }, snoozedTriggerMs - Date.now());
}

function fireAlarm(alarm: FiredAlarm): void {
  setActiveAlarms((prev) => [
    ...prev.filter((a) => a.eventId !== alarm.eventId),
    alarm,
  ]);

  const loc = alarm.location ? `\nLocation: ${alarm.location}` : "";
  const timeStr = new Date(alarm.startMs).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
  void showNotification(
    `Reminder: ${alarm.title}`,
    `Starts at ${timeStr}${loc}`,
  );
}

export async function checkAlarms(events: CalendarEvent[]): Promise<void> {
  const settings = await getSettings();
  const defaultAlarm = settings.notifications?.defaultAlarmMinutes ?? 15;
  const now = Date.now();

  for (const event of events) {
    const alarmMin = event.alarm_minutes_before ?? defaultAlarm;
    if (alarmMin == null) continue;

    const triggerMs = event.start_ms - alarmMin * 60_000;
    const fireKey = `${event.id}:${event.start_ms}:${alarmMin}`;
    if (firedEventKeys.has(fireKey)) continue;

    // If trigger is in the past by less than 10 minutes and hasn't started more than 10 min ago
    if (now >= triggerMs && now <= event.start_ms + 600_000) {
      firedEventKeys.add(fireKey);
      fireAlarm({
        eventId: event.id,
        title: event.title,
        startMs: event.start_ms,
        location: event.location,
        firedAtMs: now,
      });
    }
  }
}

export function startAlarmScheduler(
  getEvents: () => Promise<CalendarEvent[]>,
): () => void {
  if (schedulerInterval) clearInterval(schedulerInterval);

  const runCheck = () => {
    void getEvents().then((events) => {
      void checkAlarms(events);
    });
  };

  runCheck();
  schedulerInterval = setInterval(runCheck, 30_000); // Check every 30 seconds

  return () => {
    if (schedulerInterval) clearInterval(schedulerInterval);
  };
}
