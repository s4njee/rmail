import { createSignal, onMount, render } from "solid-js";
import { Calendar, MonthView, OccurrenceItem, Titlebar, ViewMode, WeekView } from "@rcalendar/ui";
import "@rcalendar/ui/tokens.css";
import { InMemoryCalendarDataSource } from "./inMemoryAdapter";

const App = () => {
  const dataSource = new InMemoryCalendarDataSource();
  const [view, setView] = createSignal<ViewMode>("Month");
  const [date, setDate] = createSignal(new Date(2026, 7, 13));
  const [calendars, setCalendars] = createSignal<Calendar[]>([]);
  const [occurrences, setOccurrences] = createSignal<OccurrenceItem[]>([]);

  onMount(async () => {
    const cals = await dataSource.listCalendars();
    setCalendars(cals);
    const occs = await dataSource.listOccurrences("2026-08-01", "2026-08-31");
    setOccurrences(occs);
  });

  return (
    <div style={{ height: "100vh", width: "100vw", display: "flex", "flex-direction": "column" }}>
      <Titlebar
        activeView={view()}
        onViewChange={setView}
        onNewEvent={() => alert("New Event clicked")}
        onSearchClick={() => alert("Search clicked")}
      />
      <div style={{ flex: 1, display: "flex", overflow: "hidden" }}>
        {view() === "Month" ? (
          <MonthView
            focusedDate={date()}
            selectedDate={date()}
            onSelectDate={setDate}
            onFocusedDateChange={setDate}
            occurrences={occurrences()}
            calendars={calendars()}
          />
        ) : (
          <WeekView
            focusedDate={date()}
            selectedDate={date()}
            onSelectDate={setDate}
            onFocusedDateChange={setDate}
            occurrences={occurrences()}
            calendars={calendars()}
          />
        )}
      </div>
    </div>
  );
};

render(() => <App />, document.getElementById("root")!);
