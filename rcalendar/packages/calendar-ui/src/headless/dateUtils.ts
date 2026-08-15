/**
 * Pure date math and formatting utilities for headless calendar calculations.
 */

export const MONTH_NAMES = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

export const MONTH_NAMES_SHORT = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
];

export const WEEKDAYS = [
  "Sunday",
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
];
export const WEEKDAYS_SHORT = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
export const WEEKDAYS_LETTER = ["S", "M", "T", "W", "T", "F", "S"];

export interface MonthGridCell {
  date: Date;
  dateString: string; // YYYY-MM-DD
  dayNumber: number;
  isCurrentMonth: boolean;
  isToday: boolean;
  isSelected: boolean;
}

/** Returns a date string formatted as YYYY-MM-DD. */
export function toDateKey(d: Date): string {
  const year = d.getFullYear();
  const month = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

/** Parses a YYYY-MM-DD string into a local Date. */
export function fromDateKey(key: string): Date {
  const [y, m, d] = key.split("-").map(Number);
  return new Date(y, m - 1, d, 0, 0, 0, 0);
}

/** Adds or subtracts days from a Date. */
export function addDays(date: Date, days: number): Date {
  const res = new Date(date);
  res.setDate(res.getDate() + days);
  return res;
}

/** Adds or subtracts months from a Date. */
export function addMonths(date: Date, months: number): Date {
  const res = new Date(date);
  res.setMonth(res.getMonth() + months);
  return res;
}

/** Returns the start of the week for a given date (default Sunday start). */
export function startOfWeek(date: Date, weekStartsOn = 0): Date {
  const res = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const day = res.getDay();
  const diff = (day < weekStartsOn ? 7 : 0) + day - weekStartsOn;
  res.setDate(res.getDate() - diff);
  return res;
}

/** Returns the ISO week number (1-53). */
export function getWeekNumber(date: Date): number {
  const d = new Date(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()));
  const dayNum = d.getUTCDay() || 7;
  d.setUTCDate(d.getUTCDate() + 4 - dayNum);
  const yearStart = new Date(Date.UTC(d.getUTCFullYear(), 0, 1));
  return Math.ceil(((d.getTime() - yearStart.getTime()) / 86400000 + 1) / 7);
}

/** Checks whether two dates represent the exact same calendar day. */
export function isSameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

/** Builds the standard 42-cell (6 weeks x 7 days) calendar month grid. */
export function buildMonthGrid(
  year: number,
  month: number,
  selectedDate: Date,
  today: Date = new Date(),
  weekStartsOn = 0,
): MonthGridCell[] {
  const firstOfMonth = new Date(year, month, 1);
  const startDay = startOfWeek(firstOfMonth, weekStartsOn);

  const cells: MonthGridCell[] = [];
  let current = new Date(startDay);

  for (let i = 0; i < 42; i++) {
    const isCurrentMonth = current.getMonth() === month;
    cells.push({
      date: new Date(current),
      dateString: toDateKey(current),
      dayNumber: current.getDate(),
      isCurrentMonth,
      isToday: isSameDay(current, today),
      isSelected: isSameDay(current, selectedDate),
    });
    current.setDate(current.getDate() + 1);
  }

  return cells;
}

/** Formats time as HH:MM in 24-hour time. */
export function formatTime24(date: Date): string {
  const hours = String(date.getHours()).padStart(2, "0");
  const minutes = String(date.getMinutes()).padStart(2, "0");
  return `${hours}:${minutes}`;
}
