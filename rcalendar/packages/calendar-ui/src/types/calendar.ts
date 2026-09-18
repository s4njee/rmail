/**
 * Shared calendar types and the `CalendarDataSource` seam.
 *
 * These mirror the Rust model in `calendar-core` (plan.md §5) so one shape
 * spans the whole app. `calendar-ui` must never import `@tauri-apps/api`:
 * hosts implement `CalendarDataSource` and inject their own backend — the
 * desktop app wires it to Tauri commands (M3), another web app to its own API.
 */

export type ViewMode = "Month" | "Week" | "3-day" | "Day" | "Agenda" | "Year";

export type AccountKind = "local" | "google" | "caldav";
export type AccountStatus = "idle" | "syncing" | "error";
export type EditScope = "this" | "future" | "all";

/** An invitee's role (RFC 5545 `ROLE`). */
export type AttendeeRole = "chair" | "required" | "optional" | "non_participant";
/** An invitee's RSVP status (RFC 5545 `PARTSTAT`). */
export type AttendeeStatus = "needs_action" | "accepted" | "declined" | "tentative" | "delegated";

/** An Account owning calendars. */
export interface Account {
  id: string;
  kind: AccountKind;
  displayName: string;
  detail: string;
  lastSyncedAt?: string | null;
  status: AccountStatus;
  createdAt: string;
  updatedAt: string;
  deletedAt?: string | null;
}

/** A calendar (a collection of events). */
export interface Calendar {
  id: string;
  accountId: string;
  name: string;
  color: string;
  enabled: boolean;
  eventCount: number;
  /** Read-only calendars (subscriptions, shared) can't be edited (P1.4). */
  readOnly?: boolean;
  createdAt?: string;
  updatedAt?: string;
}

/** A calendar event. */
export interface Event {
  id: string;
  calendarId: string;
  uid: string;
  title: string;
  location?: string | null;
  notes?: string | null;
  startsAt: string; // ISO 8601 UTC
  endsAt: string; // ISO 8601 UTC
  allDay: boolean;
  tz?: string | null;
  rrule?: string | null;
  exdates?: string[];
  travelTimeMinutes?: number | null;
  /** Per-event color override — falls back to the calendar color (P1.4). */
  color?: string | null;
  etag?: string | null;
  /** RFC 5545 TRANSP: true = busy (opaque), false = free (transparent). */
  busy?: boolean;
  createdAt?: string;
  updatedAt?: string;
}

/** An expanded occurrence of an event. */
export interface Occurrence {
  eventId: string;
  startsAt: string;
  endsAt: string;
  allDay: boolean;
}

/** An occurrence paired with its parent event. */
export interface OccurrenceItem {
  occurrence: Occurrence;
  event: Event;
}

/** A reminder on an event. */
export interface Reminder {
  id: string;
  eventId: string;
  offsetMinutes?: number | null;
  absoluteAt?: string | null;
  createdAt?: string;
  updatedAt?: string;
}

/** A person on an event (invitee or organizer). */
export interface Attendee {
  id: string;
  eventId: string;
  email: string;
  displayName?: string | null;
  role: AttendeeRole;
  status: AttendeeStatus;
  rsvp: boolean;
  isOrganizer: boolean;
  createdAt?: string;
  updatedAt?: string;
}

/** Payload for creating/updating an attendee (the id is optional for creates). */
export interface AttendeeDraft {
  id?: string;
  eventId: string;
  email: string;
  displayName?: string | null;
  role?: AttendeeRole;
  status?: AttendeeStatus;
  rsvp?: boolean;
  isOrganizer?: boolean;
}

/** Payload for creating/updating a reminder (the id is optional for creates). */
export interface ReminderDraft {
  id?: string;
  eventId: string;
  offsetMinutes?: number | null;
  absoluteAt?: string | null;
}

/** Default alert offsets (minutes before start; negative = before, 0 = at start). */
export interface DefaultAlerts {
  event: number | null;
  allDay: number | null;
}

/** A task / to-do item. */
export interface Task {
  id: string;
  calendarId: string;
  title: string;
  dueAt?: string | null;
  completedAt?: string | null;
  createdAt: string;
  updatedAt: string;
}

/** Payload for saving/updating an event from the UI editor. */
export interface EventDraft {
  calendarId: string;
  title: string;
  location?: string | null;
  notes?: string | null;
  startsAt: string;
  endsAt: string;
  allDay: boolean;
  tz?: string | null;
  rrule?: string | null;
  travelTimeMinutes?: number | null;
  /** Per-event color override (P1.4). */
  color?: string | null;
  /** RFC 5545 TRANSP. Defaults to busy. */
  busy?: boolean;
}

/** Local identity used to hide declined invitations. */
export interface IdentitySettings {
  selfEmail?: string | null;
  showDeclined: boolean;
}

/** A free meeting slot returned by "Find a time". */
export interface AvailableSlot {
  start: string;
  end: string;
}

/** Search results payload. */
export interface SearchResults {
  events: Event[];
  tasks: Task[];
  matchedDate?: string | null;
}

/**
 * The embeddability seam for `calendar-ui` (plan.md §8).
 *
 * Views render against this interface only; hosts supply the implementation.
 * The Tauri-backed adapter lives in `apps/desktop`, never in this package.
 */
export interface CalendarDataSource {
  listAccounts(): Promise<{ account: Account; calendars: Calendar[] }[]>;
  listCalendars(): Promise<Calendar[]>;
  setCalendarEnabled(calendarId: string, enabled: boolean): Promise<void>;
  deleteAccount(accountId: string): Promise<void>;
  deleteCalendar(calendarId: string): Promise<void>;
  listOccurrences(from: string, to: string, calendarIds?: string[]): Promise<OccurrenceItem[]>;
  getEvent(id: string): Promise<Event | null>;
  saveEvent(
    draft: EventDraft,
    id?: string,
    scope?: EditScope,
    targetDate?: string,
  ): Promise<Event[]>;
  deleteEvent(id: string, scope?: EditScope, targetDate?: string): Promise<Event[]>;
  listTasks(from?: string, to?: string): Promise<Task[]>;
  toggleTask(id: string): Promise<Task>;
  listReminders(eventId: string): Promise<Reminder[]>;
  saveReminder(draft: ReminderDraft): Promise<Reminder>;
  deleteReminder(id: string): Promise<void>;
  snoozeReminder(id: string, minutes: number): Promise<void>;
  listAttendees(eventId: string): Promise<Attendee[]>;
  saveAttendee(draft: AttendeeDraft): Promise<Attendee>;
  deleteAttendee(id: string): Promise<void>;
  suggestAttendees(query: string): Promise<Attendee[]>;
  findAvailableSlots(
    date: string,
    durationMinutes: number,
    calendarIds?: string[],
  ): Promise<AvailableSlot[]>;
  sendInvitations(eventId: string): Promise<{
    subject: string;
    recipients: string[];
    outboxPath?: string | null;
  }>;
  applyItip(
    calendarId: string,
    icsContent: string,
  ): Promise<{ method: string; uid: string; message: string }>;
  getIdentity(): Promise<IdentitySettings>;
  setIdentity(identity: IdentitySettings): Promise<void>;
  getDefaultAlerts(): Promise<DefaultAlerts>;
  setDefaultAlerts(alerts: DefaultAlerts): Promise<void>;
  search(query: string): Promise<SearchResults>;
  exportIcs(calendarId?: string): Promise<string>;
  importIcs(calendarId: string, icsContent: string): Promise<Event[]>;
  addAccount(spec: {
    kind: AccountKind;
    displayName: string;
    detail: string;
  }): Promise<{ account: Account; calendars: Calendar[] }>;
  connectGoogleAccount(
    email: string,
    token: string,
  ): Promise<{ account: Account; calendars: Calendar[] }>;
  syncAccount(
    accountId: string,
  ): Promise<{ accountId: string; syncedAt: string; success: boolean; message: string }>;
  setSyncInterval(minutes: number): Promise<void>;
}
