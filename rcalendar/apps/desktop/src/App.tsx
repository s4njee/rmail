import { Component, createEffect, createSignal, onCleanup, onMount, Switch, Match, Show } from 'solid-js';
import {
  Account,
  addDays,
  addMonths,
  AgendaView,
  Calendar,
  DayView,
  EditScope,
  Event,
  EventDraft,
  EventEditorModal,
  GoogleConnectModal,
  IcsImportExportModal,
  MonthView,
  OccurrenceItem,
  SearchModal,
  SettingsView,
  ShortcutsHelpModal,
  Sidebar,
  Task,
  ThreeDayView,
  Titlebar,
  ViewMode,
  WeekView,
} from '@rcalendar/ui';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { TauriCalendarDataSource } from './services/tauriAdapter';

export const App: Component = () => {
  const dataSource = new TauriCalendarDataSource();

  const [view, setView] = createSignal<ViewMode>('Month');
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

  const loadAccountsAndCalendars = async () => {
    try {
      const accs = await dataSource.listAccounts();
      setAccounts(accs);
      const cals = await dataSource.listCalendars();
      setCalendars(cals);
      const t = await dataSource.listTasks();
      setTasks(t);
    } catch (err) {
      console.error('Failed to load initial data:', err);
    }
  };

  const loadOccurrences = async () => {
    try {
      const f = focusedDate();
      // Fetch a 3-month window around focusedDate
      const fromDate = new Date(f.getFullYear(), f.getMonth() - 1, 1);
      const toDate = new Date(f.getFullYear(), f.getMonth() + 2, 0, 23, 59, 59);

      const fromIso = fromDate.toISOString();
      const toIso = toDate.toISOString();

      const enabledCals = calendars().filter((c) => c.enabled);
      const enabledIds = enabledCals.length > 0 ? enabledCals.map((c) => c.id) : undefined;

      const items = await dataSource.listOccurrences(fromIso, toIso, enabledIds);
      setOccurrences(items);
    } catch (err) {
      console.error('Failed to fetch occurrences:', err);
    }
  };

  onMount(() => {
    loadAccountsAndCalendars().then(() => loadOccurrences());
  });

  createEffect(() => {
    // Re-fetch occurrences when focused date or calendars change
    focusedDate();
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
        (target.tagName === 'INPUT' ||
          target.tagName === 'TEXTAREA' ||
          target.tagName === 'SELECT' ||
          target.isContentEditable)
      ) {
        return;
      }

      // ⌘K: Quick search
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setIsSearchOpen(true);
        return;
      }

      // ⌘,: Settings
      if ((e.metaKey || e.ctrlKey) && e.key === ',') {
        e.preventDefault();
        setIsSettingsOpen((prev) => !prev);
        return;
      }

      // ?: Help shortcuts cheat sheet
      if (e.key === '?' || (e.shiftKey && e.key === '/')) {
        e.preventDefault();
        setIsHelpOpen(true);
        return;
      }

      // 'c' or 'n': New event
      if (e.key === 'c' || e.key === 'n') {
        e.preventDefault();
        handleNewEvent();
        return;
      }

      // 't': Jump to Today
      if (e.key === 't') {
        e.preventDefault();
        const now = new Date();
        setSelectedDate(now);
        setFocusedDate(now);
        return;
      }

      // 'j' or ArrowLeft: Previous period
      if (e.key === 'j' || e.key === 'ArrowLeft') {
        e.preventDefault();
        handleNavPrev();
        return;
      }

      // 'k' or ArrowRight: Next period
      if (e.key === 'k' || e.key === 'ArrowRight') {
        e.preventDefault();
        handleNavNext();
        return;
      }

      // '1' - '5': Switch views
      if (e.key === '1') {
        setView('Month');
      } else if (e.key === '2') {
        setView('Week');
      } else if (e.key === '3') {
        setView('3-day');
      } else if (e.key === '4') {
        setView('Day');
      } else if (e.key === '5') {
        setView('Agenda');
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    onCleanup(() => window.removeEventListener('keydown', handleKeyDown));
  });

  const handleNavPrev = () => {
    const v = view();
    if (v === 'Month') {
      setFocusedDate(addMonths(focusedDate(), -1));
    } else if (v === 'Week') {
      setFocusedDate(addDays(focusedDate(), -7));
    } else if (v === '3-day') {
      setFocusedDate(addDays(focusedDate(), -3));
    } else if (v === 'Day') {
      const prev = addDays(selectedDate(), -1);
      setSelectedDate(prev);
      setFocusedDate(prev);
    }
  };

  const handleNavNext = () => {
    const v = view();
    if (v === 'Month') {
      setFocusedDate(addMonths(focusedDate(), 1));
    } else if (v === 'Week') {
      setFocusedDate(addDays(focusedDate(), 7));
    } else if (v === '3-day') {
      setFocusedDate(addDays(focusedDate(), 3));
    } else if (v === 'Day') {
      const next = addDays(selectedDate(), 1);
      setSelectedDate(next);
      setFocusedDate(next);
    }
  };

  const handleToggleCalendar = async (id: string, enabled: boolean) => {
    try {
      await dataSource.setCalendarEnabled(id, enabled);
      setCalendars((prev) =>
        prev.map((c) => (c.id === id ? { ...c, enabled } : c)),
      );
      await loadAccountsAndCalendars();
      await loadOccurrences();
    } catch (err) {
      console.error('Failed to toggle calendar:', err);
    }
  };

  const handleToggleTask = async (id: string) => {
    try {
      const updated = await dataSource.toggleTask(id);
      setTasks((prev) => prev.map((t) => (t.id === id ? updated : t)));
    } catch (err) {
      console.error('Failed to toggle task:', err);
    }
  };

  const handleNewEvent = () => {
    setEditor({
      isOpen: true,
      event: null,
      initialDate: selectedDate(),
    });
  };

  const handleEventClick = (occ: OccurrenceItem) => {
    setEditor({
      isOpen: true,
      event: occ.event,
      initialDate: new Date(occ.occurrence.startsAt),
    });
  };

  const handleSlotClick = (date: Date) => {
    setSelectedDate(date);
    setEditor({
      isOpen: true,
      event: null,
      initialDate: date,
    });
  };

  const handleSaveEvent = async (
    draft: EventDraft,
    id?: string,
    scope?: EditScope,
    targetDate?: string,
  ) => {
    try {
      await dataSource.saveEvent(draft, id, scope, targetDate);
      await loadOccurrences();
      await loadAccountsAndCalendars();
    } catch (err) {
      console.error('Failed to save event:', err);
    }
  };

  const handleDeleteEvent = async (id: string, scope?: EditScope, targetDate?: string) => {
    try {
      await dataSource.deleteEvent(id, scope, targetDate);
      await loadOccurrences();
      await loadAccountsAndCalendars();
    } catch (err) {
      console.error('Failed to delete event:', err);
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
      };
      await dataSource.saveEvent(draft, occ.event.id, 'all');
      await loadOccurrences();
    } catch (err) {
      console.error('Failed to move event:', err);
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
      };
      await dataSource.saveEvent(draft, occ.event.id, 'all');
      await loadOccurrences();
    } catch (err) {
      console.error('Failed to resize event:', err);
    }
  };

  const handleRangeCreate = (startsAt: Date, _endsAt: Date) => {
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
      console.warn('Window control not available:', e);
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
      console.warn('Window control not available:', e);
    }
  };

  const handleClose = async () => {
    try {
      await getCurrentWindow().close();
    } catch (e) {
      console.warn('Window control not available:', e);
    }
  };

  return (
    <div
      style={{
        display: 'flex',
        'flex-direction': 'column',
        height: '100vh',
        width: '100vw',
        background: 'var(--al-chrome, #FAFAFA)',
        overflow: 'hidden',
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
            onAddAccountClick={() => setIsIcsOpen(true)}
            onConnectGoogleClick={() => setIsGoogleConnectOpen(true)}
            onClose={() => setIsSettingsOpen(false)}
          />
        }
      >
        <div style={{ display: 'flex', flex: 1, 'min-height': 0, overflow: 'hidden' }}>
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

          <main style={{ flex: 1, display: 'flex', 'min-width': 0, height: '100%', overflow: 'hidden' }}>
            <Switch>
              <Match when={view() === 'Month'}>
                <MonthView
                  focusedDate={focusedDate()}
                  selectedDate={selectedDate()}
                  onSelectDate={setSelectedDate}
                  onFocusedDateChange={setFocusedDate}
                  occurrences={occurrences()}
                  calendars={calendars()}
                  onEventClick={handleEventClick}
                  onCellClick={(d) => {
                    setSelectedDate(d);
                  }}
                />
              </Match>

              <Match when={view() === 'Week'}>
                <WeekView
                  focusedDate={focusedDate()}
                  selectedDate={selectedDate()}
                  onSelectDate={setSelectedDate}
                  onFocusedDateChange={setFocusedDate}
                  occurrences={occurrences()}
                  calendars={calendars()}
                  onEventClick={handleEventClick}
                  onSlotClick={handleSlotClick}
                  onEventMove={handleEventMove}
                  onEventResize={handleEventResize}
                  onRangeCreate={handleRangeCreate}
                />
              </Match>

              <Match when={view() === '3-day'}>
                <ThreeDayView
                  focusedDate={focusedDate()}
                  selectedDate={selectedDate()}
                  onSelectDate={setSelectedDate}
                  onFocusedDateChange={setFocusedDate}
                  occurrences={occurrences()}
                  calendars={calendars()}
                  onEventClick={handleEventClick}
                  onSlotClick={handleSlotClick}
                  onEventMove={handleEventMove}
                  onEventResize={handleEventResize}
                  onRangeCreate={handleRangeCreate}
                />
              </Match>

              <Match when={view() === 'Day'}>
                <DayView
                  focusedDate={focusedDate()}
                  selectedDate={selectedDate()}
                  onSelectDate={setSelectedDate}
                  onFocusedDateChange={setFocusedDate}
                  occurrences={occurrences()}
                  calendars={calendars()}
                  tasks={tasks()}
                  onToggleTask={handleToggleTask}
                  onEventClick={handleEventClick}
                  onSlotClick={handleSlotClick}
                  onAddToDay={(d) => {
                    setEditor({
                      isOpen: true,
                      event: null,
                      initialDate: d,
                    });
                  }}
                  onEventMove={handleEventMove}
                  onEventResize={handleEventResize}
                  onRangeCreate={handleRangeCreate}
                />
              </Match>

              <Match when={view() === 'Agenda'}>
                <AgendaView
                  focusedDate={focusedDate()}
                  selectedDate={selectedDate()}
                  onSelectDate={setSelectedDate}
                  onFocusedDateChange={setFocusedDate}
                  occurrences={occurrences()}
                  calendars={calendars()}
                  tasks={tasks()}
                  onToggleTask={handleToggleTask}
                  onEventClick={handleEventClick}
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
          setEditor({ isOpen: true, event: evt });
        }}
        onSelectDate={(d) => {
          setSelectedDate(d);
          setFocusedDate(d);
        }}
      />

      {/* Shortcuts Help Modal */}
      <ShortcutsHelpModal
        isOpen={isHelpOpen()}
        onClose={() => setIsHelpOpen(false)}
      />

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
    </div>
  );
};

export default App;
