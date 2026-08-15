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
import { createSignal, untrack } from "solid-js";
import type { CalendarEvent } from "./ipc/CalendarEvent";
import type { CalendarTask } from "./ipc/CalendarTask";
import {
  createEvent,
  deleteEvent,
  listCalendars,
  listEvents,
  listTasks as listTasksCmd,
  toggleTask as toggleTaskCmd,
  updateEvent,
} from "./tauri";
import { useAccounts } from "./mail";

const DISABLED_CALS_KEY = "quill_disabled_calendars";

function loadDisabledCalendarIds(): Set<string> {
  try {
    const raw = localStorage.getItem(DISABLED_CALS_KEY);
    if (!raw) return new Set();
    return new Set(JSON.parse(raw));
  } catch {
    return new Set();
  }
}

// Reactive, so the calendar sidebar's show/hide toggles update the views
// immediately (calendars() + the occurrence filter both track this). The set
// is persisted to localStorage so the choice survives restarts.
const [disabledCalendarIds, setDisabledCalendarIdsSignal] = createSignal<
  Set<string>
>(loadDisabledCalendarIds());

/** Reactive set of currently-hidden calendar ids. */
export function getDisabledCalendarIds(): Set<string> {
  return disabledCalendarIds();
}

/** Show/hide a calendar by id; updates the reactive set immediately. */
export function setCalendarEnabled(calendarId: string, enabled: boolean): void {
  setDisabledCalendarId(calendarId, !enabled);
}

function setDisabledCalendarId(id: string, disabled: boolean) {
  // Snapshot read on a write path — no reason to track.
  const next = new Set(untrack(disabledCalendarIds));
  if (disabled) {
    next.add(id);
  } else {
    next.delete(id);
  }
  setDisabledCalendarIdsSignal(next);
  try {
    localStorage.setItem(DISABLED_CALS_KEY, JSON.stringify(Array.from(next)));
  } catch {
    // Ignore localStorage errors
  }
}

const HIDDEN_SIDEBAR_KEY = "quill_hidden_from_sidebar";

function loadHiddenFromSidebar(): Set<string> {
  try {
    const raw = localStorage.getItem(HIDDEN_SIDEBAR_KEY);
    if (!raw) return new Set();
    return new Set(JSON.parse(raw));
  } catch {
    return new Set();
  }
}

// Calendars the user removed from the *sidebar* only — their data stays synced
// and they remain manageable in Settings (unlike a full removal). Also
// persisted, and reactive like the disabled set.
const [hiddenFromSidebar, setHiddenFromSidebarSignal] = createSignal<
  Set<string>
>(loadHiddenFromSidebar());

/** Reactive set of calendar ids hidden from the sidebar (still in Settings). */
export function getHiddenFromSidebarIds(): Set<string> {
  return hiddenFromSidebar();
}

/** Remove a calendar from the sidebar (or bring it back). */
export function setHiddenFromSidebar(
  calendarId: string,
  hidden: boolean,
): void {
  const next = new Set(untrack(hiddenFromSidebar));
  if (hidden) {
    next.add(calendarId);
  } else {
    next.delete(calendarId);
  }
  setHiddenFromSidebarSignal(next);
  try {
    localStorage.setItem(HIDDEN_SIDEBAR_KEY, JSON.stringify(Array.from(next)));
  } catch {
    // Ignore localStorage errors
  }
}

/** True when a calendar's events should be hidden from the views: disabled via
 * the show/hide toggle OR removed from the sidebar. */
export function isCalendarHidden(calendarId: string): boolean {
  return (
    getDisabledCalendarIds().has(calendarId) ||
    getHiddenFromSidebarIds().has(calendarId)
  );
}

/** Calendar id for an account (and optional source calendar, e.g. a Google
 * calendar id). Source calendars get their own id so they render and toggle
 * independently: `cal-1` (account) vs `cal-1:personal@google.com` (source). */
export function calendarIdFor(
  accountId: number,
  source?: string | null,
): string {
  return source ? `cal-${accountId}:${source}` : `cal-${accountId}`;
}

/** The calendar id an event belongs to — its source calendar if it has one. */
export function eventCalendarId(event: CalendarEvent): string {
  return calendarIdFor(event.account_id, event.calendar_source);
}

/** Split a calendar id back into its account id and optional source. */
export function parseCalendarId(calId: string): {
  accountId: number;
  source?: string;
} {
  const rest = calId.replace(/^cal-/, "");
  const idx = rest.indexOf(":");
  if (idx === -1) return { accountId: Number(rest) || 1 };
  return {
    accountId: Number(rest.slice(0, idx)) || 1,
    source: rest.slice(idx + 1),
  };
}

/** Name/color for a source calendar, resolved from the store's source list. */
async function sourceMeta(
  accountId: number,
  source: string | null,
): Promise<{ name: string | null; color: string | null }> {
  if (!source) return { name: null, color: null };
  const sources = await listCalendars();
  const match = sources.find(
    (s) => s.accountId === accountId && s.source === source,
  );
  return {
    name: match?.name ?? source.split("@")[0] ?? source,
    color: match?.color ?? "#3b5bdb",
  };
}

/** Map a stored task to the rcalendar `Task` shape (P0.4). */
export function quillTaskToDomain(t: CalendarTask): Task {
  const now = new Date().toISOString();
  return {
    id: String(t.id),
    calendarId: calendarIdFor(t.accountId, null),
    title: t.title,
    dueAt: t.dueAtMs ? new Date(t.dueAtMs).toISOString() : null,
    completedAt: t.completedAtMs
      ? new Date(t.completedAtMs).toISOString()
      : null,
    createdAt: now,
    updatedAt: now,
  };
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
    tz: event.timezone || null,
    travelTimeMinutes: event.travel_time_minutes || null,
    color: event.color || null,
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
  return Date.now();
}

export class QuillCalendarDataSource implements CalendarDataSource {
  async listAccounts(): Promise<{ account: Account; calendars: Calendar[] }[]> {
    const accounts = useAccounts()();
    const disabled = untrack(getDisabledCalendarIds);

    return accounts.map((acc) => {
      const calId = calendarIdFor(acc.id);
      return {
        account: {
          id: String(acc.id),
          kind:
            acc.protocol.toLowerCase() === "google"
              ? "google"
              : acc.protocol.toLowerCase() === "caldav"
                ? "caldav"
                : "local",
          displayName: acc.address,
          detail: `${acc.protocol} · ${acc.server || "localhost"}`,
          status: acc.connected ? "idle" : "error",
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
        },
        calendars: [
          {
            id: calId,
            accountId: String(acc.id),
            name: acc.address.split("@")[0] || "Personal",
            color: acc.color,
            enabled: !disabled.has(calId),
            eventCount: 0,
          },
        ],
      };
    });
  }

  async listCalendars(): Promise<Calendar[]> {
    const accountList = await this.listAccounts();
    return accountList.flatMap((a) => a.calendars);
  }

  async setCalendarEnabled(
    calendarId: string,
    enabled: boolean,
  ): Promise<void> {
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
    // Snapshot the hidden sets — async reads shouldn't create subscriptions.
    const disabled = untrack(getDisabledCalendarIds);
    const hiddenSidebar = untrack(getHiddenFromSidebarIds);

    const results: OccurrenceItem[] = [];
    for (const e of rawEvents) {
      const calId = eventCalendarId(e);
      if (disabled.has(calId) || hiddenSidebar.has(calId)) continue;
      if (calendarIds && calendarIds.length > 0 && !calendarIds.includes(calId))
        continue;
      results.push(quillEventToDomain(e, calId));
    }
    return results;
  }

  /** The stored reminder for an event, or null if it has none (or can't be
   * found in the ±90-day window). */
  private async alarmMinutesFor(id: number): Promise<number | null> {
    const now = Date.now();
    const events = await listEvents(now - 90 * 86400000, now + 90 * 86400000);
    return events.find((e) => e.id === id)?.alarm_minutes_before ?? null;
  }

  async getEvent(id: string): Promise<Event | null> {
    const numId = Number(id);
    const now = Date.now();
    const events = await listEvents(now - 90 * 86400000, now + 90 * 86400000);
    const found = events.find((e) => e.id === numId);
    if (!found) return null;
    return quillEventToDomain(found, eventCalendarId(found)).event;
  }

  async saveEvent(
    draft: EventDraft,
    id?: string,
    _scope?: EditScope,
    _targetDate?: string,
  ): Promise<Event[]> {
    const startMs = new Date(draft.startsAt).getTime();
    const endMs = new Date(draft.endsAt).getTime();
    const { accountId, source } = parseCalendarId(draft.calendarId);

    // Tag the event with its source calendar (if any) and keep that calendar's
    // name/color so the sidebar row survives edits — and new events created in
    // a synced calendar land in the right row.
    const { name: calName, color: calColor } = await sourceMeta(
      accountId,
      source ?? null,
    );

    const base: Omit<CalendarEvent, "alarm_minutes_before"> = {
      id: id ? Number(id) : 0,
      account_id: accountId,
      title: draft.title,
      start_ms: startMs,
      end_ms: endMs,
      all_day: draft.allDay,
      location: draft.location || null,
      notes: draft.notes || null,
      timezone: draft.tz || null,
      travel_time_minutes: draft.travelTimeMinutes || null,
      calendar_source: source || null,
      calendar_name: calName,
      calendar_color: calColor,
      color: draft.color || null,
    };

    if (id) {
      const existingAlarm = await this.alarmMinutesFor(Number(id));
      const updated: CalendarEvent = {
        ...base,
        alarm_minutes_before: existingAlarm,
      };
      await updateEvent(updated);
      return [quillEventToDomain(updated, draft.calendarId).event];
    } else {
      const newEvt: CalendarEvent = { ...base, alarm_minutes_before: null };
      const created = await createEvent(newEvt);
      return [quillEventToDomain(created, draft.calendarId).event];
    }
  }

  async deleteEvent(id: string): Promise<Event[]> {
    await deleteEvent(Number(id));
    return [];
  }

  async listTasks(): Promise<Task[]> {
    const raw = await listTasksCmd();
    return raw.map(quillTaskToDomain);
  }

  async toggleTask(id: string): Promise<Task> {
    const t = await toggleTaskCmd(Number(id));
    return quillTaskToDomain(t);
  }

  async search(query: string): Promise<SearchResults> {
    const now = Date.now();
    const rawEvents = await listEvents(
      now - 180 * 86400000,
      now + 180 * 86400000,
    );
    const q = query.toLowerCase().trim();
    if (!q) return { events: [], tasks: [] };

    const filtered = rawEvents
      .filter(
        (e) =>
          e.title.toLowerCase().includes(q) ||
          (e.location && e.location.toLowerCase().includes(q)) ||
          (e.notes && e.notes.toLowerCase().includes(q)),
      )
      .map((e) => quillEventToDomain(e, eventCalendarId(e)).event);
    return { events: filtered, tasks: [] };
  }

  async exportIcs(calendarId?: string): Promise<string> {
    const now = Date.now();
    const rawEvents = await listEvents(
      now - 365 * 86400000,
      now + 365 * 86400000,
    );
    const target = calendarId ? parseCalendarId(calendarId) : null;
    const eventsToExport = target
      ? rawEvents.filter(
          (e) =>
            e.account_id === target.accountId &&
            (!target.source || e.calendar_source === target.source),
        )
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
        lines.push(
          `DTSTART;VALUE=DATE:${formatDateToIcs(new Date(e.start_ms).toISOString(), true)}`,
        );
        lines.push(
          `DTEND;VALUE=DATE:${formatDateToIcs(new Date(e.end_ms).toISOString(), true)}`,
        );
      } else {
        lines.push(
          `DTSTART:${formatDateToIcs(new Date(e.start_ms).toISOString())}`,
        );
        lines.push(
          `DTEND:${formatDateToIcs(new Date(e.end_ms).toISOString())}`,
        );
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
    const parsed = calendarId ? parseCalendarId(calendarId) : null;
    const defaultAccountId = parsed?.accountId || accounts[0]?.id || 1;
    const targetSource = parsed?.source ?? null;
    const targetCalId = calendarIdFor(defaultAccountId, targetSource);
    const { name: calName, color: calColor } = await sourceMeta(
      defaultAccountId,
      targetSource,
    );

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
        alarm_minutes_before: null,
        timezone: null,
        travel_time_minutes: null,
        calendar_source: targetSource,
        calendar_name: calName,
        calendar_color: calColor,
        color: null,
      };

      const created = await createEvent(newEvt);
      importedEvents.push(quillEventToDomain(created, targetCalId).event);
    }

    return importedEvents;
  }

  async addAccount(_spec: {
    kind: AccountKind;
    displayName: string;
    detail: string;
  }): Promise<{ account: Account; calendars: Calendar[] }> {
    throw new Error("Add account via Quill settings");
  }

  async connectGoogleAccount(
    _email: string,
    _token: string,
  ): Promise<{ account: Account; calendars: Calendar[] }> {
    throw new Error("Connect Google account via Quill settings");
  }

  async syncAccount(_accountId: string): Promise<{
    accountId: string;
    syncedAt: string;
    success: boolean;
    message: string;
  }> {
    return {
      accountId: _accountId,
      syncedAt: new Date().toISOString(),
      success: true,
      message: "Quill synchronized",
    };
  }

  async setSyncInterval(_minutes: number): Promise<void> {}
}
