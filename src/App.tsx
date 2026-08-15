import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { AccountEditModal } from "./components/AccountEditModal";
import { AlarmBanner } from "./components/AlarmBanner";
import { CalendarUndoBar } from "./components/CalendarUndoBar";
import { CalendarView } from "./components/calendar/CalendarView";
import { ContextMenu } from "./components/ContextMenu";
import { EventDetail } from "./components/calendar/EventDetail";
import { Composer } from "./components/Composer";
import { Divider } from "./components/Divider";
import { MessageList } from "./components/MessageList";
import { ReadingPane } from "./components/ReadingPane";
import { Settings } from "./components/Settings";
import { Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { TriageUndoBar } from "./components/TriageUndoBar";
import { UndoSendBar } from "./components/UndoSendBar";
import {
  initPanes,
  persistPaneWidths,
  resizeList,
  resizeSidebar,
} from "./lib/panes";
import { startAlarmScheduler } from "./lib/alarms";
import { initKeymap } from "./lib/keymap";
import { refreshMail, useAccounts } from "./lib/mail";
import { initSettings } from "./lib/settings";
import { initStoreEvents } from "./lib/store-events";
import { THEMES, applyTheme, isThemeName, useTheme } from "./lib/theme";
import { getSettings, listEvents, patchSettings } from "./lib/tauri";
import {
  checkForUpdates,
  currentVersion,
  lastSeenVersion,
  markVersionSeen,
  notesFor,
  useUpdateReadyVersion,
} from "./lib/updates";
import {
  effectiveMode,
  initResponsive,
  openSettingsAt,
  useSection,
  useSettingsOpen,
} from "./lib/ui";
import { WhatsNewModal } from "./components/WhatsNewModal";
import { Onboarding } from "./components/onboarding/Onboarding";
import "./App.css";

// Diagnostics nudge (E2.3): a one-time, dismissible pointer at the opt-in
// toggles in Settings → Diagnostics. Never auto-sends anything — consent is
// a manual, default-off choice.
const DIAGNOSTICS_NUDGE_KEY = "quill_diagnostics_seen";

function shouldShowDiagnosticsNudge(): boolean {
  try {
    return localStorage.getItem(DIAGNOSTICS_NUDGE_KEY) !== "1";
  } catch {
    return false;
  }
}

// The three-pane shell (Epic 4.1): a horizontal workspace with the sidebar
// and message list fixed-width and the reading pane flexing. `data-theme` is
// the theme mechanism — Rust injects it before first paint, the signal seeds
// from it, and `data-theme={theme()}` keeps them in lockstep.
function App() {
  const theme = useTheme();
  const accounts = useAccounts();
  // First-run gate (P0.2): until accounts load we show nothing, then either
  // the onboarding (no accounts + never dismissed) or the app.
  const [gate, setGate] = createSignal<"loading" | "onboarding" | "app">(
    "loading",
  );
  const [whatsNewVersion, setWhatsNewVersion] = createSignal<string | null>(
    null,
  );
  const updateReady = useUpdateReadyVersion();

  // One-time diagnostics nudge (E2.3): shown on first launch after this ships,
  // dismissible, points at the opt-in toggles. localStorage-gated so it shows
  // once and never again.
  const [diagnosticsNudgeVisible, setDiagnosticsNudgeVisible] = createSignal(
    shouldShowDiagnosticsNudge(),
  );

  function dismissDiagnosticsNudge(): void {
    try {
      localStorage.setItem(DIAGNOSTICS_NUDGE_KEY, "1");
    } catch {
      /* ignore */
    }
    setDiagnosticsNudgeVisible(false);
  }

  onMount(() => {
    initStoreEvents();
    initResponsive();
    // Auto-update (E2.2): silent check for a signed update, and show "What's
    // new" on first launch after an upgrade.
    void checkForUpdates();
    void currentVersion().then((version) => {
      if (lastSeenVersion() !== version && notesFor(version)) {
        setWhatsNewVersion(version);
      }
      markVersionSeen(version);
    });
    onCleanup(initKeymap());
    void initPanes();
    void initSettings();
    // Decide the first-run gate once accounts are known: no accounts and the
    // onboarding wasn't dismissed → show it (P0.2).
    void refreshMail().then(() => {
      let dismissed = "1";
      try {
        dismissed = localStorage.getItem("quill_setup_done") ?? "0";
      } catch {
        /* ignore */
      }
      setGate(
        accounts().length > 0 || dismissed === "1" ? "app" : "onboarding",
      );
    });

    // Start background calendar alarms scheduler (Roadmap 4.2)
    const stopScheduler = startAlarmScheduler(() =>
      listEvents(Date.now() - 3600000, Date.now() + 86400000 * 7),
    );
    onCleanup(stopScheduler);

    // Belt and braces: confirm the persisted theme matches what the init
    // script stamped pre-paint. No-op when they agree.
    void getSettings().then((settings) => {
      if (isThemeName(settings.theme) && settings.theme !== theme()) {
        applyTheme(settings.theme);
      }
    });

    // Global drop blocker: prevent the webview from navigating to dropped files (AC 13.3).
    const blockGlobalDrop = (e: DragEvent) => {
      e.preventDefault();
    };
    window.addEventListener("dragover", blockGlobalDrop);
    window.addEventListener("drop", blockGlobalDrop);
    onCleanup(() => {
      window.removeEventListener("dragover", blockGlobalDrop);
      window.removeEventListener("drop", blockGlobalDrop);
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
      <Show
        when={gate() === "app"}
        fallback={
          gate() === "onboarding" ? (
            <Onboarding onDone={() => setGate("app")} />
          ) : null
        }
      >
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
        <AlarmBanner />
        <UndoSendBar />
        <TriageUndoBar />
        <CalendarUndoBar />
        <StatusBar />
        <ContextMenu />
        <AccountEditModal />

        {/* "What's new" after an update, and the restart-to-apply banner once an
          update is downloaded. */}
        <Show when={whatsNewVersion()}>
          {(version) => (
            <WhatsNewModal
              version={version()}
              onClose={() => setWhatsNewVersion(null)}
            />
          )}
        </Show>
        <Show when={updateReady()}>
          {(version) => (
            <div class="update-ready" role="status">
              Update v{version()} downloaded — restart Quill to apply it.
            </div>
          )}
        </Show>

        {/* Diagnostics & privacy nudge (E2.3) — one-time, dismissible. */}
        <Show when={diagnosticsNudgeVisible()}>
          <div class="diagnostics-nudge" role="status">
            <span class="diagnostics-nudge__text">
              Diagnostics & privacy: review what Quill may send, and where logs
              and crash reports live.
            </span>
            <button
              type="button"
              class="btn btn--secondary btn--sm"
              onClick={() => {
                dismissDiagnosticsNudge();
                openSettingsAt("diagnostics");
              }}
            >
              Review
            </button>
            <button
              type="button"
              class="diagnostics-nudge__dismiss"
              aria-label="Dismiss"
              onClick={dismissDiagnosticsNudge}
            >
              ×
            </button>
          </div>
        </Show>
      </Show>
    </div>
  );
}

export default App;
