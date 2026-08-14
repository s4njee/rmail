/**
 * Headless drag interaction engine for calendar time grids (S4.1, S4.2, S4.3).
 *
 * Provides pure calculations for:
 * - Snap-to-interval time offsets (e.g. 15-minute snaps)
 * - Move delta in minutes given pixel displacement
 * - Resize duration delta given pixel displacement with minimum duration clamp
 * - Drag-to-create range calculation across start and end slots
 */

export interface DragSnapConfig {
  rowPitch: number;      // pixels per hour (e.g. 45, 47, 49)
  snapMinutes?: number;  // default 15
  startHour?: number;    // default 7 (07:00)
  endHour?: number;      // default 21 (21:00)
  minDurationMinutes?: number; // default 15
}

/** Snaps minutes to the nearest interval (default 15 minutes). */
export function snapToInterval(minutes: number, interval = 15): number {
  return Math.round(minutes / interval) * interval;
}

/** Computes minutes moved based on vertical pixel displacement. */
export function computeMoveDeltaMinutes(
  deltaPixels: number,
  config: DragSnapConfig
): number {
  const pixelsPerHour = config.rowPitch;
  const rawMinutes = (deltaPixels / pixelsPerHour) * 60;
  return snapToInterval(rawMinutes, config.snapMinutes || 15);
}

/** Computes resized end time based on bottom-edge vertical pixel displacement. */
export function computeResizedEnd(
  currentStart: Date,
  currentEnd: Date,
  deltaPixels: number,
  config: DragSnapConfig
): Date {
  const currentDurationMinutes = (currentEnd.getTime() - currentStart.getTime()) / 60000;
  const deltaMinutes = computeMoveDeltaMinutes(deltaPixels, config);
  const minDuration = config.minDurationMinutes || 15;
  const newDurationMinutes = Math.max(minDuration, currentDurationMinutes + deltaMinutes);

  const newEnd = new Date(currentStart.getTime() + newDurationMinutes * 60000);
  return newEnd;
}

/** Computes new start and end times when moving an event by delta pixels. */
export function computeMovedRange(
  currentStart: Date,
  currentEnd: Date,
  deltaPixels: number,
  config: DragSnapConfig
): { newStart: Date; newEnd: Date } {
  const durationMs = currentEnd.getTime() - currentStart.getTime();
  const deltaMinutes = computeMoveDeltaMinutes(deltaPixels, config);
  const deltaMs = deltaMinutes * 60000;

  const newStart = new Date(currentStart.getTime() + deltaMs);
  const newEnd = new Date(newStart.getTime() + durationMs);

  return { newStart, newEnd };
}

/** Computes new event start and end times from drag-to-create coordinates. */
export function computeDragToCreateRange(
  dayDate: Date,
  startPixelY: number,
  currentPixelY: number,
  config: DragSnapConfig
): { startsAt: Date; endsAt: Date } {
  const startHour = config.startHour ?? 7;
  const pixelsPerHour = config.rowPitch;
  const snap = config.snapMinutes || 15;

  const topY = Math.min(startPixelY, currentPixelY);
  const bottomY = Math.max(startPixelY, currentPixelY);

  const startMinutesRaw = (topY / pixelsPerHour) * 60;
  const endMinutesRaw = (bottomY / pixelsPerHour) * 60;

  const snappedStartMinutes = Math.max(0, snapToInterval(startMinutesRaw, snap));
  const snappedEndMinutes = Math.max(
    snappedStartMinutes + (config.minDurationMinutes || 15),
    snapToInterval(endMinutesRaw, snap)
  );

  const startsAt = new Date(dayDate.getFullYear(), dayDate.getMonth(), dayDate.getDate(), startHour, 0, 0);
  startsAt.setMinutes(startsAt.getMinutes() + snappedStartMinutes);

  const endsAt = new Date(dayDate.getFullYear(), dayDate.getMonth(), dayDate.getDate(), startHour, 0, 0);
  endsAt.setMinutes(endsAt.getMinutes() + snappedEndMinutes);

  return { startsAt, endsAt };
}
