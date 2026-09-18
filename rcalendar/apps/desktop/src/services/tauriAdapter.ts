/**
 * Tauri-backed implementation of [`CalendarDataSource`].
 *
 * This adapter lives in `apps/desktop` (the third layer), so `calendar-ui`
 * never imports `@tauri-apps/api` (plan.md §8).
 */

import { invoke } from "@tauri-apps/api/core";
import {
  Account,
  AccountKind,
  Attendee,
  AttendeeDraft,
  Calendar,
  CalendarDataSource,
  AvailableSlot,
  DefaultAlerts,
  EditScope,
  IdentitySettings,
  Event,
  EventDraft,
  OccurrenceItem,
  Reminder,
  ReminderDraft,
  SearchResults,
  Task,
} from "@rcalendar/ui";

interface RustAccount {
  id: string;
  kind: "local" | "google" | "caldav";
  display_name: string;
  detail: string;
  last_synced_at?: string | null;
  status: "idle" | "syncing" | "error";
  created_at: string;
  updated_at: string;
  deleted_at?: string | null;
}

interface RustCalendar {
  id: string;
  account_id: string;
  name: string;
  color: string;
  enabled: boolean;
  event_count: number;
  created_at?: string;
  updated_at?: string;
}

interface RustEvent {
  id: string;
  calendar_id: string;
  uid: string;
  title: string;
  location?: string | null;
  notes?: string | null;
  starts_at: string;
  ends_at: string;
  all_day: boolean;
  tz?: string | null;
  rrule?: string | null;
  exdates?: string[];
  travel_time_minutes?: number | null;
  color?: string | null;
  etag?: string | null;
  busy?: boolean;
  created_at?: string;
  updated_at?: string;
}

interface RustOccurrenceItem {
  occurrence: {
    event_id: string;
    starts_at: string;
    ends_at: string;
    all_day: boolean;
  };
  event: RustEvent;
}

interface RustTask {
  id: string;
  calendar_id: string;
  title: string;
  due_at?: string | null;
  completed_at?: string | null;
  created_at: string;
  updated_at: string;
}

interface RustReminder {
  id: string;
  event_id: string;
  offset_minutes?: number | null;
  absolute_at?: string | null;
  created_at?: string;
  updated_at?: string;
}

interface RustAttendee {
  id: string;
  event_id: string;
  email: string;
  display_name?: string | null;
  role: "chair" | "required" | "optional" | "non_participant";
  status: "needs_action" | "accepted" | "declined" | "tentative" | "delegated";
  rsvp: boolean;
  is_organizer: boolean;
  created_at?: string;
  updated_at?: string;
}

function mapAccount(r: RustAccount): Account {
  return {
    id: r.id,
    kind: r.kind,
    displayName: r.display_name,
    detail: r.detail,
    lastSyncedAt: r.last_synced_at,
    status: r.status,
    createdAt: r.created_at,
    updatedAt: r.updated_at,
    deletedAt: r.deleted_at,
  };
}

function mapCalendar(r: RustCalendar): Calendar {
  return {
    id: r.id,
    accountId: r.account_id,
    name: r.name,
    color: r.color,
    enabled: r.enabled,
    eventCount: r.event_count,
    createdAt: r.created_at,
    updatedAt: r.updated_at,
  };
}

function mapEvent(r: RustEvent): Event {
  return {
    id: r.id,
    calendarId: r.calendar_id,
    uid: r.uid,
    title: r.title,
    location: r.location,
    notes: r.notes,
    startsAt: r.starts_at,
    endsAt: r.ends_at,
    allDay: r.all_day,
    tz: r.tz,
    rrule: r.rrule,
    exdates: r.exdates,
    travelTimeMinutes: r.travel_time_minutes,
    color: r.color,
    etag: r.etag,
    busy: r.busy !== false,
    createdAt: r.created_at,
    updatedAt: r.updated_at,
  };
}

function mapTask(r: RustTask): Task {
  return {
    id: r.id,
    calendarId: r.calendar_id,
    title: r.title,
    dueAt: r.due_at,
    completedAt: r.completed_at,
    createdAt: r.created_at,
    updatedAt: r.updated_at,
  };
}

function mapReminder(r: RustReminder): Reminder {
  return {
    id: r.id,
    eventId: r.event_id,
    offsetMinutes: r.offset_minutes,
    absoluteAt: r.absolute_at,
    createdAt: r.created_at,
    updatedAt: r.updated_at,
  };
}

function mapAttendee(r: RustAttendee): Attendee {
  return {
    id: r.id,
    eventId: r.event_id,
    email: r.email,
    displayName: r.display_name,
    role: r.role,
    status: r.status,
    rsvp: r.rsvp,
    isOrganizer: r.is_organizer,
    createdAt: r.created_at,
    updatedAt: r.updated_at,
  };
}

export class TauriCalendarDataSource implements CalendarDataSource {
  async listAccounts(): Promise<{ account: Account; calendars: Calendar[] }[]> {
    const raw =
      await invoke<{ account: RustAccount; calendars: RustCalendar[] }[]>("list_accounts");
    return raw.map((item) => ({
      account: mapAccount(item.account),
      calendars: item.calendars.map(mapCalendar),
    }));
  }

  async listCalendars(): Promise<Calendar[]> {
    const accounts = await this.listAccounts();
    return accounts.flatMap((a) => a.calendars);
  }

  async setCalendarEnabled(calendarId: string, enabled: boolean): Promise<void> {
    await invoke("set_calendar_enabled", { calendarId, enabled });
  }

  async deleteAccount(accountId: string): Promise<void> {
    await invoke("delete_account", { id: accountId });
  }

  async deleteCalendar(calendarId: string): Promise<void> {
    await invoke("delete_calendar", { id: calendarId });
  }

  async listOccurrences(
    from: string,
    to: string,
    calendarIds?: string[],
  ): Promise<OccurrenceItem[]> {
    const raw = await invoke<RustOccurrenceItem[]>("list_occurrences", {
      from,
      to,
      calendarIds: calendarIds || null,
    });
    return raw.map((item) => ({
      occurrence: {
        eventId: item.occurrence.event_id,
        startsAt: item.occurrence.starts_at,
        endsAt: item.occurrence.ends_at,
        allDay: item.occurrence.all_day,
      },
      event: mapEvent(item.event),
    }));
  }

  async getEvent(id: string): Promise<Event | null> {
    const raw = await invoke<RustEvent | null>("get_event", { id });
    return raw ? mapEvent(raw) : null;
  }

  async saveEvent(
    draft: EventDraft,
    id?: string,
    scope?: EditScope,
    targetDate?: string,
  ): Promise<Event[]> {
    const rustDraft = {
      calendar_id: draft.calendarId,
      title: draft.title,
      location: draft.location || null,
      notes: draft.notes || null,
      starts_at: draft.startsAt,
      ends_at: draft.endsAt,
      all_day: draft.allDay,
      tz: draft.tz || null,
      rrule: draft.rrule || null,
      travel_time_minutes: draft.travelTimeMinutes ?? null,
      color: draft.color || null,
      busy: draft.busy !== false,
    };
    const raw = await invoke<RustEvent[]>("save_event", {
      draft: rustDraft,
      id: id || null,
      scope: scope || null,
      targetDate: targetDate || null,
    });
    return raw.map(mapEvent);
  }

  async deleteEvent(id: string, scope?: EditScope, targetDate?: string): Promise<Event[]> {
    const raw = await invoke<RustEvent[]>("delete_event", {
      id,
      scope: scope || null,
      targetDate: targetDate || null,
    });
    return raw.map(mapEvent);
  }

  async listTasks(from?: string, to?: string): Promise<Task[]> {
    const raw = await invoke<RustTask[]>("list_tasks", {
      from: from || null,
      to: to || null,
    });
    return raw.map(mapTask);
  }

  async toggleTask(id: string): Promise<Task> {
    const raw = await invoke<RustTask>("toggle_task", { id });
    return mapTask(raw);
  }

  async listReminders(eventId: string): Promise<Reminder[]> {
    const raw = await invoke<RustReminder[]>("list_reminders", { eventId });
    return raw.map(mapReminder);
  }

  async saveReminder(draft: ReminderDraft): Promise<Reminder> {
    const raw = await invoke<RustReminder>("save_reminder", {
      payload: {
        id: draft.id || null,
        event_id: draft.eventId,
        offset_minutes: draft.offsetMinutes ?? null,
        absolute_at: draft.absoluteAt ?? null,
      },
    });
    return mapReminder(raw);
  }

  async deleteReminder(id: string): Promise<void> {
    await invoke("delete_reminder", { id });
  }

  async snoozeReminder(id: string, minutes: number): Promise<void> {
    await invoke("snooze_reminder", { id, minutes });
  }

  async listAttendees(eventId: string): Promise<Attendee[]> {
    const raw = await invoke<RustAttendee[]>("list_attendees", { eventId });
    return raw.map(mapAttendee);
  }

  async saveAttendee(draft: AttendeeDraft): Promise<Attendee> {
    const raw = await invoke<RustAttendee>("save_attendee", {
      payload: {
        id: draft.id || null,
        event_id: draft.eventId,
        email: draft.email,
        display_name: draft.displayName ?? null,
        role: draft.role ?? "required",
        status: draft.status ?? "needs_action",
        rsvp: draft.rsvp ?? false,
        is_organizer: draft.isOrganizer ?? false,
      },
    });
    return mapAttendee(raw);
  }

  async deleteAttendee(id: string): Promise<void> {
    await invoke("delete_attendee", { id });
  }

  async suggestAttendees(query: string): Promise<Attendee[]> {
    const raw = await invoke<RustAttendee[]>("suggest_attendees", { query });
    return raw.map(mapAttendee);
  }

  async findAvailableSlots(
    date: string,
    durationMinutes: number,
    calendarIds?: string[],
  ): Promise<AvailableSlot[]> {
    const raw = await invoke<{ start: string; end: string }[]>("find_available_slots", {
      date,
      durationMinutes,
      calendarIds: calendarIds ?? null,
    });
    return raw.map((r) => ({ start: r.start, end: r.end }));
  }

  async sendInvitations(eventId: string): Promise<{
    subject: string;
    recipients: string[];
    outboxPath?: string | null;
  }> {
    const raw = await invoke<{
      envelope: { subject: string };
      recipients: string[];
      outbox_path?: string | null;
    }>("send_invitations", { eventId });
    return {
      subject: raw.envelope.subject,
      recipients: raw.recipients,
      outboxPath: raw.outbox_path,
    };
  }

  async applyItip(
    calendarId: string,
    icsContent: string,
  ): Promise<{ method: string; uid: string; message: string }> {
    return invoke("apply_itip", { calendarId, icsContent });
  }

  async getIdentity(): Promise<IdentitySettings> {
    const raw = await invoke<{ self_email?: string | null; show_declined: boolean }>(
      "get_identity",
    );
    return { selfEmail: raw.self_email ?? null, showDeclined: raw.show_declined };
  }

  async setIdentity(identity: IdentitySettings): Promise<void> {
    await invoke("set_identity", {
      identity: {
        self_email: identity.selfEmail ?? null,
        show_declined: identity.showDeclined,
      },
    });
  }

  async getDefaultAlerts(): Promise<DefaultAlerts> {
    const raw = await invoke<{ event: number | null; all_day: number | null }>(
      "get_default_alerts",
    );
    return { event: raw.event, allDay: raw.all_day };
  }

  async setDefaultAlerts(alerts: DefaultAlerts): Promise<void> {
    await invoke("set_default_alerts", {
      alerts: { event: alerts.event, all_day: alerts.allDay },
    });
  }

  async search(query: string): Promise<SearchResults> {
    const raw = await invoke<{
      events: RustEvent[];
      tasks: RustTask[];
      matched_date?: string | null;
    }>("search", {
      query,
    });
    return {
      events: raw.events.map(mapEvent),
      tasks: raw.tasks.map(mapTask),
      matchedDate: raw.matched_date || null,
    };
  }

  async exportIcs(calendarId?: string): Promise<string> {
    return invoke<string>("export_ics", {
      calendarId: calendarId || null,
    });
  }

  async importIcs(calendarId: string, icsContent: string): Promise<Event[]> {
    const raw = await invoke<RustEvent[]>("import_ics", {
      calendarId,
      icsContent,
    });
    return raw.map(mapEvent);
  }

  async addAccount(spec: {
    kind: AccountKind;
    displayName: string;
    detail: string;
  }): Promise<{ account: Account; calendars: Calendar[] }> {
    const raw = await invoke<{ account: RustAccount; calendars: RustCalendar[] }>("add_account", {
      spec: {
        kind: spec.kind,
        display_name: spec.displayName,
        detail: spec.detail,
      },
    });
    return {
      account: mapAccount(raw.account),
      calendars: raw.calendars.map(mapCalendar),
    };
  }

  async connectGoogleAccount(
    email: string,
    token: string,
  ): Promise<{ account: Account; calendars: Calendar[] }> {
    const raw = await invoke<{ account: RustAccount; calendars: RustCalendar[] }>(
      "connect_google_account",
      {
        email,
        token,
      },
    );
    return {
      account: mapAccount(raw.account),
      calendars: raw.calendars.map(mapCalendar),
    };
  }

  async syncAccount(accountId: string): Promise<{
    accountId: string;
    syncedAt: string;
    success: boolean;
    message: string;
  }> {
    const raw = await invoke<{
      account_id: string;
      synced_at: string;
      success: boolean;
      message: string;
    }>("sync_account", {
      accountId,
    });
    return {
      accountId: raw.account_id,
      syncedAt: raw.synced_at,
      success: raw.success,
      message: raw.message,
    };
  }

  async setSyncInterval(minutes: number): Promise<void> {
    await invoke<void>("set_sync_interval", {
      minutes,
    });
  }
}
