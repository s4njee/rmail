import {
  Component,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
  Switch,
  Match,
  Show,
} from "solid-js";
import {
  Account,
  addDays,
  addMonths,
  AgendaView,
  Attendee,
  AttendeeSummary,
  Calendar,
  DayView,
  DefaultAlerts,
  EditScope,
  IdentitySettings,
  EditorAlert,
  EditorAttendee,
  Event,
  EventDraft,
  EventEditorModal,
  GoogleConnectModal,
  IcsImportExportModal,
  MonthView,
  OccurrenceItem,
  Reminder,
  SearchModal,
  SettingsView,
  ShortcutsHelpModal,
  Sidebar,
  summarizeAttendees,
  Task,
  ThreeDayView,
  Titlebar,
  ViewMode,
  WeekView,
  YearView,
} from "@rcalendar/ui";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { TauriCalendarDataSource } from "./services/tauriAdapter";

const isMacOS = typeof navigator !== "undefined" && /Mac/i.test(navigator.userAgent);

export const App: Component = () => {
  const dataSource = new TauriCalendarDataSource();

  const [view, setView] = createSignal<ViewMode>("Month");
  const [focusedDate, setFocusedDate] = createSignal(new Date());
  const [selectedDate, setSelectedDate] = createSignal(new Date());

  const [accounts, setAccounts] = createSignal<{ account: Account; calendars: Calendar[] }[]>([]);
  const [calendars, setCalendars] = createSignal<Calendar[]>([]);
  const [tasks, setTasks] = createSignal<Task[]>([]);
  const [occurrences, setOccurrences] = createSignal<OccurrenceItem[]>([]);

  // Modals & Panels
  const [editor, setEditor] = createSignal<{
    isOpen: boolean;
    event: Event | null;
    initialDate?: Date;
  }>({
    isOpen: false,
    event: null,
  });

  const [isSearchOpen, setIsSearchOpen] = createSignal(false);
  const [isHelpOpen, setIsHelpOpen] = createSignal(false);
  const [isIcsOpen, setIsIcsOpen] = createSignal(false);
  const [isGoogleConnectOpen, setIsGoogleConnectOpen] = createSignal(false);
  const [isSettingsOpen, setIsSettingsOpen] = createSignal(false);

  const [defaultAlerts, setDefaultAlerts] = createSignal<DefaultAlerts>({
    event: null,
    allDay: null,
  });
  const [editorReminders, setEditorReminders] = createSignal<Reminder[]>([]);
  const [editorAttendees, setEditorAttendees] = createSignal<Attendee[]>([]);
  const [attendeeSummaries, setAttendeeSummaries] = createSignal<
    ReadonlyMap<string, AttendeeSummary>
  >(new Map());
  const [identity, setIdentity] = createSignal<IdentitySettings>({
    selfEmail: null,
    showDeclined: false,
  });
  const [firedToast, setFiredToast] = createSignal<{
    eventId: string;
    title: string;
    reminderId: string;
    startedAt: string;
  } | null>(null);

  const loadAccountsAndCalendars = async () => {
    try {
      const accs = await dataSource.listAccounts();
      setAccounts(accs);
      const cals = await dataSource.listCalendars();
      setCalendars(cals);
      const t = await dataSource.listTasks();
      setTasks(t);
    } catch (err) {
      console.error("Failed to load initial data:", err);
    }
  };

  const loadOccurrences = async () => {
    try {
      const f = focusedDate();
      const yearView = view() === "Year";
      const fromDate = yearView
        ? new Date(f.getFullYear(), 0, 1)
        : new Date(f.getFullYear(), f.getMonth() - 1, 1);
      const toDate = yearView
        ? new Date(f.getFullYear(), 11, 31, 23, 59, 59)
        : new Date(f.getFullYear(), f.getMonth() + 2, 0, 23, 59, 59);

      const fromIso = fromDate.toISOString();
      const toIso = toDate.toISOString();

      const enabledCals = calendars().filter((c) => c.enabled);
      const enabledIds = enabledCals.length > 0 ? enabledCals.map((c) => c.id) : undefined;

      const items = await dataSource.listOccurrences(fromIso, toIso, enabledIds);
      setOccurrences(items);
      await loadAttendeeSummaries(items);
    } catch (err) {
      console.error("Failed to fetch occurrences:", err);
    }
  };

  const loadAttendeeSummaries = async (items: OccurrenceItem[]) => {
    const uniqueIds = [...new Set(items.map((i) => i.event.id))];
    if (uniqueIds.length === 0) {
      setAttendeeSummaries(new Map());
      return;
    }
    try {
      const rows = await Promise.all(
        uniqueIds.map(async (id) => {
          const attendees = await dataSource.listAttendees(id);
          return [id, summarizeAttendees(attendees, identity().selfEmail)] as const;
        }),
      );
      setAttendeeSummaries(new Map(rows));
    } catch (err) {
      console.error("Failed to load attendee summaries:", err);
    }
  };

  /** Occurrences actually shown; events the user declined are hidden unless opted in. */
  const visibleOccurrences = createMemo(() => {
    const items = occurrences();
    if (identity().showDeclined) return items;
    const summaries = attendeeSummaries();
    return items.filter((i) => !summaries.get(i.event.id)?.selfDeclined);
  });

  onMount(() => {
    loadAccountsAndCalendars().then(() => loadOccurrences());
    dataSource
      .getDefaultAlerts()
      .then(setDefaultAlerts)
      .catch((err) => console.error("Failed to load default alerts:", err));
    dataSource
      .getIdentity()
      .then(setIdentity)
      .catch((err) => console.error("Failed to load identity:", err));

    // Native reminder delivery: show an in-app banner with snooze + open.
    const unlistenPromise = listen<{
      eventId: string;
      title: string;
      reminderId: string;
    }>("almanac://reminder-fired", (event) => {
      setFiredToast({ ...event.payload, startedAt: Date.now().toString() });
    });
    onCleanup(() => {
      unlistenPromise.then((unlisten) => unlisten());
    });
  });

  createEffect(() => {
    // Re-fetch occurrences when focused date, view, or calendars change
    focusedDate();
    view();
    calendars();
    loadOccurrences();
  });

  // Global Keyboard Shortcuts (S4.6)
  onMount(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't trigger if user is typing in an input/textarea
      const target = e.target as HTMLElement;
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable)
      ) {
        return;
      }

      // ⌘K: Quick search
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setIsSearchOpen(true);
        return;
      }

      // ⌘,: Settings
      if ((e.metaKey || e.ctrlKey) && e.key === ",") {
        e.preventDefault();
        setIsSettingsOpen((prev) => !prev);
        return;
      }

      // ?: Help shortcuts cheat sheet
      if (e.key === "?" || (e.shiftKey && e.key === "/")) {
        e.preventDefault();
        setIsHelpOpen(true);
        return;
      }

      // 'c' or 'n': New event
      if (e.key === "c" || e.key === "n") {
        e.preventDefault();
        handleNewEvent();
        return;
      }

      // 't': Jump to Today
      if (e.key === "t") {
        e.preventDefault();
        const now = new Date();
        setSelectedDate(now);
        setFocusedDate(now);
        return;
      }

      // 'j' or ArrowLeft: Previous period
      if (e.key === "j" || e.key === "ArrowLeft") {
        e.preventDefault();
        handleNavPrev();
        return;
      }

      // 'k' or ArrowRight: Next period
      if (e.key === "k" || e.key === "ArrowRight") {
        e.preventDefault();
        handleNavNext();
        return;
      }

      // '1' - '6': Switch views
      if (e.key === "1") {
        setView("Month");
      } else if (e.key === "2") {
        setView("Week");
      } else if (e.key === "3") {
        setView("3-day");
      } else if (e.key === "4") {
        setView("Day");
      } else if (e.key === "5") {
        setView("Agenda");
      } else if (e.key === "6") {
        setView("Year");
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    onCleanup(() => window.removeEventListener("keydown", handleKeyDown));
  });

  const handleNavPrev = () => {
    const v = view();
    if (v === "Month") {
      setFocusedDate(addMonths(focusedDate(), -1));
    } else if (v === "Week") {
      setFocusedDate(addDays(focusedDate(), -7));
    } else if (v === "3-day") {
      setFocusedDate(addDays(focusedDate(), -3));
    } else if (v === "Day") {
      const prev = addDays(selectedDate(), -1);
      setSelectedDate(prev);
      setFocusedDate(prev);
    } else if (v === "Year") {
      setFocusedDate(addMonths(focusedDate(), -12));
    }
  };

  const handleNavNext = () => {
    const v = view();
    if (v === "Month") {
      setFocusedDate(addMonths(focusedDate(), 1));
    } else if (v === "Week") {
      setFocusedDate(addDays(focusedDate(), 7));
    } else if (v === "3-day") {
      setFocusedDate(addDays(focusedDate(), 3));
    } else if (v === "Day") {
      const next = addDays(selectedDate(), 1);
      setSelectedDate(next);
      setFocusedDate(next);
    } else if (v === "Year") {
      setFocusedDate(addMonths(focusedDate(), 12));
    }
  };

  const handleToggleCalendar = async (id: string, enabled: boolean) => {
    try {
      await dataSource.setCalendarEnabled(id, enabled);
      setCalendars((prev) => prev.map((c) => (c.id === id ? { ...c, enabled } : c)));
      await loadAccountsAndCalendars();
      await loadOccurrences();
    } catch (err) {
      console.error("Failed to toggle calendar:", err);
    }
  };

  const handleToggleTask = async (id: string) => {
    try {
      const updated = await dataSource.toggleTask(id);
      setTasks((prev) => prev.map((t) => (t.id === id ? updated : t)));
    } catch (err) {
      console.error("Failed to toggle task:", err);
    }
  };

  const handleNewEvent = () => {
    setEditorReminders([]);
    setEditor({
      isOpen: true,
      event: null,
      initialDate: selectedDate(),
    });
  };

  const handleEventClick = (occ: OccurrenceItem) => {
    setEditorReminders([]);
    setEditorAttendees([]);
    dataSource
      .listReminders(occ.event.id)
      .then(setEditorReminders)
      .catch((err) => console.error("Failed to load reminders:", err));
    dataSource
      .listAttendees(occ.event.id)
      .then(setEditorAttendees)
      .catch((err) => console.error("Failed to load attendees:", err));
    setEditor({
      isOpen: true,
      event: occ.event,
      initialDate: new Date(occ.occurrence.startsAt),
    });
  };

  const handleSlotClick = (date: Date) => {
    setSelectedDate(date);
    setEditorReminders([]);
    setEditorAttendees([]);
    setEditor({
      isOpen: true,
      event: null,
      initialDate: date,
    });
  };

  const reconcileReminders = async (eventId: string, alerts: EditorAlert[]) => {
    const existing = await dataSource.listReminders(eventId);
    const keepIds = new Set(alerts.map((a) => a.id).filter((x): x is string => !!x));
    for (const r of existing) {
      if (!keepIds.has(r.id)) await dataSource.deleteReminder(r.id);
    }
    for (const a of alerts) {
      await dataSource.saveReminder({
        id: a.id,
        eventId,
        offsetMinutes: a.offsetMinutes ?? null,
        absoluteAt: a.absoluteAt ?? null,
      });
    }
  };

  const reconcileAttendees = async (eventId: string, invitees: EditorAttendee[]) => {
    const existing = await dataSource.listAttendees(eventId);
    const keepIds = new Set(invitees.map((a) => a.id).filter((x): x is string => !!x));
    for (const a of existing) {
      if (!keepIds.has(a.id)) await dataSource.deleteAttendee(a.id);
    }
    for (const a of invitees) {
      await dataSource.saveAttendee({
        id: a.id,
        eventId,
        email: a.email,
        displayName: a.displayName ?? null,
        role: a.role,
        status: a.status,
        rsvp: a.rsvp ?? false,
        isOrganizer: a.isOrganizer ?? false,
      });
    }
  };

  const handleSaveEvent = async (
    draft: EventDraft,
    alerts: EditorAlert[],
    invitees: EditorAttendee[],
    id?: string,
    scope?: EditScope,
    targetDate?: string,
  ) => {
    try {
      const saved = await dataSource.saveEvent(draft, id, scope, targetDate);
      const targetEventId = id || saved[0]?.id || "";
      if (targetEventId) {
        const previous = await dataSource.listAttendees(targetEventId).catch(() => []);
        await reconcileReminders(targetEventId, alerts);
        await reconcileAttendees(targetEventId, invitees);
        const previousEmails = new Set(previous.map((a) => a.email.toLowerCase()));
        const added = invitees.some(
          (a) => !a.isOrganizer && !previousEmails.has(a.email.toLowerCase()),
        );
        if (added) {
          await dataSource.sendInvitations(targetEventId).catch((err) => {
            console.error("Failed to send invitations:", err);
          });
        }
      }
      setEditorReminders([]);
      setEditorAttendees([]);
      await loadOccurrences();
      await loadAccountsAndCalendars();
    } catch (err) {
      console.error("Failed to save event:", err);
    }
  };

  const handleDeleteEvent = async (id: string, scope?: EditScope, targetDate?: string) => {
    try {
      await dataSource.deleteEvent(id, scope, targetDate);
      await loadOccurrences();
      await loadAccountsAndCalendars();
    } catch (err) {
      console.error("Failed to delete event:", err);
    }
  };

  // Drag mutations (S4.2, S4.3)
  const handleEventMove = async (occ: OccurrenceItem, newStart: Date, newEnd: Date) => {
    try {
      const draft: EventDraft = {
        calendarId: occ.event.calendarId,
        title: occ.event.title,
        location: occ.event.location,
        notes: occ.event.notes,
        startsAt: newStart.toISOString(),
        endsAt: newEnd.toISOString(),
        allDay: occ.event.allDay,
        tz: occ.event.tz,
        rrule: occ.event.rrule,
        travelTimeMinutes: occ.event.travelTimeMinutes,
        color: occ.event.color,
        busy: occ.event.busy !== false,
      };
      await dataSource.saveEvent(draft, occ.event.id, "all");
      await loadOccurrences();
    } catch (err) {
      console.error("Failed to move event:", err);
    }
  };

  const handleEventResize = async (occ: OccurrenceItem, newEnd: Date) => {
    try {
      const draft: EventDraft = {
        calendarId: occ.event.calendarId,
        title: occ.event.title,
        location: occ.event.location,
        notes: occ.event.notes,
        startsAt: occ.event.startsAt,
        endsAt: newEnd.toISOString(),
        allDay: occ.event.allDay,
        tz: occ.event.tz,
        rrule: occ.event.rrule,
        travelTimeMinutes: occ.event.travelTimeMinutes,
        color: occ.event.color,
        busy: occ.event.busy !== false,
      };
      await dataSource.saveEvent(draft, occ.event.id, "all");
      await loadOccurrences();
    } catch (err) {
      console.error("Failed to resize event:", err);
    }
  };

  const handleRangeCreate = (startsAt: Date, _endsAt: Date) => {
    setEditorReminders([]);
    setEditor({
      isOpen: true,
      event: null,
      initialDate: startsAt,
    });
  };

  // Window Controls
  const handleMinimize = async () => {
    try {
      await getCurrentWindow().minimize();
    } catch (e) {
      console.warn("Window control not available:", e);
    }
  };

  const handleMaximize = async () => {
    try {
      const win = getCurrentWindow();
      if (await win.isMaximized()) {
        await win.unmaximize();
      } else {
        await win.maximize();
      }
    } catch (e) {
      console.warn("Window control not available:", e);
    }
  };

  const handleClose = async () => {
    try {
      await getCurrentWindow().close();
    } catch (e) {
      console.warn("Window control not available:", e);
    }
  };

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        height: "100vh",
        width: "100vw",
        background: "var(--al-chrome, #FAFAFA)",
        overflow: "hidden",
      }}
    >
      {/* Titlebar */}
      <Titlebar
        activeView={view()}
        onViewChange={setView}
        onNewEvent={handleNewEvent}
        onSearchClick={() => setIsSearchOpen(true)}
        onMinimize={handleMinimize}
        onMaximize={handleMaximize}
        onClose={handleClose}
        showWindowControls={!isMacOS}
        leadingInset={isMacOS ? 72 : 0}
      />

      {/* Main Workspace */}
      <Show
        when={!isSettingsOpen()}
        fallback={
          <SettingsView
            accounts={accounts()}
            calendars={calendars()}
            onToggleCalendar={handleToggleCalendar}
            onSyncAccount={async (accId) => {
              await dataSource.syncAccount(accId);
              await loadAccountsAndCalendars();
              await loadOccurrences();
            }}
            onSetSyncInterval={(mins) => dataSource.setSyncInterval(mins)}
            defaultAlerts={defaultAlerts()}
            onSetDefaultAlerts={async (alerts) => {
              await dataSource.setDefaultAlerts(alerts);
              setDefaultAlerts(alerts);
            }}
            identity={identity()}
            onSetIdentity={async (next) => {
              await dataSource.setIdentity(next);
              setIdentity(next);
              await loadOccurrences();
            }}
            onAddAccountClick={() => setIsIcsOpen(true)}
            onConnectGoogleClick={() => setIsGoogleConnectOpen(true)}
            onDeleteAccount={async (accountId) => {
              await dataSource.deleteAccount(accountId);
              await loadAccountsAndCalendars();
              await loadOccurrences();
            }}
            onDeleteCalendar={async (calendarId) => {
              await dataSource.deleteCalendar(calendarId);
              await loadAccountsAndCalendars();
              await loadOccurrences();
            }}
            onClose={() => setIsSettingsOpen(false)}
          />
        }
      >
        <div style={{ display: "flex", flex: 1, "min-height": 0, overflow: "hidden" }}>
          <Sidebar
            focusedDate={focusedDate()}
            selectedDate={selectedDate()}
            onSelectDate={(d) => {
              setSelectedDate(d);
              setFocusedDate(d);
            }}
            onFocusedDateChange={setFocusedDate}
            calendars={calendars()}
            onToggleCalendar={handleToggleCalendar}
            tasks={tasks()}
            onToggleTask={handleToggleTask}
            onAddTask={handleNewEvent}
            onSettingsClick={() => setIsSettingsOpen(true)}
          />

          <main
            style={{ flex: 1, display: "flex", "min-width": 0, height: "100%", overflow: "hidden" }}
          >
            <Switch>
              <Match when={view() === "Month"}>
                <MonthView
                  focusedDate={focusedDate()}
                  selectedDate={selectedDate()}
                  onSelectDate={setSelectedDate}
                  onFocusedDateChange={setFocusedDate}
                  occurrences={visibleOccurrences()}
                  calendars={calendars()}
                  onEventClick={handleEventClick}
                  onCellClick={(d) => {
                    setSelectedDate(d);
                  }}
                  attendeeSummaries={attendeeSummaries()}
                />
              </Match>

              <Match when={view() === "Week"}>
                <WeekView
                  focusedDate={focusedDate()}
                  selectedDate={selectedDate()}
                  onSelectDate={setSelectedDate}
                  onFocusedDateChange={setFocusedDate}
                  occurrences={visibleOccurrences()}
                  calendars={calendars()}
                  onEventClick={handleEventClick}
                  onSlotClick={handleSlotClick}
                  onEventMove={handleEventMove}
                  onEventResize={handleEventResize}
                  onRangeCreate={handleRangeCreate}
                  attendeeSummaries={attendeeSummaries()}
                />
              </Match>

              <Match when={view() === "3-day"}>
                <ThreeDayView
                  focusedDate={focusedDate()}
                  selectedDate={selectedDate()}
                  onSelectDate={setSelectedDate}
                  onFocusedDateChange={setFocusedDate}
                  occurrences={visibleOccurrences()}
                  calendars={calendars()}
                  onEventClick={handleEventClick}
                  onSlotClick={handleSlotClick}
                  onEventMove={handleEventMove}
                  onEventResize={handleEventResize}
                  onRangeCreate={handleRangeCreate}
                  attendeeSummaries={attendeeSummaries()}
                />
              </Match>

              <Match when={view() === "Day"}>
                <DayView
                  focusedDate={focusedDate()}
                  selectedDate={selectedDate()}
                  onSelectDate={setSelectedDate}
                  onFocusedDateChange={setFocusedDate}
                  occurrences={visibleOccurrences()}
                  calendars={calendars()}
                  tasks={tasks()}
                  onToggleTask={handleToggleTask}
                  onEventClick={handleEventClick}
                  onSlotClick={handleSlotClick}
                  onAddToDay={(d) => {
                    setEditorReminders([]);
                    setEditor({
                      isOpen: true,
                      event: null,
                      initialDate: d,
                    });
                  }}
                  onEventMove={handleEventMove}
                  onEventResize={handleEventResize}
                  onRangeCreate={handleRangeCreate}
                  attendeeSummaries={attendeeSummaries()}
                />
              </Match>

              <Match when={view() === "Agenda"}>
                <AgendaView
                  focusedDate={focusedDate()}
                  selectedDate={selectedDate()}
                  onSelectDate={setSelectedDate}
                  onFocusedDateChange={setFocusedDate}
                  occurrences={visibleOccurrences()}
                  calendars={calendars()}
                  tasks={tasks()}
                  onToggleTask={handleToggleTask}
                  onEventClick={handleEventClick}
                  attendeeSummaries={attendeeSummaries()}
                />
              </Match>

              <Match when={view() === "Year"}>
                <YearView
                  focusedDate={focusedDate()}
                  selectedDate={selectedDate()}
                  onSelectDate={setSelectedDate}
                  onFocusedDateChange={setFocusedDate}
                  occurrences={visibleOccurrences()}
                  calendars={calendars()}
                  onNavigateView={setView}
                />
              </Match>
            </Switch>
          </main>
        </div>
      </Show>

      {/* Event Editor Modal Sheet */}
      <EventEditorModal
        isOpen={editor().isOpen}
        event={editor().event}
        initialDate={editor().initialDate}
        calendars={calendars()}
        reminders={editorReminders()}
        attendees={editorAttendees()}
        defaultAlertOffset={editor().event ? null : defaultAlerts().event}
        onSuggestAttendees={(q) => dataSource.suggestAttendees(q)}
        onFindAvailableSlots={async (date, duration) =>
          dataSource.findAvailableSlots(
            date,
            duration,
            calendars()
              .filter((c) => c.enabled)
              .map((c) => c.id),
          )
        }
        onSave={handleSaveEvent}
        onDelete={handleDeleteEvent}
        onClose={() => setEditor({ isOpen: false, event: null })}
      />

      {/* ⌘K Search Modal */}
      <SearchModal
        isOpen={isSearchOpen()}
        onClose={() => setIsSearchOpen(false)}
        onSearch={(q) => dataSource.search(q)}
        onSelectEvent={(evt) => {
          setEditorReminders([]);
          setEditorAttendees([]);
          dataSource
            .listReminders(evt.id)
            .then(setEditorReminders)
            .catch((err) => console.error("Failed to load reminders:", err));
          dataSource
            .listAttendees(evt.id)
            .then(setEditorAttendees)
            .catch((err) => console.error("Failed to load attendees:", err));
          setEditor({ isOpen: true, event: evt });
        }}
        onSelectDate={(d) => {
          setSelectedDate(d);
          setFocusedDate(d);
        }}
      />

      {/* Shortcuts Help Modal */}
      <ShortcutsHelpModal isOpen={isHelpOpen()} onClose={() => setIsHelpOpen(false)} />

      {/* iCalendar Import / Export Modal */}
      <IcsImportExportModal
        isOpen={isIcsOpen()}
        onClose={() => setIsIcsOpen(false)}
        calendars={calendars()}
        onExport={(calId) => dataSource.exportIcs(calId)}
        onImport={async (calId, icsText) => {
          await dataSource.importIcs(calId, icsText);
          await loadOccurrences();
          await loadAccountsAndCalendars();
        }}
      />

      {/* Google Calendar Connect Modal */}
      <GoogleConnectModal
        isOpen={isGoogleConnectOpen()}
        onClose={() => setIsGoogleConnectOpen(false)}
        onConnect={async (email, token) => {
          await dataSource.connectGoogleAccount(email, token);
          await loadAccountsAndCalendars();
          await loadOccurrences();
        }}
      />

      {/* Reminder-fired banner (native delivery surface for snooze + open) */}
      <Show when={firedToast()}>
        {(toast) => (
          <div
            style={{
              position: "fixed",
              top: "64px",
              right: "16px",
              "z-index": 200,
              width: "320px",
              padding: "14px 16px",
              "border-radius": "12px",
              background: "var(--al-surface, #FFFFFF)",
              "box-shadow": "var(--al-shadow-modal, 0 24px 60px -18px rgba(0,0,0,0.4))",
              border: "1px solid var(--al-border, #E0E0E0)",
              "font-family": "var(--al-font-ui)",
              color: "var(--al-ink, #1A1A1A)",
            }}
          >
            <div style={{ "font-size": "13.5px", "font-weight": 600 }}>{toast().title}</div>
            <div style={{ "font-size": "12px", color: "#777777", "margin-top": "2px" }}>
              Reminder — it's time.
            </div>
            <div
              style={{
                display: "flex",
                gap: "8px",
                "margin-top": "10px",
                "justify-content": "flex-end",
              }}
            >
              <button
                type="button"
                onClick={() =>
                  dataSource
                    .snoozeReminder(toast().reminderId, 5)
                    .catch((err) => console.error("Snooze failed:", err))
                    .then(() => setFiredToast(null))
                }
                style={{
                  height: "28px",
                  padding: "0 10px",
                  border: "1px solid var(--al-border, #E0E0E0)",
                  "border-radius": "7px",
                  background: "#FFFFFF",
                  "font-size": "11.5px",
                  cursor: "pointer",
                }}
              >
                Snooze 5 min
              </button>
              <button
                type="button"
                onClick={() =>
                  dataSource
                    .snoozeReminder(toast().reminderId, 15)
                    .catch((err) => console.error("Snooze failed:", err))
                    .then(() => setFiredToast(null))
                }
                style={{
                  height: "28px",
                  padding: "0 10px",
                  border: "1px solid var(--al-border, #E0E0E0)",
                  "border-radius": "7px",
                  background: "#FFFFFF",
                  "font-size": "11.5px",
                  cursor: "pointer",
                }}
              >
                15 min
              </button>
              <button
                type="button"
                onClick={() => {
                  const eventId = toast().eventId;
                  dataSource
                    .getEvent(eventId)
                    .then((evt) => {
                      if (evt) {
                        setEditorReminders([]);
                        dataSource
                          .listReminders(eventId)
                          .then(setEditorReminders)
                          .catch(() => {});
                        setEditor({
                          isOpen: true,
                          event: evt,
                          initialDate: new Date(evt.startsAt),
                        });
                      }
                    })
                    .catch(() => {});
                  setFiredToast(null);
                }}
                style={{
                  height: "28px",
                  padding: "0 10px",
                  border: "none",
                  "border-radius": "7px",
                  background: "var(--al-accent, #1F6FEB)",
                  color: "#FFFFFF",
                  "font-size": "11.5px",
                  cursor: "pointer",
                }}
              >
                Open
              </button>
            </div>
          </div>
        )}
      </Show>
    </div>
  );
};

export default App;
