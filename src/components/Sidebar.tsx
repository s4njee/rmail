import { createSignal, For, onMount, Show } from "solid-js";
import { Calendar, Sidebar as CalendarSidebar } from "@rcalendar/ui";
import {
  calendarList,
  removeSourceCalendar,
  requestNewEvent,
  setCalendarFocusedDate,
  setCalendarSelectedDate,
  toggleCalendarTask,
  useCalendarFocusedDate,
  useCalendarSelectedDate,
  useCalendarTasks,
} from "../lib/calendar";
import {
  parseCalendarId,
  setCalendarEnabled,
  setHiddenFromSidebar,
} from "../lib/calendarAdapter";
import { openNewComposer } from "../lib/compose";
import { openContextMenu } from "../lib/context-menu";
import { formatBytes } from "../lib/format";
import type { Account } from "../lib/ipc/Account";
import {
  moveMessages,
  openAccountEdit,
  refreshMail,
  refreshSavedSearches,
  runSearchQuery,
  selectAccount,
  selectFolder,
  useAccounts,
  useFilter,
  useFolders,
  useSavedSearches,
} from "../lib/mail";
import { effectiveSidebarWidth } from "../lib/panes";
import {
  connectivityText,
  useConnectivity,
  useFootprintBytes,
} from "../lib/store-events";
import { deleteSavedSearch, removeAccount } from "../lib/tauri";
import { useTheme } from "../lib/theme";
import { openSettings, switchSection, useSection } from "../lib/ui";
import { ScheduledView } from "./ScheduledView";
import "./Sidebar.css";

// The sidebar (Epic 5): wordmark, folder rows with live counts, account rows,
// and the footer (Hairline connectivity status) / "On this device" card
// (Banded footprint). Folder/account clicks set the filter the message list
// consumes. Structural differences (footer vs card, folder dot presence)
// follow Epic 2.2 — one branch on useTheme() each.
export function Sidebar() {
  const theme = useTheme();
  const folders = useFolders();
  const accounts = useAccounts();
  const savedSearches = useSavedSearches();
  const filter = useFilter();
  const connectivity = useConnectivity();
  const footprintBytes = useFootprintBytes();
  const section = useSection();
  // Calendar navigation state is shared with CalendarView (lib/calendar), so
  // the embedded calendar sidebar below drives the same focused/selected date.
  const calFocusedDate = useCalendarFocusedDate();
  const calSelectedDate = useCalendarSelectedDate();
  const calTasks = useCalendarTasks();
  const [confirmRemoving, setConfirmRemoving] = createSignal<Account | null>(
    null,
  );
  const [scheduledOpen, setScheduledOpen] = createSignal(false);

  onMount(() => void refreshSavedSearches());

  const confirmRemoveAccount = async () => {
    const account = confirmRemoving();
    if (account) {
      await removeAccount(account.id);
      await refreshMail();
    }
    setConfirmRemoving(null);
  };

  // P1.1 drag-and-drop: dropping message rows on a real folder moves them.
  // Derived views (Starred, Snoozed) aren't mailboxes and don't accept drops.
  const isMailbox = (name: string) => name !== "Starred" && name !== "Snoozed";

  const handleFolderDrop = (folderName: string, e: DragEvent) => {
    e.preventDefault();
    if (!isMailbox(folderName)) return;
    const raw = e.dataTransfer?.getData("application/x-quill-message-ids");
    if (!raw) return;
    try {
      const ids = JSON.parse(raw) as number[];
      if (ids.length > 0) void moveMessages(ids, folderName);
    } catch {
      /* not one of our drag payloads */
    }
  };

  const handleFolderDragOver = (folderName: string, e: DragEvent) => {
    if (isMailbox(folderName)) e.preventDefault();
  };

  // Right-click on an account row: Edit (shared dialog) or Delete (confirm).
  const openAccountMenu = (account: Account, event: MouseEvent) => {
    event.preventDefault();
    openContextMenu(
      [
        { label: "Edit account…", onSelect: () => openAccountEdit(account) },
        {
          label: "Delete account…",
          danger: true,
          onSelect: () => setConfirmRemoving(account),
        },
      ],
      event.clientX,
      event.clientY,
    );
  };

  // Right-click on a calendar row: "Remove from sidebar" (hides it here, stays
  // in Settings) for every calendar, plus the destructive option per type.
  const handleCalendarContextMenu = (cal: Calendar, event: MouseEvent) => {
    event.preventDefault();
    const { accountId, source } = parseCalendarId(cal.id);
    const removeFromSidebar = {
      label: "Remove from sidebar",
      onSelect: () => setHiddenFromSidebar(cal.id, true),
    };
    if (source) {
      openContextMenu(
        [
          removeFromSidebar,
          {
            label: "Remove calendar",
            danger: true,
            onSelect: () => void removeSourceCalendar(accountId, source),
          },
        ],
        event.clientX,
        event.clientY,
      );
    } else {
      const account = accounts().find((a) => a.id === accountId);
      openContextMenu(
        [
          removeFromSidebar,
          {
            label: "Edit account…",
            onSelect: () => account && openAccountEdit(account),
          },
          {
            label: "Delete account…",
            danger: true,
            onSelect: () => account && setConfirmRemoving(account),
          },
        ],
        event.clientX,
        event.clientY,
      );
    }
  };

  const isFolderActive = (id: number) => {
    const current = filter();
    return current.kind === "folder" && current.folderId === id;
  };
  const isAccountActive = (id: number) => {
    const current = filter();
    return current.kind === "account" && current.accountId === id;
  };

  return (
    <aside
      class="sidebar"
      aria-label="Sidebar"
      style={{ width: `${effectiveSidebarWidth(theme)}px` }}
    >
      {/* Mail / Calendar section switch (Epic 14.6) — extrapolated. */}
      <div class="sidebar__section-switch" role="tablist" aria-label="Section">
        <button
          type="button"
          class="sidebar__section-tab"
          classList={{ "is-selected": section() === "mail" }}
          role="tab"
          aria-selected={section() === "mail"}
          onClick={() => switchSection("mail")}
        >
          Mail
        </button>
        <button
          type="button"
          class="sidebar__section-tab"
          classList={{ "is-selected": section() === "calendar" }}
          role="tab"
          aria-selected={section() === "calendar"}
          onClick={() => switchSection("calendar")}
        >
          Calendar
        </button>
      </div>

      <Show when={section() === "mail"}>
        <button
          type="button"
          class="sidebar__compose-btn"
          onClick={() => void openNewComposer()}
        >
          <span aria-hidden="true">✎</span> New message
        </button>

        <div class="sidebar__navs">
          <h2 class="sidebar__label">Unified</h2>
          <nav class="sidebar__folders" aria-label="Folders">
            <For each={folders()}>
              {(folder) => (
                <button
                  type="button"
                  class="sidebar__row"
                  classList={{ "is-selected": isFolderActive(folder.id) }}
                  aria-current={isFolderActive(folder.id) ? "true" : undefined}
                  onClick={() => selectFolder(folder.id)}
                  onDragOver={(e) => handleFolderDragOver(folder.name, e)}
                  onDrop={(e) => handleFolderDrop(folder.name, e)}
                >
                  <span class="sidebar__dot" aria-hidden="true" />
                  <span class="sidebar__row-text">{folder.name}</span>
                  <Show when={folder.total_count > 0}>
                    <span class="sidebar__count tabular">
                      {folder.total_count}
                    </span>
                  </Show>
                </button>
              )}
            </For>
          </nav>

          {/* P1.3 saved searches — persistent virtual folders. */}
          <Show when={savedSearches().length > 0}>
            <h2 class="sidebar__label sidebar__label--accounts">
              Saved searches
            </h2>
            <nav class="sidebar__folders" aria-label="Saved searches">
              <For each={savedSearches()}>
                {(s) => (
                  <div class="sidebar__saved-row">
                    <button
                      type="button"
                      class="sidebar__row sidebar__saved-open"
                      onClick={() => runSearchQuery(s.query)}
                      title={s.query}
                    >
                      <span class="sidebar__row-text">{s.name}</span>
                    </button>
                    <button
                      type="button"
                      class="sidebar__saved-remove"
                      aria-label={`Delete saved search ${s.name}`}
                      onClick={() => {
                        void deleteSavedSearch(s.id).then(refreshSavedSearches);
                      }}
                    >
                      ×
                    </button>
                  </div>
                )}
              </For>
            </nav>
          </Show>

          {/* P1.1 send-later Outbox — opens the Scheduled list (not a folder). */}
          <button
            type="button"
            class="sidebar__row sidebar__row--scheduled"
            onClick={() => setScheduledOpen(true)}
          >
            <span class="sidebar__dot" aria-hidden="true" />
            <span class="sidebar__row-text">Scheduled</span>
          </button>

          <h2 class="sidebar__label sidebar__label--accounts">Accounts</h2>
          <nav class="sidebar__accounts" aria-label="Accounts">
            <For each={accounts()}>
              {(account) => (
                <button
                  type="button"
                  class="sidebar__account-row"
                  classList={{ "is-selected": isAccountActive(account.id) }}
                  aria-current={
                    isAccountActive(account.id) ? "true" : undefined
                  }
                  onClick={() => selectAccount(account.id)}
                  onContextMenu={(e) => openAccountMenu(account, e)}
                >
                  <span
                    class="sidebar__account-dot"
                    style={{ background: account.color }}
                    aria-hidden="true"
                  />
                  <span class="sidebar__account-address">
                    {account.address}
                  </span>
                </button>
              )}
            </For>
          </nav>
        </div>

        {theme() === "banded" ? (
          <div class="sidebar-card">
            <div class="sidebar-card__title">On this device</div>
            <div class="sidebar-card__line">
              {formatBytes(footprintBytes())} mail cache
            </div>
          </div>
        ) : (
          <div
            class="sidebar-footer"
            role="status"
            data-connectivity={connectivity().state}
          >
            <span class="sidebar-footer__dot" aria-hidden="true" />
            <span>{connectivityText(connectivity())}</span>
          </div>
        )}
      </Show>

      {/* Calendar content lives in the same sidebar, below the section toggle
          (rather than a second column) — the Almanac sidebar embeds here. */}
      <Show when={section() === "calendar"}>
        <CalendarSidebar
          fill
          focusedDate={calFocusedDate()}
          selectedDate={calSelectedDate()}
          onSelectDate={(d) => {
            setCalendarSelectedDate(d);
            setCalendarFocusedDate(d);
          }}
          onFocusedDateChange={setCalendarFocusedDate}
          calendars={calendarList()}
          onToggleCalendar={(id, enabled) => setCalendarEnabled(id, enabled)}
          tasks={calTasks()}
          onToggleTask={(id) => void toggleCalendarTask(id)}
          onAddTask={() => requestNewEvent()}
          onSettingsClick={openSettings}
          onCalendarContextMenu={handleCalendarContextMenu}
        />
      </Show>

      {/* P1.1 send-later Outbox */}
      <Show when={scheduledOpen()}>
        <ScheduledView onClose={() => setScheduledOpen(false)} />
      </Show>

      {/* Delete-account confirm (reached from a right-click menu) */}
      <Show when={confirmRemoving()}>
        <div
          class="account-confirm"
          role="alertdialog"
          aria-label="Remove account"
        >
          <span class="account-confirm__text">
            Delete local mail and calendar data for {confirmRemoving()?.address}
            ? This cannot be undone.
          </span>
          <button
            type="button"
            class="btn btn--secondary"
            onClick={() => setConfirmRemoving(null)}
          >
            Cancel
          </button>
          <button
            type="button"
            class="btn btn--primary"
            onClick={() => void confirmRemoveAccount()}
          >
            Delete
          </button>
        </div>
      </Show>
    </aside>
  );
}
