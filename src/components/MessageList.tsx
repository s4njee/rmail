import { createVirtualizer } from "@tanstack/solid-virtual";
import { createEffect, createSignal, For, Show, untrack } from "solid-js";
import {
  clearSearch,
  loadRows,
  moveMessages,
  refreshSavedSearches,
  saveCurrentSearch,
  selectMessage,
  selectRangeTo,
  setListEl,
  setSearchEl,
  snooze,
  toggleMultiSelect,
  triage,
  updateSearch,
  useAccounts,
  useFilter,
  useFolders,
  useIsSearching,
  useMultiSelectedIds,
  useRows,
  useSearchQuery,
  useSearchResults,
  useSelectedId,
} from "../lib/mail";
import { effectiveListWidth } from "../lib/panes";
import { syncNow } from "../lib/tauri";
import { useTheme } from "../lib/theme";
import { formatRelativeTime } from "../lib/format";
import { openContextMenu } from "../lib/context-menu";
import type { MessageRow as MessageRowData } from "../lib/ipc/MessageRow";
import { openShortcuts } from "../lib/shortcuts";
import { BulkActionBar } from "./BulkActionBar";
import { MessageRow } from "./MessageRow";
import { SearchHelp } from "./SearchHelp";
import { tomorrowMorning } from "./SnoozeMenu";
import "./MessageList.css";

const ROW_H = 80; // keep in sync with --row-h in tokens.css
const BANDED_ROW_STEP = ROW_H + 2; // rows carry a 2px margin-bottom in Banded

// P1.5: per-filter scroll position persists across restarts.
function scrollKey(key: string): string {
  return `quill_scroll_${key}`;
}
function persistedScroll(key: string): number | null {
  try {
    const raw = localStorage.getItem(scrollKey(key));
    return raw == null ? null : Number(raw);
  } catch {
    return null;
  }
}
function saveScroll(key: string, top: number): void {
  try {
    localStorage.setItem(scrollKey(key), String(top));
  } catch {
    /* ignore */
  }
}

function HighlightedSnippet(props: { text: string }) {
  const parts = () => {
    const raw = props.text;
    const regex = /<mark>(.*?)<\/mark>/g;
    const pieces: { text: string; highlight: boolean }[] = [];
    let lastIndex = 0;
    let match: RegExpExecArray | null;

    while ((match = regex.exec(raw)) !== null) {
      if (match.index > lastIndex) {
        pieces.push({
          text: raw.slice(lastIndex, match.index),
          highlight: false,
        });
      }
      pieces.push({ text: match[1] ?? "", highlight: true });
      lastIndex = regex.lastIndex;
    }
    if (lastIndex < raw.length) {
      pieces.push({ text: raw.slice(lastIndex), highlight: false });
    }
    return pieces;
  };

  return (
    <div class="search-match__snippet">
      <For each={parts()}>
        {(part) =>
          part.highlight ? <mark>{part.text}</mark> : <span>{part.text}</span>
        }
      </For>
    </div>
  );
}

// The message list (Epic 6 & Epic 15): a header reflecting the active filter /
// search query, a search field, and a virtualized list of 72px rows paged from
// the store or ranked search results.
export function MessageList() {
  const theme = useTheme();
  const folders = useFolders();
  const accounts = useAccounts();
  const filter = useFilter();
  const rows = useRows();
  const selectedId = useSelectedId();
  const multiSelectedIds = useMultiSelectedIds();
  const searchQuery = useSearchQuery();
  const searchResults = useSearchResults();
  const isSearching = useIsSearching();

  const [scrollEl, setScrollEl] = createSignal<HTMLDivElement | null>(null);

  // One ref feeds the virtualizer's scroll element and the keymap's listbox
  // focus check (Solid clears refs with null on unmount).
  const assignScrollRef = (el: HTMLDivElement | null) => {
    setScrollEl(el);
    setListEl(el);
  };

  // Scroll offsets remembered per filter, so each folder/account restores its
  // own position (6.3). Defined once — the component body runs once.
  const scrollOffsets: Record<string, number> = {};
  let prevKey: string | undefined;

  const filterKey = () => {
    const current = filter();
    return current.kind === "folder"
      ? `folder:${current.folderId}`
      : `account:${current.accountId}`;
  };

  createEffect(() => {
    const key = filterKey();
    filter(); // track
    const el = scrollEl();
    if (folders().length === 0) return; // wait for the sidebar's folders
    if (key !== prevKey) {
      if (prevKey != null && el) scrollOffsets[prevKey] = el.scrollTop;
      prevKey = key;
      // Capture the key; if the user switches filters while the fetch is in
      // flight, loadRows discards the result but this `.then` would still
      // restore the *old* filter's scroll offset over the new one.
      const keyAtLoad = key;
      void loadRows().then(() => {
        // `el` was captured in the tracking scope above. P1.5: restore the
        // last scroll for this filter from the in-memory map or localStorage.
        if (el && filterKey() === keyAtLoad) {
          const stored = scrollOffsets[keyAtLoad] ?? persistedScroll(keyAtLoad);
          el.scrollTop = stored ?? 0;
        }
      });
    } else {
      // Folders refreshed (e.g. after a mark-read) — re-sync rows, keep scroll.
      void loadRows();
    }
  });

  const rowStep = () => (theme() === "banded" ? BANDED_ROW_STEP : ROW_H);
  const virtualizer = createVirtualizer({
    get count() {
      return rows.length;
    },
    initialRect: { width: 400, height: 600 },
    // Only hand the virtualizer a scroll element once it is actually in the
    // document. The ref fires while the element is still detached (created
    // during render), and TanStack caches the target window from the first
    // observation — observing a detached element whose document has no
    // `defaultView` leaves `targetWindow` null, so the scroll/rect observers
    // never attach and the list freezes (rows don't update on scroll).
    // The adapter re-runs `_willUpdate` from its own `onMount` after the DOM
    // is connected, which is when the wiring actually happens.
    getScrollElement: () => {
      const el = scrollEl();
      return el && el.isConnected ? el : null;
    },
    estimateSize: () => rowStep(),
    overscan: 5,
  });

  // Keep the selection in view with a ≥1-row margin as the keyboard moves it
  // (Epic 9.2) — via the virtualizer, never scrollIntoView jumps.
  //
  // This must fire only when the *selection* changes. Both the virtual window
  // (`getVirtualItems()`) and the rows array change for reasons that have
  // nothing to do with the selection — the user scrolling, mark-read
  // refreshing folders, new mail arriving — and re-running then would drag
  // the scroll back to the selection mid-scroll. So the window is read with
  // `untrack` (not tracked), and a handled-id guard skips re-fires while the
  // selection is unchanged.
  let lastHandledId: number | null = null;
  createEffect(() => {
    if (isSearching()) return;
    const id = selectedId();
    if (id == null) {
      lastHandledId = null;
      return;
    }
    const idx = rows.findIndex((r) => r.id === id);
    if (idx < 0) return;
    if (id === lastHandledId) return;
    lastHandledId = id;
    const items = untrack(() => virtualizer.getVirtualItems());
    const first = items[0]?.index;
    const last = items[items.length - 1]?.index;
    if (first == null || last == null) return;
    const margin = 1;
    if (idx < first + margin) {
      virtualizer.scrollToIndex(Math.max(idx - margin, 0), { align: "start" });
    } else if (idx > last - margin) {
      virtualizer.scrollToIndex(Math.min(idx + margin, rows.length - 1), {
        align: "end",
      });
    }
  });

  // Selecting with the mouse also hands keyboard focus to the listbox, so the
  // arrow keys keep working afterwards. Ctrl/Cmd+click toggles bulk
  // membership; Shift+click extends the bulk range (P1.1).
  const handleSelect = (id: number) => {
    scrollEl()?.focus();
    selectMessage(id);
  };
  const handleToggleSelect = (id: number) => {
    scrollEl()?.focus();
    toggleMultiSelect(id);
  };
  const handleSelectRange = (id: number) => {
    scrollEl()?.focus();
    selectRangeTo(id);
  };

  // P1.1 right-click triage menu for one or many selected rows. "Move to…"
  // opens a second menu at the same position.
  const handleMessageContextMenu = (row: MessageRowData, event: MouseEvent) => {
    event.preventDefault();
    const ids = multiSelectedIds().has(row.id)
      ? [...multiSelectedIds()]
      : [row.id];
    const destinations = () =>
      folders()
        .filter((f) => f.name !== "Starred" && f.name !== "Snoozed")
        .map((f) => ({
          label: f.name,
          onSelect: () => void moveMessages(ids, f.name),
        }));
    openContextMenu(
      [
        {
          label: row.unread ? "Mark read" : "Mark unread",
          onSelect: () =>
            void triage(ids, row.unread ? "markRead" : "markUnread"),
        },
        {
          label: row.flagged ? "Unstar" : "Star",
          onSelect: () => void triage(ids, row.flagged ? "unstar" : "star"),
        },
        { label: "Archive", onSelect: () => void triage(ids, "archive") },
        {
          label: row.folder === "Junk" ? "Not junk" : "Mark junk",
          onSelect: () =>
            void triage(
              ids,
              row.folder === "Junk" ? "markNotJunk" : "markJunk",
            ),
        },
        {
          label: "Snooze 1 hour",
          onSelect: () => void snooze(ids, Date.now() + 3600_000),
        },
        {
          label: "Snooze until tomorrow 9am",
          onSelect: () => void snooze(ids, tomorrowMorning()),
        },
        {
          label: "Move to…",
          onSelect: () =>
            openContextMenu(destinations(), event.clientX, event.clientY),
        },
        {
          label: "Delete",
          danger: true,
          onSelect: () => void triage(ids, "delete"),
        },
      ],
      event.clientX,
      event.clientY,
    );
  };

  // P1.1 drag-and-drop: the drag carries the multi-selection (or the row) as
  // message ids; sidebar folders are the drop targets.
  const handleRowDragStart = (row: MessageRowData, event: DragEvent) => {
    const ids = multiSelectedIds().has(row.id)
      ? [...multiSelectedIds()]
      : [row.id];
    event.dataTransfer?.setData(
      "application/x-quill-message-ids",
      JSON.stringify(ids),
    );
    if (event.dataTransfer) event.dataTransfer.effectAllowed = "copyMove";
  };

  // P1.3 saved searches — the header's "Save search" inline form.
  const [saving, setSaving] = createSignal(false);
  const [saveName, setSaveName] = createSignal("");
  const doSaveSearch = async () => {
    const name = saveName().trim();
    if (!name) return;
    await saveCurrentSearch(name);
    setSaving(false);
    setSaveName("");
    await refreshSavedSearches();
  };

  // Scrolling to the top of the list is the manual refresh gesture: trigger a
  // sync (throttled to once per 20s so a long scroll doesn't fire a storm).
  // P1.5: persist the scroll position per filter across restarts.
  let lastManualSyncAt = 0;
  const handleScroll = (e: Event) => {
    const el = e.currentTarget as HTMLElement;
    saveScroll(filterKey(), el.scrollTop);
    if (el.scrollTop > 0) return;
    const now = Date.now();
    if (now - lastManualSyncAt < 20_000) return;
    lastManualSyncAt = now;
    void syncNow();
  };

  const title = () => {
    if (isSearching()) {
      return `Search (${searchResults().length})`;
    }
    const current = filter();
    if (current.kind === "folder") {
      return folders().find((f) => f.id === current.folderId)?.name ?? "Inbox";
    }
    return (
      accounts().find((a) => a.id === current.accountId)?.address ?? "Account"
    );
  };

  const scopeLabel = () => {
    const current = filter();
    if (current.kind === "folder") {
      const name =
        folders().find((f) => f.id === current.folderId)?.name ?? "Inbox";
      return `in ${name}`;
    }
    const addr =
      accounts().find((a) => a.id === current.accountId)?.address ?? "Account";
    return `in ${addr}`;
  };

  return (
    <section
      class="message-list"
      aria-label="Message list"
      style={{ width: `${effectiveListWidth(theme)}px` }}
    >
      <header class="list-header">
        <div class="list-header__top">
          <h1 class="list-title">{title()}</h1>
          <Show when={theme() === "hairline" && !isSearching()}>
            <span class="list-accounts">{accounts().length} accounts</span>
          </Show>
          <Show when={isSearching()}>
            <span class="list-scope">{scopeLabel()}</span>
          </Show>
        </div>
        <div class="list-search-wrap">
          <input
            ref={setSearchEl}
            class="list-search"
            type="text"
            value={searchQuery()}
            onInput={(e) => updateSearch(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                clearSearch();
                e.currentTarget.blur();
              }
            }}
            placeholder="Search mail and events… (/)"
          />
          <SearchHelp />
          <button
            type="button"
            class="list-search-save-btn"
            onClick={openShortcuts}
            aria-label="Keyboard shortcuts"
          >
            ⌘?
          </button>
          <Show when={isSearching()}>
            <Show
              when={!saving()}
              fallback={
                <span class="list-search-save">
                  <input
                    type="text"
                    value={saveName()}
                    onInput={(e) => setSaveName(e.currentTarget.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void doSaveSearch();
                    }}
                    placeholder="Name this search"
                    aria-label="Saved search name"
                  />
                  <button
                    type="button"
                    class="list-search-save__btn"
                    onClick={() => void doSaveSearch()}
                    disabled={!saveName().trim()}
                  >
                    Save
                  </button>
                  <button
                    type="button"
                    class="list-search-save__btn"
                    onClick={() => setSaving(false)}
                  >
                    Cancel
                  </button>
                </span>
              }
            >
              <button
                type="button"
                class="list-search-save-btn"
                onClick={() => setSaving(true)}
              >
                Save search
              </button>
            </Show>
            <button
              type="button"
              class="list-search__clear"
              onClick={() => clearSearch()}
              aria-label="Clear search"
            >
              ×
            </button>
          </Show>
        </div>
      </header>

      {/* P1.1 bulk triage bar — only while >1 messages are selected. */}
      <Show when={multiSelectedIds().size > 1 && !isSearching()}>
        <BulkActionBar folders={folders()} />
      </Show>

      {/* The listbox (9.3): keyboard-focusable, active-descendant points at
          the selected option. `assignScrollRef` feeds both the virtualizer's
          scroll element and the keymap's focus check. */}
      <div
        ref={assignScrollRef}
        class="message-list__scroll"
        role="listbox"
        tabindex="0"
        aria-label="Message list"
        aria-activedescendant={
          selectedId() != null ? `mail-opt-${selectedId()}` : undefined
        }
        onScroll={handleScroll}
      >
        <Show
          when={!isSearching()}
          fallback={
            <div class="search-results">
              <For each={searchResults()}>
                {(match) => (
                  <button
                    type="button"
                    id={`mail-opt-${match.id}`}
                    class="search-match"
                    classList={{
                      "is-selected":
                        selectedId() === match.id && match.kind === "message",
                    }}
                    onClick={() => {
                      if (match.kind === "message") {
                        handleSelect(match.id);
                      }
                    }}
                  >
                    <div class="search-match__top">
                      <span class="search-match__title">{match.title}</span>
                      <span class="search-match__kind" data-kind={match.kind}>
                        {match.kind}
                      </span>
                      <span class="search-match__time mono tabular">
                        {formatRelativeTime(match.timestamp_ms)}
                      </span>
                    </div>
                    <div class="search-match__subtitle">{match.subtitle}</div>
                    <HighlightedSnippet text={match.snippet} />
                  </button>
                )}
              </For>
              <Show when={searchResults().length === 0}>
                <div class="list-empty">
                  No results found for "{searchQuery()}" {scopeLabel()}
                </div>
              </Show>
            </div>
          }
        >
          <div
            style={{
              height: `${virtualizer.getTotalSize()}px`,
              position: "relative",
            }}
          >
            <For each={virtualizer.getVirtualItems()}>
              {(item) => (
                <div
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    transform: `translateY(${item.start}px)`,
                  }}
                >
                  <MessageRow
                    row={rows[item.index]}
                    account={accounts().find(
                      (a) => a.id === rows[item.index].account_id,
                    )}
                    selected={selectedId() === rows[item.index].id}
                    multiSelected={multiSelectedIds().has(rows[item.index].id)}
                    onSelect={handleSelect}
                    onToggleSelect={handleToggleSelect}
                    onSelectRange={handleSelectRange}
                    onContextMenu={(e) =>
                      handleMessageContextMenu(rows[item.index], e)
                    }
                    onDragStart={(e) => handleRowDragStart(rows[item.index], e)}
                  />
                </div>
              )}
            </For>
          </div>
          <Show when={rows.length === 0}>
            <div class="list-empty">No messages</div>
          </Show>
        </Show>
      </div>
    </section>
  );
}
