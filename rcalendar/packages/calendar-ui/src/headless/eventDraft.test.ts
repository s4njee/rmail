import { describe, expect, it } from "vitest";
import { buildEventDraft, EventEditorValues } from "./eventDraft";

function filled(overrides: Partial<EventEditorValues> = {}): EventEditorValues {
  return {
    title: "Studio time",
    calendarId: "cal-1",
    dateStr: "2026-08-13",
    startTime: "15:00",
    endTime: "16:30",
    allDay: false,
    repeatFreq: "WEEKLY",
    selectedDays: ["TH"],
    endsMode: "count",
    untilDate: "",
    occurrenceCount: 8,
    tz: "America/Los_Angeles",
    location: "Kane 210",
    notes: "Bring cables",
    travelTime: 20,
    color: "#e8590c",
    fallbackCalendarId: "cal-fallback",
    ...overrides,
  };
}

describe("buildEventDraft", () => {
  it("round-trips every editor control into the save payload", () => {
    const draft = buildEventDraft(filled());
    expect(draft).toEqual({
      calendarId: "cal-1",
      title: "Studio time",
      location: "Kane 210",
      notes: "Bring cables",
      startsAt: "2026-08-13T15:00:00Z",
      endsAt: "2026-08-13T16:30:00Z",
      allDay: false,
      tz: "America/Los_Angeles",
      rrule: "FREQ=WEEKLY;BYDAY=TH;COUNT=8",
      travelTimeMinutes: 20,
      color: "#e8590c",
      busy: true,
    });
  });

  it("clears optional fields when the controls are empty", () => {
    const draft = buildEventDraft(
      filled({
        location: "  ",
        notes: "",
        repeatFreq: "none",
        tz: "local",
        travelTime: 0,
        color: "",
      }),
    );
    expect(draft?.location).toBeNull();
    expect(draft?.notes).toBeNull();
    expect(draft?.rrule).toBeNull();
    expect(draft?.tz).toBeNull();
    expect(draft?.travelTimeMinutes).toBeNull();
    expect(draft?.color).toBeNull();
  });

  it("rejects an empty title", () => {
    expect(buildEventDraft(filled({ title: "   " }))).toBeNull();
  });
});
