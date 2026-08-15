import { describe, expect, it } from "vitest";
import {
  computeDragToCreateRange,
  computeMoveDeltaMinutes,
  computeMovedRange,
  computeResizedEnd,
  snapToInterval,
} from "./dragEngine";

describe("dragEngine", () => {
  const config = {
    rowPitch: 60, // 1px = 1 min
    snapMinutes: 15,
    minDurationMinutes: 15,
    startHour: 7,
  };

  it("snaps minutes to intervals", () => {
    expect(snapToInterval(14, 15)).toBe(15);
    expect(snapToInterval(7, 15)).toBe(0);
    expect(snapToInterval(8, 15)).toBe(15);
    expect(snapToInterval(44, 15)).toBe(45);
  });

  it("computes move delta in minutes", () => {
    expect(computeMoveDeltaMinutes(30, config)).toBe(30);
    expect(computeMoveDeltaMinutes(14, config)).toBe(15);
    expect(computeMoveDeltaMinutes(-28, config)).toBe(-30);
  });

  it("computes moved range while preserving duration", () => {
    const start = new Date(2026, 7, 13, 10, 0, 0);
    const end = new Date(2026, 7, 13, 11, 30, 0); // 90 min duration

    const moved = computeMovedRange(start, end, 30, config);
    expect(moved.newStart.getHours()).toBe(10);
    expect(moved.newStart.getMinutes()).toBe(30);
    expect(moved.newEnd.getHours()).toBe(12);
    expect(moved.newEnd.getMinutes()).toBe(0);
  });

  it("computes resized end time and enforces minimum duration", () => {
    const start = new Date(2026, 7, 13, 10, 0, 0);
    const end = new Date(2026, 7, 13, 11, 0, 0); // 60 min

    const extended = computeResizedEnd(start, end, 30, config);
    expect(extended.getHours()).toBe(11);
    expect(extended.getMinutes()).toBe(30);

    const clamped = computeResizedEnd(start, end, -90, config);
    // minimum duration 15 min -> 10:15
    expect(clamped.getHours()).toBe(10);
    expect(clamped.getMinutes()).toBe(15);
  });

  it("computes drag-to-create range", () => {
    const day = new Date(2026, 7, 13);
    // start at 60px (= 08:00), end at 150px (= 09:30)
    const range = computeDragToCreateRange(day, 60, 150, config);
    expect(range.startsAt.getHours()).toBe(8);
    expect(range.startsAt.getMinutes()).toBe(0);
    expect(range.endsAt.getHours()).toBe(9);
    expect(range.endsAt.getMinutes()).toBe(30);
  });
});
