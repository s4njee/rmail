/**
 * `@rcalendar/ui` — Almanac's SolidJS component library.
 *
 * Framework-agnostic calendar components with pluggable data sources (plan.md §8).
 */

// Package Version
export const VERSION = "0.1.0";

// Styles & Tokens
import "./tokens/tokens.css";

// Types & Data Source Seam
export * from "./types/calendar";

// Headless Helpers & Layout / Interaction Engine
export * from "./headless/dateUtils";
export * from "./headless/layout";
export * from "./headless/dragEngine";

// Chrome & Modals
export * from "./components/Titlebar";
export * from "./components/Sidebar";
export * from "./components/EventEditorModal";
export * from "./components/SearchModal";
export * from "./components/ShortcutsHelpModal";
export * from "./components/IcsImportExportModal";
export * from "./components/GoogleConnectModal";

// Views
export * from "./views/MonthView";
export * from "./views/WeekView";
export * from "./views/ThreeDayView";
export * from "./views/DayView";
export * from "./views/AgendaView";
export * from "./views/SettingsView";
export * from "./views/YearView";
