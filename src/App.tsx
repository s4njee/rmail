import { onCleanup, onMount, Show } from "solid-js";
import { CalendarView } from "./components/calendar/CalendarView";
import { EventDetail } from "./components/calendar/EventDetail";
import { Composer } from "./components/Composer";
import { Divider } from "./components/Divider";
import { MessageList } from "./components/MessageList";
import { ReadingPane } from "./components/ReadingPane";
import { Settings } from "./components/Settings";
import { Sidebar } from "./components/Sidebar";
import { Titlebar } from "./components/Titlebar";
import {
  initPanes,
  persistPaneWidths,
  resizeList,
  resizeSidebar,
} from "./lib/panes";
import { initKeymap } from "./lib/keymap";
import { refreshMail } from "./lib/mail";
import { initSettings } from "./lib/settings";
import { initStoreEvents } from "./lib/store-events";
import { THEMES, applyTheme, isThemeName, useTheme } from "./lib/theme";
import { getSettings, patchSettings } from "./lib/tauri";
import {
  effectiveMode,
  initResponsive,
  useSection,
  useSettingsOpen,
} from "./lib/ui";
import "./App.css";

// The three-pane shell (Epic 4.1): titlebar over a horizontal workspace with
// the sidebar and message list fixed-width and the reading pane flexing.
// `data-theme` is the theme mechanism — Rust injects it before first paint,
// the signal seeds from it, and `data-theme={theme()}` keeps them in lockstep.
function App() {
  const theme = useTheme();

  onMount(() => {
    initStoreEvents();
    initResponsive();
    onCleanup(initKeymap());
    void initPanes();
    void initSettings();
    void refreshMail();

    // Belt and braces: confirm the persisted theme matches what the init
    // script stamped pre-paint. No-op when they agree.
    void getSettings().then((settings) => {
      if (isThemeName(settings.theme) && settings.theme !== theme()) {
        applyTheme(settings.theme);
      }
    });

    // Dev-only toggle until the real Settings control lands (Epic 10.3).
    // Shift+T cycles Hairline ↔ Banded and persists. Dead-code-eliminated
    // from release builds.
    if (import.meta.env.DEV) {
      const onKey = (event: KeyboardEvent) => {
        if (event.key === "T" && event.shiftKey) {
          const next =
            THEMES[(THEMES.indexOf(theme()) + 1) % THEMES.length] ?? "hairline";
          applyTheme(next);
          void patchSettings({ theme: next });
        }
      };
      window.addEventListener("keydown", onKey);
      onCleanup(() => window.removeEventListener("keydown", onKey));
    }
  });

  return (
    <div class="app" data-theme={theme()}>
      <Titlebar />
      <Show
        when={useSettingsOpen()()}
        fallback={
          // In focused mode the sidebar/list are hidden (CSS) so the reading
          // pane fills the window; in calendar mode the mail panes are hidden
          // and the calendar + event detail fill the area right of the
          // sidebar. Everything stays mounted so state is preserved.
          <div
            class="workspace"
            classList={{
              "workspace--focused": effectiveMode() === "focused",
              "workspace--calendar": useSection()() === "calendar",
            }}
          >
            <Sidebar />
            <div class="calendar-layout">
              <CalendarView />
              <EventDetail />
            </div>
            <Divider
              side="sidebar"
              onResize={(delta) => resizeSidebar(theme, delta)}
              onResizeEnd={persistPaneWidths}
            />
            <MessageList />
            <Divider
              side="list"
              onResize={(delta) => resizeList(theme, delta)}
              onResizeEnd={persistPaneWidths}
            />
            <ReadingPane focused={effectiveMode() === "focused"} />
          </div>
        }
      >
        <Settings />
      </Show>
      <Composer />
    </div>
  );
}

export default App;
