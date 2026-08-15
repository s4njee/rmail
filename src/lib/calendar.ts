import { createSignal } from "solid-js";
import type { Calendar, Task } from "@rcalendar/ui";
import {
  calendarIdFor,
  getDisabledCalendarIds,
  getHiddenFromSidebarIds,
} from "./calendarAdapter";
import type { CalendarEvent } from "./ipc/CalendarEvent";
import type { CalendarSource } from "./ipc/CalendarSource";
import { useAccounts } from "./mail";
import {
  createEvent,
  deleteEvent,
  listCalendars,
  listEvents,
  listRemovedCalendarSources,
  listTasks,
  removeCalendarSource as removeCalendarSourceCmd,
  restoreCalendarSource as restoreCalendarSourceCmd,
  restoreEvent,
  toggleTask,
  updateEvent,
} from "./tauri";

// Calendar state (Epic 14): events loaded from the store for the visible
// range, plus the selection. Recurrence is expanded in Rust (Epic 14.1) — the
// frontend only ever sees resolved instances, so this is a plain list. The
// view reloads after any mutation.
//
// The navigation state (focused/selected date, tasks) lives here too, shared
// between the CalendarView and the embedded calendar sidebar in the app chrome.

const [events, setEvents] = createSignal<CalendarEvent[]>([]);
const [selected, setSelected] = createSignal<CalendarEvent | null>(null);

const [focusedDate, setFocusedDate] = createSignal<Date>(new Date());
const [selectedDate, setSelectedDate] = createSignal<Date>(new Date());
const [tasks, setTasks] = createSignal<Task[]>([]);
// "Open the new-event editor" request from outside CalendarView (e.g. the
// calendar sidebar's "+"). CalendarView consumes and clears it.
const [newEventRequest, setNewEventRequest] = createSignal<Date | null>(null);

export function useCalendarFocusedDate(): () => Date {
  return focusedDate;
}

export function useCalendarSelectedDate(): () => Date {
  return selectedDate;
}

export function setCalendarFocusedDate(d: Date): void {
  setFocusedDate(d);
}

export function setCalendarSelectedDate(d: Date): void {
  setSelectedDate(d);
}

export function useCalendarTasks(): () => Task[] {
  return tasks;
}

export async function loadCalendarTasks(): Promise<void> {
  const raw = await listTasks();
  setTasks(
    raw.map((t) => ({
      id: String(t.id),
      calendarId: `cal-${t.accountId}`,
      title: t.title,
      dueAt: t.dueAtMs ? new Date(t.dueAtMs).toISOString() : null,
      completedAt: t.completedAtMs
        ? new Date(t.completedAtMs).toISOString()
        : null,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    })),
  );
}

export async function toggleCalendarTask(taskId: string): Promise<void> {
  const numId = Number(taskId);
  if (!isNaN(numId)) {
    await toggleTask(numId);
    await loadCalendarTasks();
  }
}

/** Calendars for the current accounts, reflecting the sidebar's show/hide and
 * which ones the user removed from the sidebar. */
export function calendarList(): Calendar[] {
  const accs = useAccounts()();
  const sources = calendarSources();
  const hidden = getHiddenFromSidebarIds();
  const list: Calendar[] = accs
    .map((acc) => {
      const calId = calendarIdFor(acc.id);
      return {
        id: calId,
        accountId: String(acc.id),
        name: acc.address,
        color: acc.color || "#3b5bdb",
        enabled: !getDisabledCalendarIds().has(calId),
        eventCount: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      };
    })
    .filter((cal) => !hidden.has(cal.id));
  // One row per synced source calendar (e.g. each Google calendar) so they
  // show and toggle independently instead of flattening into the account.
  for (const s of sources) {
    const calId = calendarIdFor(s.accountId, s.source);
    if (hidden.has(calId)) continue;
    list.push({
      id: calId,
      accountId: String(s.accountId),
      name: s.name,
      color: s.color,
      enabled: !getDisabledCalendarIds().has(calId),
      eventCount: 0,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    });
  }
  return list;
}

const [calendarSources, setCalendarSources] = createSignal<CalendarSource[]>([]);

/** Distinct source calendars present in the store (Roadmap 4.4). */
export function useCalendarSources(): () => CalendarSource[] {
  return calendarSources;
}

export async function loadCalendarSources(): Promise<void> {
  setCalendarSources(await listCalendars());
}

const [removedCalendarSources, setRemovedCalendarSources] = createSignal<
  CalendarSource[]
>([]);

/** Source calendars the user has removed (Settings "Removed" list). */
export function useRemovedCalendarSources(): () => CalendarSource[] {
  return removedCalendarSources;
}

export async function loadRemovedCalendarSources(): Promise<void> {
  setRemovedCalendarSources(await listRemovedCalendarSources());
}

/** Delete a source calendar's local events and stop it re-syncing. */
export async function removeSourceCalendar(
  accountId: number,
  source: string,
): Promise<void> {
  await removeCalendarSourceCmd(accountId, source);
  await loadCalendarSources();
  await loadRemovedCalendarSources();
}

/** Undo a calendar removal so the next sync re-adds it. */
export async function restoreSourceCalendar(
  accountId: number,
  source: string,
): Promise<void> {
  await restoreCalendarSourceCmd(accountId, source);
  await loadRemovedCalendarSources();
}

// Calendar-sync activity flag — set by the "Sync Cal" action so the bottom
// status bar can show "Syncing calendar…" while a manual sync runs.
const [calendarSyncing, setCalendarSyncingSignal] = createSignal(false);

export function useCalendarSyncing(): () => boolean {
  return calendarSyncing;
}

export function setCalendarSyncing(active: boolean): void {
  setCalendarSyncingSignal(active);
}

/** Ask CalendarView to open the new-event editor (used by the sidebar "+"). */
export function requestNewEvent(date?: Date): void {
  setNewEventRequest(date ?? new Date());
}

export function useNewEventRequest(): () => Date | null {
  return newEventRequest;
}

export function clearNewEventRequest(): void {
  setNewEventRequest(null);
}

export function useEvents(): () => CalendarEvent[] {
  return events;
}

export function useSelectedEvent(): () => CalendarEvent | null {
  return selected;
}

export async function loadEvents(
  startMs: number,
  endMs: number,
): Promise<void> {
  setEvents(await listEvents(startMs, endMs));
}

export function selectEvent(event: CalendarEvent | null): void {
  setSelected(event);
}

export async function saveEvent(event: CalendarEvent): Promise<void> {
  const before = events().find((e) => e.id === event.id) ?? null;
  await updateEvent(event);
  // Keep the in-memory list in sync so the edit shows immediately and the
  // detail pane keeps the event's reminder (a fresh list round-trip would
  // otherwise be needed to see the change).
  setEvents((prev) => prev.map((e) => (e.id === event.id ? event : e)));
  setSelected(event);
  if (before && JSON.stringify(before) !== JSON.stringify(event)) {
    recordCalendarUndo({ label: `Edited "${event.title}"`, event: before });
  }
}

export async function createNewEvent(event: CalendarEvent): Promise<void> {
  const created = await createEvent(event);
  recordCalendarUndo({ label: `Created "${created.title}"`, createdEventId: created.id });
}

export async function removeEvent(id: number): Promise<void> {
  const before = events().find((e) => e.id === id) ?? null;
  await deleteEvent(id);
  setEvents((prev) => prev.filter((e) => e.id !== id));
  setSelected(null);
  if (before) {
    recordCalendarUndo({ label: `Deleted "${before.title}"`, event: before });
  }
}

// -- P1.4 calendar undo (delete / edit / move / resize) ------------------

export type CalendarUndo = {
  label: string;
  /** The pre-edit / deleted event snapshot — restores via `restore_event`. */
  event?: CalendarEvent;
  /** A create: the id to delete on undo. */
  createdEventId?: number;
};

const [calendarUndo, setCalendarUndo] = createSignal<CalendarUndo | null>(null);

export function useCalendarUndo(): () => CalendarUndo | null {
  return calendarUndo;
}

export function clearCalendarUndo(): void {
  setCalendarUndo(null);
}

/** Record a calendar change for the undo bar (also called by CalendarView for
 * drag/resize/modal-create, which go through the adapter). */
export function recordCalendarUndo(record: CalendarUndo): void {
  setCalendarUndo(record);
}

/** Restore the last calendar change: re-create/overwrite the event, or remove
 * a just-created one, then refresh the in-memory list (the views re-render). */
export async function undoCalendar(): Promise<void> {
  const u = calendarUndo();
  if (!u) return;
  setCalendarUndo(null);
  if (u.createdEventId != null) {
    await deleteEvent(u.createdEventId);
    setEvents((prev) => prev.filter((e) => e.id !== u.createdEventId));
    setSelected(null);
    return;
  }
  if (u.event) {
    await restoreEvent(u.event);
    const restored = u.event;
    setEvents((prev) => {
      const exists = prev.some((e) => e.id === restored.id);
      return exists
        ? prev.map((e) => (e.id === restored.id ? restored : e))
        : [...prev, restored];
    });
    setSelected(restored);
  }
}
