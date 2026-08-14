import {
  createEffect,
  createSignal,
  For,
  Match,
  onCleanup,
  onMount,
  Switch,
} from "solid-js";
import {
  addDays,
  addMonths,
  AgendaView,
  DayView,
  EditScope,
  Event,
  EventDraft,
  EventEditorModal,
  IcsImportExportModal,
  MonthView,
  OccurrenceItem,
  SearchModal,
  ShortcutsHelpModal,
  ThreeDayView,
  ViewMode,
  WeekView,
} from "@rcalendar/ui";
import "@rcalendar/ui/tokens.css";
import {
  selectEvent,
  useEvents,
  useSelectedEvent,
} from "../../lib/calendar";
import { useAccounts } from "../../lib/mail";
import { useTheme } from "../../lib/theme";
import { QuillCalendarDataSource, quillEventToDomain } from "../../lib/calendarAdapter";
import "./CalendarView.css";

export function CalendarView() {
  const theme = useTheme();
  const accounts = useAccounts();
  const rawEvents = useEvents();
  const selectedEvent = useSelectedEvent();

  const dataSource = new QuillCalendarDataSource();

  const [view, setView] = createSignal<ViewMode>("Month");
  const [focusedDate, setFocusedDate] = createSignal(new Date());
  const [selectedDate, setSelectedDate] = createSignal(new Date());
  const [occurrences, setOccurrences] = createSignal<OccurrenceItem[]>([]);

  // Modals state
  const [isEditorOpen, setIsEditorOpen] = createSignal(false);
  const [editingEvent, setEditingEvent] = createSignal<Event | null>(null);
  const [editorInitialDate, setEditorInitialDate] = createSignal<Date>(new Date());
  const [isSearchOpen, setIsSearchOpen] = createSignal(false);
  const [isIcsOpen, setIsIcsOpen] = createSignal(false);
  const [isShortcutsOpen, setIsShortcutsOpen] = createSignal(false);

  // Sync occurrences whenever rawEvents change
  createEffect(() => {
    const accs = accounts();
    const evts = rawEvents();
    const items = evts.map((e) => {
      const calId = `cal-${e.account_id || accs[0]?.id || 1}`;
      return quillEventToDomain(e, calId);
    });
    setOccurrences(items);
  });

  const reloadData = async () => {
    const f = focusedDate();
    const fromDate = new Date(f.getFullYear(), f.getMonth() - 1, 1);
    const toDate = new Date(f.getFullYear(), f.getMonth() + 2, 0, 23, 59, 59);
    const occs = await dataSource.listOccurrences(
      fromDate.toISOString(),
      toDate.toISOString(),
    );
    setOccurrences(occs);
  };

  onMount(() => {
    void reloadData();

    // Calendar-level keyboard shortcuts
    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't trigger if user is in an input or textarea
      const target = e.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable)
      ) {
        return;
      }

      if (isEditorOpen() || isSearchOpen() || isIcsOpen() || isShortcutsOpen()) {
        return;
      }

      if (e.key === "t" || e.key === "T") {
        handleToday();
      } else if (e.key === "m" || e.key === "M") {
        setView("Month");
      } else if (e.key === "w" || e.key === "W") {
        setView("Week");
      } else if (e.key === "3") {
        setView("3-day");
      } else if (e.key === "d" || e.key === "D") {
        setView("Day");
      } else if (e.key === "a" || e.key === "A") {
        setView("Agenda");
      } else if (e.key === "c" || e.key === "C" || e.key === "n" || e.key === "N") {
        handleOpenNewEvent();
      } else if (e.key === "/" || (e.key === "f" && (e.metaKey || e.ctrlKey))) {
        e.preventDefault();
        setIsSearchOpen(true);
      } else if (e.key === "?") {
        setIsShortcutsOpen(true);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    onCleanup(() => window.removeEventListener("keydown", handleKeyDown));
  });

  createEffect(() => {
    focusedDate();
    void reloadData();
  });

  const calendars = () =>
    accounts().map((acc) => ({
      id: `cal-${acc.id}`,
      accountId: String(acc.id),
      name: acc.address,
      color: acc.color || "#3b5bdb",
      enabled: true,
      eventCount: 0,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    }));

  const handlePrev = () => {
    if (view() === "Month") setFocusedDate(addMonths(focusedDate(), -1));
    else if (view() === "Week") setFocusedDate(addDays(focusedDate(), -7));
    else if (view() === "3-day") setFocusedDate(addDays(focusedDate(), -3));
    else setFocusedDate(addDays(focusedDate(), -1));
  };

  const handleNext = () => {
    if (view() === "Month") setFocusedDate(addMonths(focusedDate(), 1));
    else if (view() === "Week") setFocusedDate(addDays(focusedDate(), 7));
    else if (view() === "3-day") setFocusedDate(addDays(focusedDate(), 3));
    else setFocusedDate(addDays(focusedDate(), 1));
  };

  const handleToday = () => {
    const now = new Date();
    setFocusedDate(now);
    setSelectedDate(now);
  };

  const formatTitle = () => {
    const d = focusedDate();
    return d.toLocaleString("default", { month: "long", year: "numeric" });
  };

  const handleOpenNewEvent = (date?: Date) => {
    setEditingEvent(null);
    setEditorInitialDate(date || selectedDate() || new Date());
    setIsEditorOpen(true);
  };

  const handleEventClick = (occ: OccurrenceItem) => {
    const found = rawEvents().find((e) => String(e.id) === occ.event.id);
    if (found) {
      selectEvent(found);
    } else {
      selectEvent({
        id: Number(occ.event.id) || 0,
        account_id: Number(occ.event.calendarId.replace("cal-", "")) || 1,
        title: occ.event.title,
        start_ms: new Date(occ.occurrence.startsAt).getTime(),
        end_ms: new Date(occ.occurrence.endsAt).getTime(),
        all_day: occ.occurrence.allDay,
        location: occ.event.location || null,
        notes: occ.event.notes || null,
      });
    }
  };

  const handleCellClick = (d: Date) => {
    setSelectedDate(d);
  };

  const handleSlotClick = (d: Date) => {
    setSelectedDate(d);
    handleOpenNewEvent(d);
  };

  const handleSaveModal = async (
    draft: EventDraft,
    id?: string,
    scope?: EditScope,
    targetDate?: string,
  ) => {
    await dataSource.saveEvent(draft, id, scope, targetDate);
    await reloadData();
  };

  const handleDeleteModal = async (id: string) => {
    await dataSource.deleteEvent(id);
    if (selectedEvent()?.id === Number(id)) {
      selectEvent(null);
    }
    await reloadData();
  };

  const handleEventMove = async (
    occ: OccurrenceItem,
    newStartsAt: Date,
    newEndsAt: Date,
  ) => {
    const found = rawEvents().find((e) => String(e.id) === occ.event.id);
    if (!found) return;
    await dataSource.saveEvent(
      {
        calendarId: occ.event.calendarId,
        title: found.title,
        startsAt: newStartsAt.toISOString(),
        endsAt: newEndsAt.toISOString(),
        allDay: occ.occurrence.allDay,
        location: found.location || undefined,
        notes: found.notes || undefined,
      },
      String(found.id),
    );
    void reloadData();
  };

  const handleEventResize = async (occ: OccurrenceItem, newEndsAt: Date) => {
    const found = rawEvents().find((e) => String(e.id) === occ.event.id);
    if (!found) return;
    await dataSource.saveEvent(
      {
        calendarId: occ.event.calendarId,
        title: found.title,
        startsAt: occ.occurrence.startsAt,
        endsAt: newEndsAt.toISOString(),
        allDay: occ.occurrence.allDay,
        location: found.location || undefined,
        notes: found.notes || undefined,
      },
      String(found.id),
    );
    void reloadData();
  };

  const handleSearch = (q: string) => dataSource.search(q);

  const handleSelectSearchEvent = (evt: Event) => {
    const found = rawEvents().find((e) => String(e.id) === evt.id);
    if (found) selectEvent(found);
    const d = new Date(evt.startsAt);
    setFocusedDate(d);
    setSelectedDate(d);
    setIsSearchOpen(false);
  };

  const handleSelectSearchDate = (d: Date) => {
    setFocusedDate(d);
    setSelectedDate(d);
    setIsSearchOpen(false);
  };

  const handleExportIcs = (calId?: string) => dataSource.exportIcs(calId);

  const handleImportIcs = async (calId: string, content: string) => {
    await dataSource.importIcs(calId, content);
    await reloadData();
  };

  return (
    <section class="calendar-view" data-theme={theme()} aria-label="Calendar">
      {/* Quill Calendar Toolbar */}
      <div class="calendar-quill-toolbar">
        <div class="toolbar-left">
          <button
            type="button"
            class="quill-cal-btn quill-cal-btn--primary"
            onClick={() => handleOpenNewEvent()}
            title="Create new event (N or C)"
          >
            + Event
          </button>
          <button type="button" class="quill-cal-btn" onClick={handleToday} title="Jump to today (T)">
            Today
          </button>
          <div class="nav-arrows">
            <button type="button" class="quill-cal-icon-btn" onClick={handlePrev} title="Previous">
              ‹
            </button>
            <button type="button" class="quill-cal-icon-btn" onClick={handleNext} title="Next">
              ›
            </button>
          </div>
          <span class="toolbar-title">{formatTitle()}</span>
        </div>

        <div class="toolbar-right">
          <button
            type="button"
            class="quill-cal-icon-btn toolbar-action-btn"
            onClick={() => setIsSearchOpen(true)}
            title="Search events (/)"
          >
            🔍
          </button>
          <button
            type="button"
            class="quill-cal-icon-btn toolbar-action-btn"
            onClick={() => setIsIcsOpen(true)}
            title="Import / Export ICS"
          >
            📅
          </button>
          <button
            type="button"
            class="quill-cal-icon-btn toolbar-action-btn"
            onClick={() => setIsShortcutsOpen(true)}
            title="Keyboard shortcuts (?)"
          >
            ?
          </button>

          <div class="view-segmented">
            <For each={["Month", "Week", "3-day", "Day", "Agenda"] as ViewMode[]}>
              {(v) => (
                <button
                  type="button"
                  class={`view-segment-btn ${view() === v ? "active" : ""}`}
                  onClick={() => setView(v)}
                >
                  {v}
                </button>
              )}
            </For>
          </div>
        </div>
      </div>

      {/* Almanac Native SolidJS Calendar Views */}
      <div class="calendar-views-container">
        <Switch>
          <Match when={view() === "Month"}>
            <MonthView
              focusedDate={focusedDate()}
              selectedDate={selectedDate()}
              onSelectDate={setSelectedDate}
              onFocusedDateChange={setFocusedDate}
              occurrences={occurrences()}
              calendars={calendars()}
              onEventClick={handleEventClick}
              onCellClick={handleCellClick}
            />
          </Match>

          <Match when={view() === "Week"}>
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
            />
          </Match>

          <Match when={view() === "3-day"}>
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
            />
          </Match>

          <Match when={view() === "Day"}>
            <DayView
              focusedDate={focusedDate()}
              selectedDate={selectedDate()}
              onSelectDate={setSelectedDate}
              onFocusedDateChange={setFocusedDate}
              occurrences={occurrences()}
              calendars={calendars()}
              tasks={[]}
              onEventClick={handleEventClick}
              onSlotClick={handleSlotClick}
              onEventMove={handleEventMove}
              onEventResize={handleEventResize}
            />
          </Match>

          <Match when={view() === "Agenda"}>
            <AgendaView
              focusedDate={focusedDate()}
              selectedDate={selectedDate()}
              onSelectDate={setSelectedDate}
              onFocusedDateChange={setFocusedDate}
              occurrences={occurrences()}
              calendars={calendars()}
              tasks={[]}
              onEventClick={handleEventClick}
            />
          </Match>
        </Switch>
      </div>

      {/* Modals from @rcalendar/ui */}
      <EventEditorModal
        isOpen={isEditorOpen()}
        event={editingEvent()}
        initialDate={editorInitialDate()}
        calendars={calendars()}
        onSave={handleSaveModal}
        onDelete={handleDeleteModal}
        onClose={() => setIsEditorOpen(false)}
      />

      <SearchModal
        isOpen={isSearchOpen()}
        onClose={() => setIsSearchOpen(false)}
        onSearch={handleSearch}
        onSelectEvent={handleSelectSearchEvent}
        onSelectDate={handleSelectSearchDate}
      />

      <IcsImportExportModal
        isOpen={isIcsOpen()}
        onClose={() => setIsIcsOpen(false)}
        calendars={calendars()}
        onExport={handleExportIcs}
        onImport={handleImportIcs}
      />

      <ShortcutsHelpModal
        isOpen={isShortcutsOpen()}
        onClose={() => setIsShortcutsOpen(false)}
      />
    </section>
  );
}
