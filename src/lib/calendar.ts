import { createSignal } from "solid-js";
import type { CalendarEvent } from "./ipc/CalendarEvent";
import { createEvent, deleteEvent, listEvents, updateEvent } from "./tauri";

// Calendar state (Epic 14): events loaded from the store for the visible
// range, plus the selection. Recurrence is expanded in Rust (Epic 14.1) — the
// frontend only ever sees resolved instances, so this is a plain list. The
// view reloads after any mutation.

const [events, setEvents] = createSignal<CalendarEvent[]>([]);
const [selected, setSelected] = createSignal<CalendarEvent | null>(null);

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
  await updateEvent(event);
  setSelected(event);
}

export async function createNewEvent(event: CalendarEvent): Promise<void> {
  await createEvent(event);
}

export async function removeEvent(id: number): Promise<void> {
  await deleteEvent(id);
  setSelected(null);
}
