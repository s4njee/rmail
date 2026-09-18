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

export class InMemoryCalendarDataSource implements CalendarDataSource {
  private accounts: Account[] = [
    {
      id: "acc-web",
      displayName: "Web Local Account",
      kind: "local",
      status: "active",
      detail: "In-Memory Web Client",
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    },
  ];

  private calendars: Calendar[] = [
    {
      id: "cal-classes",
      accountId: "acc-web",
      name: "Classes",
      color: "#C2410C",
      enabled: true,
      eventCount: 2,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    },
    {
      id: "cal-personal",
      accountId: "acc-web",
      name: "Personal",
      color: "#1F6FEB",
      enabled: true,
      eventCount: 1,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    },
  ];

  private events: Event[] = [
    {
      id: "evt-1",
      calendarId: "cal-classes",
      uid: "evt-1@web.local",
      title: "Stats 101 Lecture",
      location: "Kane 210",
      notes: "Weekly lecture",
      startsAt: "2026-08-13T10:00:00Z",
      endsAt: "2026-08-13T11:30:00Z",
      allDay: false,
      exdates: [],
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    },
    {
      id: "evt-2",
      calendarId: "cal-personal",
      uid: "evt-2@web.local",
      title: "Dentist Appointment",
      location: "Medical Dental Bldg",
      notes: null,
      startsAt: "2026-08-13T14:00:00Z",
      endsAt: "2026-08-13T15:00:00Z",
      allDay: false,
      exdates: [],
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    },
  ];

  private tasks: Task[] = [
    {
      id: "task-1",
      calendarId: "cal-classes",
      title: "Submit Lab Report",
      dueAt: "2026-08-13T17:00:00Z",
      completedAt: null,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    },
  ];

  async listAccounts(): Promise<{ account: Account; calendars: Calendar[] }[]> {
    return this.accounts.map((account) => ({
      account,
      calendars: this.calendars.filter((c) => c.accountId === account.id),
    }));
  }

  async listCalendars(): Promise<Calendar[]> {
    return [...this.calendars];
  }

  async setCalendarEnabled(calendarId: string, enabled: boolean): Promise<void> {
    const cal = this.calendars.find((c) => c.id === calendarId);
    if (cal) cal.enabled = enabled;
  }

  async deleteAccount(accountId: string): Promise<void> {
    const calIds = this.calendars.filter((c) => c.accountId === accountId).map((c) => c.id);
    this.accounts = this.accounts.filter((a) => a.id !== accountId);
    this.calendars = this.calendars.filter((c) => c.accountId !== accountId);
    this.events = this.events.filter((e) => !calIds.includes(e.calendarId));
    this.tasks = this.tasks.filter((t) => !calIds.includes(t.calendarId));
  }

  async deleteCalendar(calendarId: string): Promise<void> {
    this.calendars = this.calendars.filter((c) => c.id !== calendarId);
    this.events = this.events.filter((e) => e.calendarId !== calendarId);
    this.tasks = this.tasks.filter((t) => t.calendarId !== calendarId);
  }

  async listOccurrences(
    _from: string,
    _to: string,
    calendarIds?: string[],
  ): Promise<OccurrenceItem[]> {
    const allowed = calendarIds || this.calendars.filter((c) => c.enabled).map((c) => c.id);
    return this.events
      .filter((e) => allowed.includes(e.calendarId))
      .map((event) => ({
        occurrence: {
          id: `occ-${event.id}`,
          eventId: event.id,
          calendarId: event.calendarId,
          startsAt: event.startsAt,
          endsAt: event.endsAt,
          allDay: event.allDay,
        },
        event,
      }));
  }

  async getEvent(id: string): Promise<Event | null> {
    return this.events.find((e) => e.id === id) || null;
  }

  async saveEvent(
    draft: EventDraft,
    id?: string,
    _scope?: EditScope,
    _targetDate?: string,
  ): Promise<Event[]> {
    if (id) {
      const existing = this.events.find((e) => e.id === id);
      if (existing) {
        existing.title = draft.title;
        existing.startsAt = draft.startsAt;
        existing.endsAt = draft.endsAt;
        existing.calendarId = draft.calendarId;
        existing.location = draft.location || null;
        existing.notes = draft.notes || null;
        existing.allDay = draft.allDay;
        existing.rrule = draft.rrule || null;
        return [existing];
      }
    }
    const newEvt: Event = {
      id: `evt-${Date.now()}`,
      calendarId: draft.calendarId,
      uid: `evt-${Date.now()}@web.local`,
      title: draft.title,
      location: draft.location || null,
      notes: draft.notes || null,
      startsAt: draft.startsAt,
      endsAt: draft.endsAt,
      allDay: draft.allDay,
      rrule: draft.rrule || null,
      exdates: [],
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };
    this.events.push(newEvt);
    return [newEvt];
  }

  async deleteEvent(id: string): Promise<Event[]> {
    this.events = this.events.filter((e) => e.id !== id);
    return [];
  }

  async listTasks(): Promise<Task[]> {
    return [...this.tasks];
  }

  async toggleTask(id: string): Promise<Task> {
    const task = this.tasks.find((t) => t.id === id);
    if (!task) throw new Error("Task not found");
    task.completedAt = task.completedAt ? null : new Date().toISOString();
    return { ...task };
  }

  private reminders: Reminder[] = [];

  private defaultAlerts: DefaultAlerts = { event: null, allDay: null };

  async listReminders(eventId: string): Promise<Reminder[]> {
    return this.reminders.filter((r) => r.eventId === eventId);
  }

  async saveReminder(draft: ReminderDraft): Promise<Reminder> {
    const existing = this.reminders.find((r) => r.id === draft.id);
    if (existing) {
      existing.offsetMinutes = draft.offsetMinutes ?? null;
      existing.absoluteAt = draft.absoluteAt ?? null;
      return { ...existing };
    }
    const reminder: Reminder = {
      id: `rem-${Date.now()}`,
      eventId: draft.eventId,
      offsetMinutes: draft.offsetMinutes ?? null,
      absoluteAt: draft.absoluteAt ?? null,
    };
    this.reminders.push(reminder);
    return { ...reminder };
  }

  async deleteReminder(id: string): Promise<void> {
    this.reminders = this.reminders.filter((r) => r.id !== id);
  }

  async snoozeReminder(_id: string, _minutes: number): Promise<void> {}

  private attendees: Attendee[] = [];

  async listAttendees(eventId: string): Promise<Attendee[]> {
    return this.attendees.filter((a) => a.eventId === eventId);
  }

  async saveAttendee(draft: AttendeeDraft): Promise<Attendee> {
    const existing = this.attendees.find((a) => a.id === draft.id);
    if (existing) {
      existing.email = draft.email;
      existing.displayName = draft.displayName ?? existing.displayName;
      existing.role = draft.role ?? existing.role;
      existing.status = draft.status ?? existing.status;
      existing.rsvp = draft.rsvp ?? existing.rsvp;
      existing.isOrganizer = draft.isOrganizer ?? existing.isOrganizer;
      return { ...existing };
    }
    const attendee: Attendee = {
      id: `att-${Date.now()}`,
      eventId: draft.eventId,
      email: draft.email,
      displayName: draft.displayName ?? null,
      role: draft.role ?? "required",
      status: draft.status ?? "needs_action",
      rsvp: draft.rsvp ?? false,
      isOrganizer: draft.isOrganizer ?? false,
    };
    this.attendees.push(attendee);
    return { ...attendee };
  }

  async deleteAttendee(id: string): Promise<void> {
    this.attendees = this.attendees.filter((a) => a.id !== id);
  }

  async suggestAttendees(query: string): Promise<Attendee[]> {
    const q = query.toLowerCase();
    const seen = new Set<string>();
    return this.attendees.filter((a) => {
      const key = a.email.toLowerCase();
      if (seen.has(key)) return false;
      if (!a.email.toLowerCase().includes(q) && !(a.displayName || "").toLowerCase().includes(q)) {
        return false;
      }
      seen.add(key);
      return true;
    });
  }

  async findAvailableSlots(
    date: string,
    durationMinutes: number,
  ): Promise<AvailableSlot[]> {
    const start = new Date(`${date}T09:00:00Z`);
    const end = new Date(`${date}T18:00:00Z`);
    const durationMs = durationMinutes * 60_000;
    const slots: AvailableSlot[] = [];
    for (let t = start.getTime(); t + durationMs <= end.getTime(); t += 30 * 60_000) {
      const s = new Date(t);
      const e = new Date(t + durationMs);
      const conflict = this.events.some((ev) => {
        if (ev.busy === false) return false;
        const es = new Date(ev.startsAt).getTime();
        const ee = new Date(ev.endsAt).getTime();
        return es < e.getTime() && ee > s.getTime();
      });
      if (!conflict) {
        slots.push({ start: s.toISOString(), end: e.toISOString() });
      }
    }
    return slots;
  }

  async sendInvitations(eventId: string): Promise<{
    subject: string;
    recipients: string[];
    outboxPath?: string | null;
  }> {
    const event = this.events.find((e) => e.id === eventId);
    const recipients = this.attendees
      .filter((a) => a.eventId === eventId && !a.isOrganizer)
      .map((a) => a.email);
    return {
      subject: `Invitation: ${event?.title ?? "event"}`,
      recipients,
      outboxPath: null,
    };
  }

  async applyItip(): Promise<{ method: string; uid: string; message: string }> {
    return { method: "REQUEST", uid: "", message: "ignored in memory" };
  }

  private identity: IdentitySettings = { selfEmail: null, showDeclined: false };

  async getIdentity(): Promise<IdentitySettings> {
    return { ...this.identity };
  }

  async setIdentity(identity: IdentitySettings): Promise<void> {
    this.identity = { ...identity };
  }

  async getDefaultAlerts(): Promise<DefaultAlerts> {
    return { ...this.defaultAlerts };
  }

  async setDefaultAlerts(alerts: DefaultAlerts): Promise<void> {
    this.defaultAlerts = { ...alerts };
  }

  async search(query: string): Promise<SearchResults> {
    const q = query.toLowerCase();
    const events = this.events.filter(
      (e) =>
        e.title.toLowerCase().includes(q) || (e.location && e.location.toLowerCase().includes(q)),
    );
    const tasks = this.tasks.filter((t) => t.title.toLowerCase().includes(q));
    return { events, tasks };
  }

  async exportIcs(): Promise<string> {
    return "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//rcalendar//Web//EN\r\nEND:VCALENDAR";
  }

  async importIcs(): Promise<Event[]> {
    return [];
  }

  async addAccount(spec: {
    kind: AccountKind;
    displayName: string;
    detail: string;
  }): Promise<{ account: Account; calendars: Calendar[] }> {
    const account: Account = {
      id: `acc-${Date.now()}`,
      displayName: spec.displayName,
      kind: spec.kind,
      status: "active",
      detail: spec.detail,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };
    this.accounts.push(account);
    return { account, calendars: [] };
  }

  async connectGoogleAccount(
    email: string,
    _token: string,
  ): Promise<{ account: Account; calendars: Calendar[] }> {
    const account: Account = {
      id: `acc-google-${Date.now()}`,
      displayName: `Google (${email})`,
      kind: "google",
      status: "active",
      detail: email,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };
    const calendar: Calendar = {
      id: `cal-google-${Date.now()}`,
      accountId: account.id,
      name: "Google Calendar",
      color: "#1F6FEB",
      enabled: true,
      eventCount: 1,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };
    this.accounts.push(account);
    this.calendars.push(calendar);
    return { account, calendars: [calendar] };
  }

  async syncAccount(accountId: string) {
    return {
      accountId,
      syncedAt: new Date().toISOString(),
      success: true,
      message: "In-memory mock sync OK",
    };
  }

  async setSyncInterval(_minutes: number): Promise<void> {}
}
