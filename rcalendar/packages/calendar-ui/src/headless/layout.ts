/**
 * Headless layout engine for time grids (Week, 3-day, Day).
 *
 * Computes event positions (top, height, column split) and now-line positioning
 * without any DOM dependencies (S3.2).
 */

import { OccurrenceItem } from '../types/calendar';

export interface GridConfig {
  startHour: number;   // e.g. 7 (07:00)
  endHour: number;     // e.g. 21 (21:00)
  rowPitch: number;    // e.g. 45px for Week, 47px for 3-day, 49px for Day
  minHeight?: number;  // e.g. 20px
}

export interface PositionedEvent {
  item: OccurrenceItem;
  top: number;
  height: number;
  leftPercent: number;
  widthPercent: number;
  startHourFraction: number;
  durationHours: number;
}

/** Computes the top pixel offset from startHour. */
export function computeTop(date: Date, startHour: number, rowPitch: number): number {
  const hours = date.getHours() + date.getMinutes() / 60 + date.getSeconds() / 3600;
  return Math.max(0, (hours - startHour) * rowPitch);
}

/** Computes block height in pixels from duration in hours. */
export function computeHeight(durationHours: number, rowPitch: number, minHeight = 18): number {
  return Math.max(minHeight, durationHours * rowPitch - 3);
}

/** Computes the now-line vertical position. Returns null if current time is outside the visible hours. */
export function computeNowLinePosition(now: Date, config: GridConfig): number | null {
  const hours = now.getHours() + now.getMinutes() / 60;
  if (hours < config.startHour || hours > config.endHour) {
    return null;
  }
  return (hours - config.startHour) * config.rowPitch;
}

/**
 * Positions timed events within a single day column, handling side-by-side overlaps.
 */
export function positionEventsForDay(
  items: OccurrenceItem[],
  dayDate: Date,
  config: GridConfig
): PositionedEvent[] {
  // Filter only timed events matching dayDate
  const dayStart = new Date(dayDate.getFullYear(), dayDate.getMonth(), dayDate.getDate(), 0, 0, 0);
  const dayEnd = new Date(dayDate.getFullYear(), dayDate.getMonth(), dayDate.getDate(), 23, 59, 59, 999);

  const timed = items.filter((item) => {
    if (item.occurrence.allDay || item.event.allDay) return false;
    const start = new Date(item.occurrence.startsAt);
    const end = new Date(item.occurrence.endsAt);
    return start <= dayEnd && end >= dayStart;
  });

  if (timed.length === 0) return [];

  // Sort by start time, then duration descending
  const sorted = [...timed].sort((a, b) => {
    const aStart = new Date(a.occurrence.startsAt).getTime();
    const bStart = new Date(b.occurrence.startsAt).getTime();
    if (aStart !== bStart) return aStart - bStart;
    const aDur = new Date(a.occurrence.endsAt).getTime() - aStart;
    const bDur = new Date(b.occurrence.endsAt).getTime() - bStart;
    return bDur - aDur;
  });

  // Calculate top & bottom for each event
  const events = sorted.map((item) => {
    const start = new Date(item.occurrence.startsAt);
    const end = new Date(item.occurrence.endsAt);
    const startFraction = start.getHours() + start.getMinutes() / 60;
    const endFraction = end.getHours() + end.getMinutes() / 60;
    const duration = Math.max(0.25, (end.getTime() - start.getTime()) / 3600000);

    const top = computeTop(start, config.startHour, config.rowPitch);
    const height = computeHeight(duration, config.rowPitch, config.minHeight);

    return {
      item,
      top,
      height,
      bottom: top + height,
      startFraction,
      endFraction,
      durationHours: duration,
      col: 0,
      totalCols: 1,
    };
  });

  // Assign columns for overlapping clusters
  const columns: { bottom: number }[] = [];
  for (const evt of events) {
    let placed = false;
    for (let c = 0; c < columns.length; c++) {
      if (columns[c].bottom <= evt.top) {
        evt.col = c;
        columns[c].bottom = evt.bottom;
        placed = true;
        break;
      }
    }
    if (!placed) {
      evt.col = columns.length;
      columns.push({ bottom: evt.bottom });
    }
  }

  // Calculate cluster widths
  for (let i = 0; i < events.length; i++) {
    const a = events[i];
    let clusterMaxCols = a.col + 1;
    for (let j = 0; j < events.length; j++) {
      if (i === j) continue;
      const b = events[j];
      if (a.top < b.bottom && a.bottom > b.top) {
        clusterMaxCols = Math.max(clusterMaxCols, b.col + 1);
      }
    }
    a.totalCols = clusterMaxCols;
  }

  return events.map((e) => {
    const widthPercent = 100 / e.totalCols;
    const leftPercent = e.col * widthPercent;
    return {
      item: e.item,
      top: e.top,
      height: e.height,
      leftPercent,
      widthPercent,
      startHourFraction: e.startFraction,
      durationHours: e.durationHours,
    };
  });
}
