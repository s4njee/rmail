import { EventDraft } from "../types/calendar";

/** Editor field snapshot used to build an [`EventDraft`]. */
export interface EventEditorValues {
  title: string;
  calendarId: string;
  dateStr: string;
  startTime: string;
  endTime: string;
  allDay: boolean;
  repeatFreq: string;
  selectedDays: string[];
  endsMode: "never" | "until" | "count";
  untilDate: string;
  occurrenceCount: number;
  tz: string;
  location: string;
  notes: string;
  travelTime: number;
  color: string;
  fallbackCalendarId: string;
  busy?: boolean;
}

/** Pure mapping from editor controls to the save payload. */
export function buildEventDraft(values: EventEditorValues): EventDraft | null {
  if (!values.title.trim()) return null;

  let startsAt: string;
  let endsAt: string;
  if (values.allDay) {
    startsAt = `${values.dateStr}T00:00:00Z`;
    endsAt = `${values.dateStr}T23:59:59Z`;
  } else {
    startsAt = `${values.dateStr}T${values.startTime}:00Z`;
    endsAt = `${values.dateStr}T${values.endTime}:00Z`;
  }

  let rrule: string | null = null;
  if (values.repeatFreq !== "none") {
    const parts = [`FREQ=${values.repeatFreq}`];
    if (values.repeatFreq === "WEEKLY" && values.selectedDays.length > 0) {
      parts.push(`BYDAY=${values.selectedDays.join(",")}`);
    }
    if (values.endsMode === "count" && values.occurrenceCount > 0) {
      parts.push(`COUNT=${values.occurrenceCount}`);
    } else if (values.endsMode === "until" && values.untilDate) {
      const u = values.untilDate.replace(/-/g, "");
      parts.push(`UNTIL=${u}T235959Z`);
    }
    rrule = parts.join(";");
  }

  return {
    calendarId: values.calendarId || values.fallbackCalendarId || "",
    title: values.title.trim(),
    location: values.location.trim() || null,
    notes: values.notes.trim() || null,
    startsAt,
    endsAt,
    allDay: values.allDay,
    tz: values.tz === "local" ? null : values.tz,
    rrule,
    travelTimeMinutes: values.travelTime > 0 ? values.travelTime : null,
    color: values.color || null,
    busy: values.busy !== false,
  };
}
