# `@rcalendar/ui`

> Framework-agnostic SolidJS calendar views and chrome components with a pluggable data source seam, designed for [Almanac](https://github.com/rcalendar/rcalendar).

[![npm](https://img.shields.io/npm/v/@rcalendar/ui.svg)](https://www.npmjs.com/package/@rcalendar/ui)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

## Architectural Guarantees

- **Zero Tauri / IPC dependencies**: `@rcalendar/ui` never imports `@tauri-apps/api` or desktop-specific libraries.
- **Pluggable Data Source**: All views render against the standard [`CalendarDataSource`](#the-calendardatasource-seam) interface.
- **Themeable Design Tokens**: Powered by custom properties (`tokens.css`) supporting both light and dark calendar tints.

## Installation

```bash
npm install @rcalendar/ui solid-js
# or
pnpm add @rcalendar/ui solid-js
```

## The `CalendarDataSource` Seam

To embed `@rcalendar/ui` in your application, supply an implementation of `CalendarDataSource`:

```typescript
import { CalendarDataSource, Calendar, Event, OccurrenceItem, Task } from "@rcalendar/ui";

export class CustomCalendarDataSource implements CalendarDataSource {
  async listAccounts() {
    return [
      {
        account: {
          id: "acc-1",
          displayName: "Default",
          kind: "local",
          status: "active",
          detail: "Local",
          createdAt: "",
          updatedAt: "",
        },
        calendars: [],
      },
    ];
  }

  async listCalendars(): Promise<Calendar[]> {
    return [
      {
        id: "cal-1",
        accountId: "acc-1",
        name: "Work",
        color: "#0F766E",
        enabled: true,
        eventCount: 1,
        createdAt: "",
        updatedAt: "",
      },
    ];
  }

  async listOccurrences(
    from: string,
    to: string,
    calendarIds?: string[],
  ): Promise<OccurrenceItem[]> {
    return [
      {
        occurrence: {
          id: "occ-1",
          eventId: "evt-1",
          calendarId: "cal-1",
          startsAt: "2026-08-13T10:00:00Z",
          endsAt: "2026-08-13T11:00:00Z",
          allDay: false,
        },
        event: {
          id: "evt-1",
          calendarId: "cal-1",
          uid: "evt-1@local",
          title: "Design Review",
          startsAt: "2026-08-13T10:00:00Z",
          endsAt: "2026-08-13T11:00:00Z",
          allDay: false,
          exdates: [],
          createdAt: "",
          updatedAt: "",
        },
      },
    ];
  }

  async getEvent(id: string) {
    return null;
  }
  async saveEvent(draft: any) {
    return [];
  }
  async deleteEvent(id: string) {
    return [];
  }
  async setCalendarEnabled(id: string, enabled: boolean) {}
  async listTasks() {
    return [];
  }
  async toggleTask(id: string) {
    throw new Error("Not implemented");
  }
  async search(query: string) {
    return { events: [], tasks: [] };
  }
  async exportIcs() {
    return "BEGIN:VCALENDAR...";
  }
  async importIcs(calId: string, ics: string) {
    return [];
  }
  async addAccount(spec: any) {
    throw new Error("Not implemented");
  }
  async syncAccount(id: string) {
    return { accountId: id, syncedAt: new Date().toISOString(), success: true, message: "OK" };
  }
  async setSyncInterval(mins: number) {}
}
```

## Embedding Views

```tsx
import { createSignal, onMount } from "solid-js";
import { MonthView, WeekView, Titlebar } from "@rcalendar/ui";
import "@rcalendar/ui/tokens.css";

export const App = () => {
  const dataSource = new CustomCalendarDataSource();
  const [date, setDate] = createSignal(new Date());
  const [occurrences, setOccurrences] = createSignal([]);
  const [calendars, setCalendars] = createSignal([]);

  onMount(async () => {
    const cals = await dataSource.listCalendars();
    setCalendars(cals);
    const items = await dataSource.listOccurrences("2026-08-01", "2026-08-31");
    setOccurrences(items);
  });

  return (
    <div style={{ height: "100vh", display: "flex", "flex-direction": "column" }}>
      <MonthView
        focusedDate={date()}
        selectedDate={date()}
        onSelectDate={setDate}
        onFocusedDateChange={setDate}
        occurrences={occurrences()}
        calendars={calendars()}
      />
    </div>
  );
};
```

## License

MIT © Almanac Contributors.
