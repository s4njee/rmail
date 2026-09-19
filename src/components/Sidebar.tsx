import { createSignal, For, onMount, Show } from "solid-js";
import { createStore } from "solid-js/store";
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
import { FolderTree } from "./FolderTree";
import {
  favourites,
  folderCount,
  isMailbox,
  loadAccountExpanded,
  mailboxFolders,
  matchFolders,
  recentFolders,
  saveAccountExpanded,
  unifiedFolders,
} from "../lib/folders";
import type { Folder } from "../lib/ipc/Folder";
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
  useAccountConnectivity,
  useConnectivity,
  useFootprintBytes,
} from "../lib/store-events";
import {
  createFolder,
  deleteFolder,
  deleteSavedSearch,
  moveFolder,
  removeAccount,
  renameFolder,
  setFolderExpanded,
  setFolderFavourite,
  setFolderSubscribed,
} from "../lib/tauri";
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
  const accountConnectivity = useAccountConnectivity();
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
  const [jump, setJump] = createSignal("");
  const [accountOpen, setAccountOpen] = createStore<Record<number, boolean>>(
    loadAccountExpanded(),
  );
  const [nameDialog, setNameDialog] = createSignal<{
    title: string;
    initial: string;
    onSubmit: (name: string) => void;
  } | null>(null);
  const [moveDialog, setMoveDialog] = createSignal<Folder | null>(null);
  const [nameValue, setNameValue] = createSignal("");

  onMount(() => void refreshSavedSearches());

  const confirmRemoveAccount = async () => {
    const account = confirmRemoving();
    if (account) {
      await removeAccount(account.id);
      await refreshMail();
    }
    setConfirmRemoving(null);
  };

  const handleFolderDrop = (folder: Folder, e: DragEvent) => {
    e.preventDefault();
    if (!isMailbox(folder)) return;
    const raw = e.dataTransfer?.getData("application/x-quill-message-ids");
    if (!raw) return;
    try {
      const ids = JSON.parse(raw) as number[];
      if (ids.length > 0) void moveMessages(ids, folder.path);
    } catch {
      /* not one of our drag payloads */
    }
  };

  const handleFolderDragOver = (folder: Folder, e: DragEvent) => {
    if (isMailbox(folder)) e.preventDefault();
  };

  const toggleAccount = (id: number) => {
    const next = !(accountOpen[id] ?? true);
    setAccountOpen(id, next);
    saveAccountExpanded({ ...accountOpen, [id]: next });
  };

  const accountStatus = (account: Account) => {
    const update = accountConnectivity()[account.id];
    if (update?.state === "syncing") return "Syncing…";
    if (account.last_error) {
      return /auth|login|credential|token/i.test(account.last_error)
        ? "Needs sign-in"
        : "Server unreachable";
    }
    if (update?.last_synced_at_ms != null) {
      return `Up to date · ${new Date(update.last_synced_at_ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}`;
    }
    return account.connected ? "Up to date" : "Not connected";
  };

  const toggleFolder = async (folder: Folder) => {
    const next = !folder.expanded;
    try {
      await setFolderExpanded(folder.id, next);
      await refreshMail();
    } catch {
      /* non-fatal */
    }
  };

  const askName = (
    title: string,
    initial: string,
    onSubmit: (name: string) => void,
  ) => {
    setNameValue(initial);
    setNameDialog({ title, initial, onSubmit });
  };

  const openFolderMenu = (folder: Folder, event: MouseEvent) => {
    event.preventDefault();
    if (folder.account_id == null) return;
    const items = [
      {
        label: "New subfolder…",
        onSelect: () =>
          askName("New folder", "", (name) => {
            if (!folder.account_id) return;
            void createFolder({
              accountId: folder.account_id,
              parentId: folder.id,
              name,
            }).then(refreshMail);
          }),
      },
      ...(folder.kind === "inbox"
        ? []
        : [
            {
              label: "Rename…",
              onSelect: () =>
                askName("Rename folder", folder.name, (name) => {
                  void renameFolder(folder.id, name).then(refreshMail);
                }),
            },
            {
              label: "Move…",
              onSelect: () => setMoveDialog(folder),
            },
          ]),
      {
        label: folder.favourite
          ? "Remove from favourites"
          : "Add to favourites",
        onSelect: () =>
          void setFolderFavourite(folder.id, !folder.favourite).then(
            refreshMail,
          ),
      },
      {
        label: folder.subscribed ? "Unsubscribe" : "Subscribe",
        onSelect: () =>
          void setFolderSubscribed(folder.id, !folder.subscribed).then(
            refreshMail,
          ),
      },
      ...(folder.kind === "inbox"
        ? []
        : [
            {
              label: "Delete folder…",
              danger: true,
              onSelect: () => {
                if (
                  window.confirm(
                    `Delete “${folder.name}” and move its mail to Trash?`,
                  )
                ) {
                  void deleteFolder(folder.id).then(refreshMail);
                }
              },
            },
          ]),
    ];
    openContextMenu(items, event.clientX, event.clientY);
  };

  const openAccountFolderMenu = (account: Account, event: MouseEvent) => {
    event.preventDefault();
    openContextMenu(
      [
        {
          label: "New folder…",
          onSelect: () =>
            askName("New folder", "", (name) => {
              void createFolder({
                accountId: account.id,
                parentId: null,
                name,
              }).then(refreshMail);
            }),
        },
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
          <div class="sidebar__jump">
            <input
              type="search"
              class="sidebar__jump-input"
              placeholder="Go to folder…"
              aria-label="Go to folder"
              value={jump()}
              onInput={(e) => setJump(e.currentTarget.value)}
            />
          </div>

          <Show when={jump().trim()}>
            <nav class="sidebar__folders" aria-label="Folder jump results">
              <For each={matchFolders(folders(), jump())}>
                {(folder) => (
                  <button
                    type="button"
                    class="sidebar__row"
                    classList={{ "is-selected": isFolderActive(folder.id) }}
                    onClick={() => {
                      selectFolder(folder.id);
                      setJump("");
                    }}
                  >
                    <span class="sidebar__row-text">
                      {folder.account_id != null ? folder.path : folder.name}
                    </span>
                  </button>
                )}
              </For>
            </nav>
          </Show>

          <Show when={!jump().trim()}>
            <h2 class="sidebar__label">Unified</h2>
            <nav class="sidebar__folders" aria-label="Folders">
              <For each={unifiedFolders(folders())}>
                {(folder) => {
                  const count = () => folderCount(folder, false);
                  return (
                    <button
                      type="button"
                      class="sidebar__row"
                      classList={{ "is-selected": isFolderActive(folder.id) }}
                      aria-current={
                        isFolderActive(folder.id) ? "true" : undefined
                      }
                      onClick={() => selectFolder(folder.id)}
                      onDragOver={(e) => handleFolderDragOver(folder, e)}
                      onDrop={(e) => handleFolderDrop(folder, e)}
                    >
                      <span class="sidebar__dot" aria-hidden="true" />
                      <span class="sidebar__row-text">{folder.name}</span>
                      <Show when={count()}>
                        {(c) => (
                          <span
                            class="sidebar__count tabular"
                            classList={{ "is-unread": c().unread }}
                          >
                            {c().text}
                          </span>
                        )}
                      </Show>
                    </button>
                  );
                }}
              </For>
            </nav>

            <Show when={favourites(folders()).length > 0}>
              <h2 class="sidebar__label sidebar__label--accounts">
                Favourites
              </h2>
              <nav class="sidebar__folders" aria-label="Favourite folders">
                <For each={favourites(folders())}>
                  {(folder) => (
                    <button
                      type="button"
                      class="sidebar__row"
                      classList={{ "is-selected": isFolderActive(folder.id) }}
                      onClick={() => selectFolder(folder.id)}
                      onDragOver={(e) => handleFolderDragOver(folder, e)}
                      onDrop={(e) => handleFolderDrop(folder, e)}
                      onContextMenu={(e) => openFolderMenu(folder, e)}
                    >
                      <span class="sidebar__fav" aria-hidden="true">
                        ★
                      </span>
                      <span class="sidebar__row-text">{folder.path}</span>
                    </button>
                  )}
                </For>
              </nav>
            </Show>

            <Show when={recentFolders(folders()).length > 0}>
              <h2 class="sidebar__label sidebar__label--accounts">Recent</h2>
              <nav class="sidebar__folders" aria-label="Recent folders">
                <For each={recentFolders(folders())}>
                  {(folder) => (
                    <button
                      type="button"
                      class="sidebar__row"
                      classList={{ "is-selected": isFolderActive(folder.id) }}
                      onClick={() => selectFolder(folder.id)}
                      onDragOver={(e) => handleFolderDragOver(folder, e)}
                      onDrop={(e) => handleFolderDrop(folder, e)}
                      onContextMenu={(e) => openFolderMenu(folder, e)}
                    >
                      <span class="sidebar__row-text">{folder.path}</span>
                    </button>
                  )}
                </For>
              </nav>
            </Show>

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
                          void deleteSavedSearch(s.id).then(
                            refreshSavedSearches,
                          );
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
                {(account) => {
                  const expanded = () => accountOpen[account.id] ?? true;
                  const selectedFolderId = () => {
                    const current = filter();
                    return current.kind === "folder" ? current.folderId : null;
                  };
                  return (
                    <div class="sidebar__account-block">
                      <button
                        type="button"
                        class="sidebar__account-row"
                        classList={{
                          "is-selected": isAccountActive(account.id),
                        }}
                        aria-current={
                          isAccountActive(account.id) ? "true" : undefined
                        }
                        aria-expanded={expanded()}
                        onClick={() => {
                          toggleAccount(account.id);
                          selectAccount(account.id);
                        }}
                        onContextMenu={(e) => openAccountFolderMenu(account, e)}
                      >
                        <span
                          class="sidebar__twistie"
                          classList={{ "is-open": expanded() }}
                          aria-hidden="true"
                        >
                          ▸
                        </span>
                        <span
                          class="sidebar__account-dot"
                          style={{ background: account.color }}
                          aria-hidden="true"
                        />
                        <span class="sidebar__account-address">
                          {account.address}
                        </span>
                        <span
                          class="sidebar__account-status"
                          title={accountStatus(account)}
                        >
                          {accountStatus(account)}
                        </span>
                      </button>
                      <Show when={expanded()}>
                        <FolderTree
                          folders={mailboxFolders(folders())}
                          accountId={account.id}
                          selectedId={selectedFolderId()}
                          onSelect={(f) => selectFolder(f.id)}
                          onToggle={(f) => void toggleFolder(f)}
                          onContextMenu={openFolderMenu}
                          onDragOver={handleFolderDragOver}
                          onDrop={handleFolderDrop}
                        />
                      </Show>
                    </div>
                  );
                }}
              </For>
            </nav>
          </Show>
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

      <Show when={nameDialog()}>
        {(d) => (
          <div class="account-confirm" role="dialog" aria-label={d().title}>
            <span class="account-confirm__text">{d().title}</span>
            <input
              class="sidebar__jump-input"
              value={nameValue()}
              onInput={(e) => setNameValue(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  const name = nameValue().trim();
                  if (name) d().onSubmit(name);
                  setNameDialog(null);
                }
                if (e.key === "Escape") setNameDialog(null);
              }}
              autofocus
            />
            <button
              type="button"
              class="btn btn--secondary"
              onClick={() => setNameDialog(null)}
            >
              Cancel
            </button>
            <button
              type="button"
              class="btn btn--primary"
              onClick={() => {
                const name = nameValue().trim();
                if (name) d().onSubmit(name);
                setNameDialog(null);
              }}
            >
              Save
            </button>
          </div>
        )}
      </Show>

      <Show when={moveDialog()}>
        {(folder) => (
          <div class="account-confirm" role="dialog" aria-label="Move folder">
            <span class="account-confirm__text">
              Move “{folder().name}” under…
            </span>
            <select
              class="sidebar__jump-input"
              onChange={(e) => {
                const raw = e.currentTarget.value;
                const parentId = raw === "" ? null : Number(raw);
                void moveFolder(folder().id, parentId).then(refreshMail);
                setMoveDialog(null);
              }}
            >
              <option value="">(top level)</option>
              <For
                each={mailboxFolders(folders()).filter(
                  (f) =>
                    f.account_id === folder().account_id &&
                    f.id !== folder().id,
                )}
              >
                {(f) => <option value={f.id}>{f.path}</option>}
              </For>
            </select>
            <button
              type="button"
              class="btn btn--secondary"
              onClick={() => setMoveDialog(null)}
            >
              Cancel
            </button>
          </div>
        )}
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
