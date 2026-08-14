import type {
  Account,
  AccountKind,
  Calendar,
  CalendarDataSource,
  EditScope,
  Event,
  EventDraft,
  OccurrenceItem,
  SearchResults,
  Task,
} from "@rcalendar/ui";
import type { CalendarEvent } from "./ipc/CalendarEvent";
import { createEvent, deleteEvent, listEvents, updateEvent } from "./tauri";
import { useAccounts } from "./mail";

const DISABLED_CALS_KEY = "quill_disabled_calendars";

function getDisabledCalendarIds(): Set<string> {
  try {
    const raw = localStorage.getItem(DISABLED_CALS_KEY);
    if (!raw) return new Set();
    return new Set(JSON.parse(raw));
  } catch {
    return new Set();
  }
}

function setDisabledCalendarId(id: string, disabled: boolean) {
  try {
    const set = getDisabledCalendarIds();
    if (disabled) {
      set.add(id);
    } else {
      set.delete(id);
    }
    localStorage.setItem(DISABLED_CALS_KEY, JSON.stringify(Array.from(set)));
  } catch {
    // Ignore localStorage errors
  }
}

export function quillEventToDomain(
  event: CalendarEvent,
  calendarId: string,
): { occurrence: OccurrenceItem["occurrence"]; event: Event } {
  const startsAt = new Date(event.start_ms).toISOString();
  const endsAt = new Date(event.end_ms).toISOString();
  const idStr = String(event.id);

  const domainEvent: Event = {
    id: idStr,
    calendarId,
    uid: `quill-${event.id}`,
    title: event.title,
    location: event.location,
    notes: event.notes,
    startsAt,
    endsAt,
    allDay: event.all_day,
    exdates: [],
    createdAt: startsAt,
    updatedAt: startsAt,
  };

  const occurrence = {
    id: `occ-${event.id}`,
    eventId: idStr,
    calendarId,
    startsAt,
    endsAt,
    allDay: event.all_day,
  };

  return { occurrence, event: domainEvent };
}

function formatDateToIcs(isoStr: string, allDay = false): string {
  const d = new Date(isoStr);
  if (allDay) {
    const y = d.getUTCFullYear();
    const m = String(d.getUTCMonth() + 1).padStart(2, "0");
    const day = String(d.getUTCDate()).padStart(2, "0");
    return `${y}${m}${day}`;
  }
  const y = d.getUTCFullYear();
  const m = String(d.getUTCMonth() + 1).padStart(2, "0");
  const day = String(d.getUTCDate()).padStart(2, "0");
  const h = String(d.getUTCHours()).padStart(2, "0");
  const min = String(d.getUTCMinutes()).padStart(2, "0");
  const s = String(d.getUTCSeconds()).padStart(2, "0");
  return `${y}${m}${day}T${h}${min}${s}Z`;
}

function parseIcsDate(raw: string): number {
  const clean = raw.trim().replace(/[^0-9TZ]/g, "");
  if (clean.length === 8) {
    const y = Number(clean.slice(0, 4));
    const m = Number(clean.slice(4, 6)) - 1;
    const d = Number(clean.slice(6, 8));
    return Date.UTC(y, m, d, 0, 0, 0);
  }
  if (clean.length >= 15) {
    const y = Number(clean.slice(0, 4));
    const m = Number(clean.slice(4, 6)) - 1;
    const d = Number(clean.slice(6, 8));
    const h = Number(clean.slice(9, 11));
    const min = Number(clean.slice(11, 13));
    const s = Number(clean.slice(13, 15));
    return Date.UTC(y, m, d, h, min, s);
  }
  const parsed = Date.parse(raw);
  return Number.isNaN(parsed) ? Date.now() : parsed;
}

export class QuillCalendarDataSource implements CalendarDataSource {
  async listAccounts(): Promise<{ account: Account; calendars: Calendar[] }[]> {
    const rawAccounts = useAccounts()();
    const disabled = getDisabledCalendarIds();

    return rawAccounts.map((acc) => {
      const calId = `cal-${acc.id}`;
      const calendar: Calendar = {
        id: calId,
        accountId: String(acc.id),
        name: acc.address,
        color: acc.color || "#3b5bdb",
        enabled: !disabled.has(calId),
        eventCount: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      };
      const account: Account = {
        id: String(acc.id),
        displayName: acc.address,
        kind: "local",
        status: "idle",
        detail: acc.address,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      };
      return { account, calendars: [calendar] };
    });
  }

  async listCalendars(): Promise<Calendar[]> {
    const rawAccounts = useAccounts()();
    const disabled = getDisabledCalendarIds();

    return rawAccounts.map((acc) => {
      const calId = `cal-${acc.id}`;
      return {
        id: calId,
        accountId: String(acc.id),
        name: acc.address,
        color: acc.color || "#3b5bdb",
        enabled: !disabled.has(calId),
        eventCount: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      };
    });
  }

  async setCalendarEnabled(calendarId: string, enabled: boolean): Promise<void> {
    setDisabledCalendarId(calendarId, !enabled);
  }

  async listOccurrences(
    from: string,
    to: string,
    calendarIds?: string[],
  ): Promise<OccurrenceItem[]> {
    const startMs = new Date(from).getTime();
    const endMs = new Date(to).getTime();
    const rawEvents = await listEvents(startMs, endMs);
    const accounts = useAccounts()();
    const disabled = getDisabledCalendarIds();

    const results: OccurrenceItem[] = [];
    for (const e of rawEvents) {
      const calId = `cal-${e.account_id || accounts[0]?.id || 1}`;
      if (disabled.has(calId)) continue;
      if (calendarIds && calendarIds.length > 0 && !calendarIds.includes(calId)) continue;
      results.push(quillEventToDomain(e, calId));
    }
    return results;
  }

  async getEvent(id: string): Promise<Event | null> {
    const numId = Number(id);
    const now = Date.now();
    const events = await listEvents(now - 90 * 86400000, now + 90 * 86400000);
    const found = events.find((e) => e.id === numId);
    if (!found) return null;
    return quillEventToDomain(found, `cal-${found.account_id}`).event;
  }

  async saveEvent(
    draft: EventDraft,
    id?: string,
    _scope?: EditScope,
    _targetDate?: string,
  ): Promise<Event[]> {
    const startMs = new Date(draft.startsAt).getTime();
    const endMs = new Date(draft.endsAt).getTime();
    const accountId = Number(draft.calendarId.replace("cal-", "")) || 1;

    if (id) {
      const numId = Number(id);
      const updated: CalendarEvent = {
        id: numId,
        account_id: accountId,
        title: draft.title,
        start_ms: startMs,
        end_ms: endMs,
        all_day: draft.allDay,
        location: draft.location || null,
        notes: draft.notes || null,
      };
      await updateEvent(updated);
      return [quillEventToDomain(updated, draft.calendarId).event];
    } else {
      const newEvt: CalendarEvent = {
        id: 0,
        account_id: accountId,
        title: draft.title,
        start_ms: startMs,
        end_ms: endMs,
        all_day: draft.allDay,
        location: draft.location || null,
        notes: draft.notes || null,
      };
      const created = await createEvent(newEvt);
      return [quillEventToDomain(created, draft.calendarId).event];
    }
  }

  async deleteEvent(id: string): Promise<Event[]> {
    await deleteEvent(Number(id));
    return [];
  }

  async listTasks(): Promise<Task[]> {
    return [];
  }

  async toggleTask(_id: string): Promise<Task> {
    throw new Error("Tasks not implemented in Quill");
  }

  async search(query: string): Promise<SearchResults> {
    const now = Date.now();
    const rawEvents = await listEvents(now - 180 * 86400000, now + 180 * 86400000);
    const q = query.toLowerCase().trim();
    if (!q) return { events: [], tasks: [] };

    const filtered = rawEvents
      .filter(
        (e) =>
          e.title.toLowerCase().includes(q) ||
          (e.location && e.location.toLowerCase().includes(q)) ||
          (e.notes && e.notes.toLowerCase().includes(q)),
      )
      .map((e) => quillEventToDomain(e, `cal-${e.account_id}`).event);
    return { events: filtered, tasks: [] };
  }

  async exportIcs(calendarId?: string): Promise<string> {
    const now = Date.now();
    const rawEvents = await listEvents(now - 365 * 86400000, now + 365 * 86400000);
    const targetAccountId = calendarId ? Number(calendarId.replace("cal-", "")) : null;
    const eventsToExport = targetAccountId
      ? rawEvents.filter((e) => e.account_id === targetAccountId)
      : rawEvents;

    const lines: string[] = [
      "BEGIN:VCALENDAR",
      "VERSION:2.0",
      "PRODID:-//Quill//EN",
      "CALSCALE:GREGORIAN",
    ];

    for (const e of eventsToExport) {
      lines.push("BEGIN:VEVENT");
      lines.push(`UID:quill-${e.id}@quill.local`);
      lines.push(`DTSTAMP:${formatDateToIcs(new Date().toISOString())}`);
      if (e.all_day) {
        lines.push(`DTSTART;VALUE=DATE:${formatDateToIcs(new Date(e.start_ms).toISOString(), true)}`);
        lines.push(`DTEND;VALUE=DATE:${formatDateToIcs(new Date(e.end_ms).toISOString(), true)}`);
      } else {
        lines.push(`DTSTART:${formatDateToIcs(new Date(e.start_ms).toISOString())}`);
        lines.push(`DTEND:${formatDateToIcs(new Date(e.end_ms).toISOString())}`);
      }
      lines.push(`SUMMARY:${e.title.replace(/\n/g, " ")}`);
      if (e.location) lines.push(`LOCATION:${e.location.replace(/\n/g, " ")}`);
      if (e.notes) lines.push(`DESCRIPTION:${e.notes.replace(/\n/g, "\\n")}`);
      lines.push("END:VEVENT");
    }

    lines.push("END:VCALENDAR");
    return lines.join("\r\n");
  }

  async importIcs(calendarId: string, icsText: string): Promise<Event[]> {
    const accounts = useAccounts()();
    const defaultAccountId = Number(calendarId?.replace("cal-", "")) || accounts[0]?.id || 1;
    const targetCalId = `cal-${defaultAccountId}`;

    const importedEvents: Event[] = [];
    const veventRegex = /BEGIN:VEVENT[\s\S]*?END:VEVENT/gi;
    const matches = icsText.match(veventRegex) || [];

    for (const block of matches) {
      let title = "Untitled Event";
      let location: string | null = null;
      let notes: string | null = null;
      let startMs = Date.now();
      let endMs = startMs + 3600000;
      let allDay = false;

      const summaryMatch = block.match(/SUMMARY(?::|;[^:]*:)(.*)/i);
      if (summaryMatch) title = summaryMatch[1].trim();

      const locMatch = block.match(/LOCATION(?::|;[^:]*:)(.*)/i);
      if (locMatch) location = locMatch[1].trim();

      const descMatch = block.match(/DESCRIPTION(?::|;[^:]*:)(.*)/i);
      if (descMatch) notes = descMatch[1].trim().replace(/\\n/g, "\n");

      const dtStartMatch = block.match(/DTSTART([^:]*):([^\r\n]+)/i);
      if (dtStartMatch) {
        if (dtStartMatch[1].includes("VALUE=DATE")) allDay = true;
        startMs = parseIcsDate(dtStartMatch[2]);
      }

      const dtEndMatch = block.match(/DTEND([^:]*):([^\r\n]+)/i);
      if (dtEndMatch) {
        endMs = parseIcsDate(dtEndMatch[2]);
      } else {
        endMs = allDay ? startMs + 86400000 : startMs + 3600000;
      }

      const newEvt: CalendarEvent = {
        id: 0,
        account_id: defaultAccountId,
        title,
        start_ms: startMs,
        end_ms: endMs,
        all_day: allDay,
        location,
        notes,
      };

      const created = await createEvent(newEvt);
      importedEvents.push(quillEventToDomain(created, targetCalId).event);
    }

    return importedEvents;
  }

  async addAccount(
    _spec: { kind: AccountKind; displayName: string; detail: string },
  ): Promise<{ account: Account; calendars: Calendar[] }> {
    throw new Error("Add account via Quill settings");
  }

  async connectGoogleAccount(
    _email: string,
    _token: string,
  ): Promise<{ account: Account; calendars: Calendar[] }> {
    throw new Error("Connect Google account via Quill settings");
  }

  async syncAccount(
    _accountId: string,
  ): Promise<{ accountId: string; syncedAt: string; success: boolean; message: string }> {
    return {
      accountId: _accountId,
      syncedAt: new Date().toISOString(),
      success: true,
      message: "Quill synchronized",
    };
  }

  async setSyncInterval(_minutes: number): Promise<void> {}
}
