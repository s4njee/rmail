import { createVirtualizer } from "@tanstack/solid-virtual";
import { createEffect, createSignal, For, Show } from "solid-js";
import {
  loadRows,
  selectMessage,
  setListEl,
  setSearchEl,
  useAccounts,
  useFilter,
  useFolders,
  useRows,
  useSelectedId,
} from "../lib/mail";
import { effectiveListWidth } from "../lib/panes";
import { useTheme } from "../lib/theme";
import { MessageRow } from "./MessageRow";
import "./MessageList.css";

const ROW_H = 80; // keep in sync with --row-h in tokens.css
const BANDED_ROW_STEP = ROW_H + 2; // rows carry a 2px margin-bottom in Banded

// The message list (Epic 6): a header reflecting the active filter, a search
// field, and a virtualized list of 72px rows paged from the store (6.3).
// Scroll position is kept per filter, so switching away and back restores it.
export function MessageList() {
  const theme = useTheme();
  const folders = useFolders();
  const accounts = useAccounts();
  const filter = useFilter();
  const rows = useRows();
  const selectedId = useSelectedId();

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
      void loadRows().then(() => {
        // `el` was captured in the tracking scope above.
        if (el) el.scrollTop = scrollOffsets[key] ?? 0;
      });
    } else {
      // Folders refreshed (e.g. after a mark-read) — re-sync rows, keep scroll.
      void loadRows();
    }
  });

  const rowStep = () => (theme() === "banded" ? BANDED_ROW_STEP : ROW_H);
  const virtualizer = createVirtualizer({
    // `count` must be a getter, not a plain value: the solid-virtual adapter
    // only recomputes when a signal is read inside its createComputed scope,
    // and a plain `rows.length` is read once at creation (count stuck at 0 →
    // empty list). A getter keeps it tracked so rows-loading re-renders.
    get count() {
      return rows.length;
    },
    // The scroll element's rect observation can land before the pane's flex
    // height resolves (measured 0 → no visible range → empty list). A
    // non-zero initialRect lets the first range compute; the ResizeObserver
    // corrects it with the real size shortly after.
    initialRect: { width: 400, height: 600 },
    getScrollElement: () => scrollEl(),
    estimateSize: () => rowStep(),
    overscan: 5,
  });

  // Keep the selection in view with a ≥1-row margin as the keyboard moves it
  // (Epic 9.2) — via the virtualizer, never scrollIntoView jumps.
  createEffect(() => {
    const id = selectedId();
    if (id == null) return;
    const idx = rows.findIndex((r) => r.id === id);
    if (idx < 0) return;
    const items = virtualizer.getVirtualItems();
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
  // arrow keys keep working afterwards.
  const handleSelect = (id: number) => {
    scrollEl()?.focus();
    selectMessage(id);
  };

  const title = () => {
    const current = filter();
    if (current.kind === "folder") {
      return folders().find((f) => f.id === current.folderId)?.name ?? "Inbox";
    }
    return (
      accounts().find((a) => a.id === current.accountId)?.address ?? "Account"
    );
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
          <Show when={theme() === "hairline"}>
            <span class="list-accounts">{accounts().length} accounts</span>
          </Show>
        </div>
        {/* Search is visual until Epic 15 wires the FTS index. */}
        <input
          ref={setSearchEl}
          class="list-search"
          type="text"
          placeholder="Search mail"
        />
      </header>

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
                {/* `rows[item.index]` must be read inline, not captured in a
                    const: <For> evaluates its item function once, and Solid
                    props are lazy getters — so the store read stays reactive
                    and a mark-read re-renders the row. */}
                <MessageRow
                  row={rows[item.index]}
                  account={accounts().find(
                    (a) => a.id === rows[item.index].account_id,
                  )}
                  selected={selectedId() === rows[item.index].id}
                  onSelect={handleSelect}
                />
              </div>
            )}
          </For>
        </div>
        <Show when={rows.length === 0}>
          {/* Not-designed copy; a quiet empty state per Epic 15/design review. */}
          <div class="list-empty">No messages</div>
        </Show>
      </div>
    </section>
  );
}
