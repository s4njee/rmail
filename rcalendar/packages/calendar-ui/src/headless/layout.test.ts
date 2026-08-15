import { describe, expect, it } from "vitest";
import {
  addDays,
  addMonths,
  buildMonthGrid,
  getWeekNumber,
  isSameDay,
  startOfWeek,
  toDateKey,
} from "./dateUtils";
import {
  computeHeight,
  computeNowLinePosition,
  computeTop,
  findConflictingEventIds,
  isWithinWorkingHours,
  positionEventsForDay,
} from "./layout";
import { OccurrenceItem } from "../types/calendar";

describe("dateUtils", () => {
  it("computes start of week and month grids correctly", () => {
    const d = new Date(2026, 7, 13); // Aug 13, 2026 (Thursday)
    const sunStart = startOfWeek(d, 0); // Sunday Aug 9
    expect(sunStart.getDate()).toBe(9);
    expect(sunStart.getMonth()).toBe(7);

    const monStart = startOfWeek(d, 1); // Monday Aug 10
    expect(monStart.getDate()).toBe(10);

    const grid = buildMonthGrid(2026, 7, d, d, 0);
    expect(grid.length).toBe(42);
    expect(grid[0].dayNumber).toBe(26); // July 26, 2026
    expect(grid[0].isCurrentMonth).toBe(false);

    const aug13 = grid.find((c) => c.dateString === "2026-08-13");
    expect(aug13?.isToday).toBe(true);
    expect(aug13?.isCurrentMonth).toBe(true);
  });

  it("handles leap years and short months in addMonths", () => {
    const jan31 = new Date(2024, 0, 31);
    const feb = addMonths(jan31, 1);
    expect(feb.getMonth()).toBe(2); // JS Date roll-over or clamp
    expect(isSameDay(addDays(new Date(2026, 7, 13), 2), new Date(2026, 7, 15))).toBe(true);
  });

  it("computes week number", () => {
    const aug13 = new Date(2026, 7, 13);
    expect(getWeekNumber(aug13)).toBe(33);
  });
});

describe("layout engine", () => {
  const config = {
    startHour: 7,
    endHour: 21,
    rowPitch: 45,
  };

  it("computes top offset and height for timed events", () => {
    const tenAm = new Date(2026, 7, 13, 10, 0, 0);
    // (10 - 7) * 45 = 135
    expect(computeTop(tenAm, 7, 45)).toBe(135);

    // 1.5h * 45 - 3 = 64.5
    expect(computeHeight(1.5, 45)).toBe(64.5);
  });

  it("computes now-line position", () => {
    const now1340 = new Date(2026, 7, 13, 13, 40, 0);
    // (13 + 40/60 - 7) * 45 = 6.6666 * 45 = 300
    const pos = computeNowLinePosition(now1340, config);
    expect(Math.round(pos!)).toBe(300);

    const early = new Date(2026, 7, 13, 5, 0, 0);
    expect(computeNowLinePosition(early, config)).toBeNull();
  });

  it("positions overlapping events side-by-side without collision", () => {
    const day = new Date(2026, 7, 13);
    const itemA: OccurrenceItem = {
      occurrence: {
        eventId: "1",
        startsAt: new Date(2026, 7, 13, 10, 0, 0).toISOString(),
        endsAt: new Date(2026, 7, 13, 11, 30, 0).toISOString(),
        allDay: false,
      },
      event: {
        id: "1",
        calendarId: "c1",
        uid: "u1",
        title: "Event A",
        startsAt: new Date(2026, 7, 13, 10, 0, 0).toISOString(),
        endsAt: new Date(2026, 7, 13, 11, 30, 0).toISOString(),
        allDay: false,
      },
    };
    const itemB: OccurrenceItem = {
      occurrence: {
        eventId: "2",
        startsAt: new Date(2026, 7, 13, 10, 30, 0).toISOString(),
        endsAt: new Date(2026, 7, 13, 12, 0, 0).toISOString(),
        allDay: false,
      },
      event: {
        id: "2",
        calendarId: "c1",
        uid: "u2",
        title: "Event B",
        startsAt: new Date(2026, 7, 13, 10, 30, 0).toISOString(),
        endsAt: new Date(2026, 7, 13, 12, 0, 0).toISOString(),
        allDay: false,
      },
    };

    const positioned = positionEventsForDay([itemA, itemB], day, config);
    expect(positioned.length).toBe(2);
    expect(positioned[0].widthPercent).toBe(50);
    expect(positioned[1].widthPercent).toBe(50);
    expect(positioned[0].leftPercent).toBe(0);
    expect(positioned[1].leftPercent).toBe(50);
  });
});

// P1.4 conflict detection + working hours
describe("findConflictingEventIds", () => {
  const day = new Date(2026, 7, 15, 0, 0, 0);
  const item = (id: string, startH: number, endH: number): OccurrenceItem => ({
    occurrence: {
      eventId: id,
      startsAt: new Date(2026, 7, 15, startH, 0, 0).toISOString(),
      endsAt: new Date(2026, 7, 15, endH, 0, 0).toISOString(),
      allDay: false,
    },
    event: {
      id,
      calendarId: "cal",
      uid: id,
      title: id,
      startsAt: new Date(2026, 7, 15, startH, 0, 0).toISOString(),
      endsAt: new Date(2026, 7, 15, endH, 0, 0).toISOString(),
      allDay: false,
    },
  });

  it("flags overlapping events on the same day", () => {
    const items = [item("a", 9, 10), item("b", 9, 11), item("c", 14, 15)];
    const conflicts = findConflictingEventIds(items, day);
    expect(conflicts.has("a")).toBe(true);
    expect(conflicts.has("b")).toBe(true);
    expect(conflicts.has("c")).toBe(false);
  });

  it("does not flag back-to-back events", () => {
    const items = [item("a", 9, 10), item("b", 10, 11)];
    expect(findConflictingEventIds(items, day).size).toBe(0);
  });

  it("ignores all-day events", () => {
    const allDay = { ...item("x", 9, 10), occurrence: { ...item("x", 9, 10).occurrence, allDay: true } };
    expect(findConflictingEventIds([allDay, item("y", 9, 10)], day).size).toBe(0);
  });
});

describe("isWithinWorkingHours", () => {
  it("respects the window", () => {
    const at = (h: number) => new Date(2026, 7, 15, h, 0, 0);
    const wh = { start: 9, end: 17 };
    expect(isWithinWorkingHours(at(9), wh)).toBe(true);
    expect(isWithinWorkingHours(at(16.5), wh)).toBe(true);
    expect(isWithinWorkingHours(at(8), wh)).toBe(false);
    expect(isWithinWorkingHours(at(17), wh)).toBe(false);
  });

  it("treats no window as always within", () => {
    expect(isWithinWorkingHours(new Date(2026, 7, 15, 3, 0, 0), undefined)).toBe(true);
  });
});
