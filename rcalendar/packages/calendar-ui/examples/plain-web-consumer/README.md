# Plain Web Consumer Example

This example demonstrates using `@rcalendar/ui` in a pure web application with zero Tauri or SQLite dependencies.

## Key Takeaways

1. `@rcalendar/ui` components render against the `CalendarDataSource` interface.
2. `InMemoryCalendarDataSource` implements the interface locally without native desktop IPC.
3. No `@tauri-apps/api` dependencies are imported.
