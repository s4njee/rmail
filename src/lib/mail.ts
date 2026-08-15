import { createSignal } from "solid-js";
import { createStore } from "solid-js/store";
import type { Account } from "./ipc/Account";
import type { ActionType } from "./ipc/ActionType";
import type { BulkAction } from "./ipc/BulkAction";
import type { BulkActionResult } from "./ipc/BulkActionResult";
import type { Folder } from "./ipc/Folder";
import type { MessageDetail } from "./ipc/MessageDetail";
import type { MessageProgressUpdate } from "./ipc/MessageProgressUpdate";
import type { MessageQuery } from "./ipc/MessageQuery";
import type { MessageRow } from "./ipc/MessageRow";
import type { SavedSearch } from "./ipc/SavedSearch";
import type { SearchMatch } from "./ipc/SearchMatch";
import { syncDockBadge } from "./notifications";
import {
  applyThreadAction,
  bulkAction,
  getMessage,
  getSettings,
  listAccounts,
  listFolders,
  listSavedSearches,
  markJunk,
  markRead,
  pageMessages,
  restoreMessage,
  saveSearch,
  searchStore,
  setSnoozed,
} from "./tauri";

// Mail navigation state (Epic 5–6): the folders and accounts the sidebar
// renders, the active filter, the loaded message rows, and the selection.

export type MailFilter =
  { kind: "folder"; folderId: number } | { kind: "account"; accountId: number };

// Default: the unified Inbox. The store returns Inbox as folder id 1.
const [folders, setFolders] = createSignal<Folder[]>([]);
const [accounts, setAccounts] = createSignal<Account[]>([]);
// The account currently open in the shared edit dialog (opened from Settings
// or a right-click menu); rendered once at the App root.
const [editingAccount, setEditingAccount] = createSignal<Account | null>(null);

export function useEditingAccount(): () => Account | null {
  return editingAccount;
}

export function openAccountEdit(account: Account): void {
  setEditingAccount(account);
}

export function closeAccountEdit(): void {
  setEditingAccount(null);
}
const [filter, setFilter] = createSignal<MailFilter>({
  kind: "folder",
  folderId: 1,
});
// Rows are a store, not a signal: MessageRow reads `row.unread` as a
// fine-grained reactive field, so a mark-read update re-renders the row even
// when the list's <For> reconciliation doesn't touch it.
const [rows, setRows] = createStore<MessageRow[]>([]);
const [total, setTotal] = createSignal(0);
const [selectedId, setSelectedId] = createSignal<number | null>(null);
// P1.1 multi-select: the bulk set (empty = single selection) + the range
// anchor for shift-extending over `rows`.
const [multiSelectedIds, setMultiSelectedIds] = createSignal<Set<number>>(
  new Set(),
);
const [selectionAnchorId, setSelectionAnchorId] = createSignal<number | null>(
  null,
);
const [detail, setDetail] = createSignal<MessageDetail | null>(null);
// Reading-pane download state (Epic 7.2): true while the selected message's
// body is being fetched on demand, plus byte progress streamed from Rust.
const [detailLoading, setDetailLoading] = createSignal(false);
const [detailProgress, setDetailProgress] =
  createSignal<MessageProgressUpdate | null>(null);

// Search state (Epic 15)
const [searchQuery, setSearchQuery] = createSignal("");
const [searchResults, setSearchResults] = createSignal<SearchMatch[]>([]);
const [isSearching, setIsSearching] = createSignal(false);
const [savedSearches, setSavedSearches] = createSignal<SavedSearch[]>([]);

export function useFolders(): () => Folder[] {
  return folders;
}

export function useAccounts(): () => Account[] {
  return accounts;
}

export function useFilter(): () => MailFilter {
  return filter;
}

/** (Re)load folders and accounts from the store. Called at startup and after
 * message actions so counts stay live (Epic 5.2 / 6.4 & Roadmap 3.4). */
export async function refreshMail(): Promise<void> {
  const [f, a, s] = await Promise.all([
    listFolders(),
    listAccounts(),
    getSettings(),
  ]);
  setFolders(f);
  setAccounts(a);

  // Sync dock badge based on unread inbox messages
  const inboxUnread = f
    .filter((folder) => folder.kind === "inbox")
    .reduce((sum, folder) => sum + folder.unread_count, 0);
  syncDockBadge(inboxUnread, s.notifications);
}

export function selectFolder(folderId: number): void {
  setFilter({ kind: "folder", folderId });
}

export function selectAccount(accountId: number): void {
  setFilter({ kind: "account", accountId });
}

/** Rows loaded for the current filter (Epic 6) — a reactive store array. */
export function useRows(): MessageRow[] {
  return rows;
}

export function useTotal(): () => number {
  return total;
}

export function useSelectedId(): () => number | null {
  return selectedId;
}

/** The multi-select set (P1.1). Empty when only a single message is selected. */
export function useMultiSelectedIds(): () => Set<number> {
  return multiSelectedIds;
}

export function useSelectionAnchor(): () => number | null {
  return selectionAnchorId;
}

/** The full message for the selection, fetched only on selection (Epic 3.3). */
export function useDetail(): () => MessageDetail | null {
  return detail;
}

/** True while the selected message's body is still being fetched. */
export function useDetailLoading(): () => boolean {
  return detailLoading;
}

/** Byte-level download progress for the selected message (Epic 7.2). */
export function useDetailProgress(): () => MessageProgressUpdate | null {
  return detailProgress;
}

// The signal setter, re-exported so store-events can feed the Rust progress
// events into the reading-pane state.
export { setDetailProgress };

export async function loadDetail(id: number): Promise<void> {
  try {
    const msg = await getMessage(id);
    // Guard: discard if the user navigated away during the fetch
    if (selectedId() === id) setDetail(msg);
  } finally {
    // The fetch resolved (or failed) for the current selection — clear the
    // loading state so the pane renders the result. When the selection moved
    // on mid-fetch, leave loading for the new selection to manage.
    if (selectedId() === id) {
      setDetailLoading(false);
      setDetailProgress(null);
    }
  }
}

/** Build the store query for a filter. A folder filter maps id → folder name
 * ("Starred" is the derived flagged set); an account filter is that account
 * across folders. */
export function buildQuery(
  current: MailFilter,
  allFolders: Folder[],
  threaded = true,
): MessageQuery {
  if (current.kind === "folder") {
    const name =
      allFolders.find((f) => f.id === current.folderId)?.name ?? null;
    return { folder: name, account_id: null, offset: 0, limit: 500, threaded };
  }
  return {
    folder: null,
    account_id: current.accountId,
    offset: 0,
    limit: 500,
    threaded,
  };
}

export function useSearchQuery(): () => string {
  return searchQuery;
}

export function useSearchResults(): () => SearchMatch[] {
  return searchResults;
}

export function useIsSearching(): () => boolean {
  return isSearching;
}

/** The saved searches (P1.3) — persistent virtual folders. */
export function useSavedSearches(): () => SavedSearch[] {
  return savedSearches;
}

export async function refreshSavedSearches(): Promise<void> {
  try {
    setSavedSearches(await listSavedSearches());
  } catch {
    /* non-fatal */
  }
}

/** Execute a search query against the current scope and publish the results. */
async function runSearch(q: string): Promise<void> {
  const current = filter();
  const currentFolder =
    current.kind === "folder"
      ? (folders().find((f) => f.id === current.folderId)?.name ?? null)
      : null;
  const currentAccountId =
    current.kind === "account" ? current.accountId : null;
  const matches = await searchStore({
    query: q,
    folder: currentFolder,
    account_id: currentAccountId,
    include_events: true,
    limit: 50,
  });
  // Guard: discard if the query changed during the search
  if (q === searchQuery().trim()) {
    setSearchResults(matches);
  }
}

let searchDebounceTimer: ReturnType<typeof setTimeout> | undefined;

export function updateSearch(queryText: string): void {
  setSearchQuery(queryText);
  const q = queryText.trim();
  if (!q) {
    if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
    setIsSearching(false);
    setSearchResults([]);
    return;
  }

  setIsSearching(true);
  if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
  searchDebounceTimer = setTimeout(() => void runSearch(q), 150);
}

/** Enter search mode with a specific query — used by saved searches (P1.3). */
export function runSearchQuery(query: string): void {
  setSearchQuery(query);
  const q = query.trim();
  if (!q) {
    clearSearch();
    return;
  }
  setIsSearching(true);
  void runSearch(q);
}

/** Save the current query under a name, then refresh the sidebar list. */
export async function saveCurrentSearch(name: string): Promise<void> {
  const q = searchQuery().trim();
  if (!q) return;
  await saveSearch(name, q);
  await refreshSavedSearches();
}

export function clearSearch(): void {
  if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
  setSearchQuery("");
  setSearchResults([]);
  setIsSearching(false);
}

/** (Re)load the message rows for the current filter. */
export async function loadRows(): Promise<void> {
  const currentFilter = filter();
  const settings = await getSettings();
  const page = await pageMessages(
    buildQuery(
      currentFilter,
      folders(),
      settings.conversationThreading ?? true,
    ),
  );
  // Guard: discard if the user switched filters during the fetch
  if (filter() === currentFilter) {
    setRows(page.items);
    setTotal(page.total);
  }
}

/** Apply an action across all messages in a conversation thread. */
export async function performThreadAction(
  accountId: number,
  threadId: string,
  action: ActionType,
): Promise<void> {
  await applyThreadAction(accountId, threadId, action);
  await loadRows();
  await refreshMail();
}

/** Mark a message as Junk (Spam) or restore to Inbox (Roadmap 3.7). */
export async function markMessageJunk(
  id: number,
  junk: boolean,
): Promise<void> {
  await markJunk(id, junk);
  await loadRows();
  await refreshMail();
}

let dwellTimer: ReturnType<typeof setTimeout> | undefined;

/** Select a message. Selection marks an unread message read after ~1s of
 * dwell (Epic 6.4) — leaving before then cancels, so arrow-key scanning
 * doesn't burn the unread list. A plain select clears the multi-selection. */
export function selectMessage(id: number): void {
  clearMultiSelect();
  if (id !== selectedId()) {
    // The reading pane drops the stale message and shows its loading screen
    // until the new body lands (Epic 7.2). Cached reads resolve in a couple of
    // milliseconds, so this is never visible for them — only for real downloads.
    setDetailLoading(true);
    setDetailProgress(null);
  }
  setSelectedId(id);
  if (dwellTimer !== undefined) clearTimeout(dwellTimer);
  const row = rows.find((r) => r.id === id);
  if (row && row.unread) {
    dwellTimer = setTimeout(() => void markMessageRead(id), 1000);
  }
}

/** Apply the read state locally (so the 120ms transition plays) and persist
 * it to the store; refresh folder counts. */
export async function markMessageRead(id: number): Promise<void> {
  await markRead(id, false);
  // Use fine-grained store mutation to avoid re-rendering the entire list
  setRows((r) => r.id === id, "unread", false);
  await refreshMail();
}

// -- P1.1 multi-select + bulk triage -------------------------------------

function rowIndex(id: number): number {
  return rows.findIndex((r) => r.id === id);
}

/** Ctrl/Cmd+click (or a selection checkbox): toggle a message in the bulk set. */
export function toggleMultiSelect(id: number): void {
  const next = new Set(multiSelectedIds());
  if (next.has(id)) next.delete(id);
  else next.add(id);
  setMultiSelectedIds(next);
  setSelectionAnchorId(id);
  setSelectedId(id);
}

export function clearMultiSelect(): void {
  if (multiSelectedIds().size > 0) setMultiSelectedIds(new Set<number>());
}

/** Shift+click: select the contiguous range from the anchor to `id`. */
export function selectRangeTo(id: number): void {
  const anchor = selectionAnchorId() ?? selectedId();
  const aIdx = anchor != null ? rowIndex(anchor) : -1;
  const bIdx = rowIndex(id);
  if (aIdx < 0 || bIdx < 0) {
    setMultiSelectedIds(new Set([id]));
    setSelectedId(id);
    return;
  }
  const lo = Math.min(aIdx, bIdx);
  const hi = Math.max(aIdx, bIdx);
  const next = new Set<number>();
  for (let i = lo; i <= hi; i++) next.add(rows[i].id);
  setMultiSelectedIds(next);
  setSelectedId(id);
}

/** Shift+arrow/j/k: extend the multi-selection by `delta` rows. */
export function selectRelativeExtended(delta: number): void {
  if (isSearching() || rows.length === 0) return;
  const anchor = selectionAnchorId() ?? selectedId();
  const aIdx = anchor != null ? rowIndex(anchor) : -1;
  const current = selectedId();
  const cIdx = current != null ? rowIndex(current) : -1;
  const target = Math.min(Math.max(cIdx + delta, 0), rows.length - 1);
  const row = rows[target];
  if (!row) return;
  if (aIdx >= 0) {
    const lo = Math.min(aIdx, target);
    const hi = Math.max(aIdx, target);
    const next = new Set<number>();
    for (let i = lo; i <= hi; i++) next.add(rows[i].id);
    setMultiSelectedIds(next);
  } else {
    setSelectionAnchorId(current);
    setMultiSelectedIds(new Set([current ?? row.id, row.id]));
  }
  setSelectedId(row.id);
}

/** Move the selection by `delta` rows in the current list (Epic 9.1). When
 * `extend` is set (Shift held), it extends the multi-selection instead. */
export function selectRelative(delta: number, extend = false): void {
  // When searching, navigate the search results instead of folder rows
  if (isSearching()) {
    if (extend) return;
    const results = searchResults();
    if (results.length === 0) return;
    const current = selectedId();
    const idx =
      current == null ? -1 : results.findIndex((r) => r.id === current);
    const next = Math.min(Math.max(idx + delta, 0), results.length - 1);
    const match = results[next];
    if (match && match.kind === "message") selectMessage(match.id);
    return;
  }
  if (rows.length === 0) return;
  if (extend) {
    selectRelativeExtended(delta);
    return;
  }
  clearMultiSelect();
  const current = selectedId();
  const idx = current == null ? -1 : rowIndex(current);
  const next = Math.min(Math.max(idx + delta, 0), rows.length - 1);
  const row = rows[next];
  if (row) selectMessage(row.id);
}

/** The message ids a triage action applies to: the multi-selection when >1,
 * otherwise the focused row. */
export function triageIds(): number[] {
  const multi = [...multiSelectedIds()];
  if (multi.length > 1) return multi;
  return selectedId() != null ? [selectedId()!] : [];
}

/** Run a bulk action across (possibly mixed-account) ids, then reload and
 * move the selection to the next visible row after the affected range
 * ("consistent next-message selection after triage", P1.1). Returns the
 * aggregated result for partial-failure reporting. `recordUndo` is false when
 * this is itself the undo pass, so undo never re-records itself. */
export async function triage(
  ids: number[],
  action: BulkAction,
  destination?: string,
  recordUndo = true,
): Promise<BulkActionResult> {
  if (ids.length === 0) return { ok: 0, failed: 0, errors: [] };
  if (recordUndo) recordTriageUndo(ids, action, destination);
  // The command is per-account; group mixed-account selections.
  const byAccount = new Map<number, number[]>();
  for (const id of ids) {
    const row = rows.find((r) => r.id === id);
    if (row) {
      const list = byAccount.get(row.account_id) ?? [];
      list.push(id);
      byAccount.set(row.account_id, list);
    }
  }
  const aggregated: BulkActionResult = { ok: 0, failed: 0, errors: [] };
  for (const [accountId, accountIds] of byAccount) {
    const result = await bulkAction(accountId, accountIds, action, destination);
    aggregated.ok += result.ok;
    aggregated.failed += result.failed;
    aggregated.errors.push(...result.errors);
  }
  const affectedIdx = ids
    .map((id) => rowIndex(id))
    .filter((i) => i >= 0)
    .sort((a, b) => a - b);
  await refreshMail();
  await loadRows();
  clearMultiSelect();
  if (affectedIdx.length > 0) {
    const next = rows[Math.min(Math.max(affectedIdx[0], 0), rows.length - 1)];
    if (next) setSelectedId(next.id);
  }
  return aggregated;
}

/** Move messages to a local folder (P1.1 — drag-drop and context menu). */
export async function moveMessages(
  ids: number[],
  folder: string,
): Promise<BulkActionResult> {
  return triage(ids, "move", folder);
}

/** Snooze messages until `untilMs` (local-only) and move the selection to the
 * next visible row, mirroring `triage`'s after-removal selection (P1.1). */
export async function snooze(ids: number[], untilMs: number): Promise<void> {
  if (ids.length === 0) return;
  const focused = selectedId();
  const focusedIdx = focused != null ? rowIndex(focused) : 0;
  await setSnoozed(ids, untilMs);
  if (untilMs > 0) recordTriageUndo(ids, "snooze", undefined);
  await refreshMail();
  await loadRows();
  clearMultiSelect();
  const next = rows[Math.min(Math.max(focusedIdx, 0), rows.length - 1)];
  if (next) setSelectedId(next.id);
}

// -- P1.1 consistent undo ------------------------------------------------

type MessageSnapshot = { folder: string; unread: boolean; flagged: boolean };

export type TriageUndoAction = BulkAction | "snooze";

export type TriageUndo = {
  label: string;
  ids: number[];
  before: Map<number, MessageSnapshot>;
  action: TriageUndoAction;
  ts: number;
};

const [triageUndo, setTriageUndo] = createSignal<TriageUndo | null>(null);

export function useTriageUndo(): () => TriageUndo | null {
  return triageUndo;
}

export function clearTriageUndo(): void {
  setTriageUndo(null);
}

function undoLabel(action: TriageUndoAction, count: number): string {
  const n = count > 1 ? `${count} messages` : "message";
  switch (action) {
    case "archive":
      return `Archived ${n}`;
    case "delete":
      return `Deleted ${n}`;
    case "markJunk":
    case "markNotJunk":
      return `Junked ${n}`;
    case "move":
      return `Moved ${n}`;
    case "markRead":
      return `Marked ${n} read`;
    case "markUnread":
      return `Marked ${n} unread`;
    case "star":
      return `Starred ${n}`;
    case "unstar":
      return `Unstarred ${n}`;
    case "snooze":
      return `Snoozed ${n}`;
    default:
      return "Changed";
  }
}

/** Capture the pre-action state so the TriageUndoBar can reverse it. Read and
 * star toggles are excluded — they're trivially re-toggled and would pop the
 * bar on every keypress. */
function recordTriageUndo(
  ids: number[],
  action: TriageUndoAction,
  _destination?: string,
): void {
  if (ids.length === 0) return;
  if (action === "markRead" || action === "markUnread") return;
  if (action === "star" || action === "unstar") return;
  const before = new Map<number, MessageSnapshot>();
  for (const id of ids) {
    const row = rows.find((r) => r.id === id);
    if (row) {
      before.set(id, {
        folder: row.folder,
        unread: row.unread,
        flagged: row.flagged,
      });
    }
  }
  setTriageUndo({
    label: undoLabel(action, ids.length),
    ids,
    before,
    action,
    ts: Date.now(),
  });
}

/** Reverse the last triage action: move/read/star/junk back to their prior
 * state, or restore a soft-deleted message and cancel its queued server
 * Delete. A delete that already replayed to the server can't be fully undone —
 * the UI notes this. */
export async function undoTriage(): Promise<void> {
  const u = triageUndo();
  if (!u) return;
  setTriageUndo(null);

  if (u.action === "delete") {
    for (const id of u.ids) {
      try {
        await restoreMessage(id);
      } catch {
        /* already replayed to the server — best effort */
      }
    }
    await refreshMail();
    await loadRows();
    return;
  }

  if (u.action === "snooze") {
    // 0 = a past time, which the hidden filter treats as not snoozed.
    await setSnoozed(u.ids, 0);
    await refreshMail();
    await loadRows();
    return;
  }

  switch (u.action) {
    case "markRead":
      await triage(u.ids, "markUnread", undefined, false);
      break;
    case "markUnread":
      await triage(u.ids, "markRead", undefined, false);
      break;
    case "star":
      await triage(u.ids, "unstar", undefined, false);
      break;
    case "unstar":
      await triage(u.ids, "star", undefined, false);
      break;
    case "archive":
    case "move":
    case "markJunk":
    case "markNotJunk": {
      // Move each message back to the folder it was in before the action.
      const byFolder = new Map<string, number[]>();
      for (const id of u.ids) {
        const folder = u.before.get(id)?.folder;
        if (folder) {
          const list = byFolder.get(folder) ?? [];
          list.push(id);
          byFolder.set(folder, list);
        }
      }
      for (const [folder, folderIds] of byFolder) {
        await triage(folderIds, "move", folder, false);
      }
      break;
    }
    default:
      break;
  }
}

// Element refs the keymap needs: the search input (for `/`) and the listbox
// container (to gate arrow-key selection on list focus).
let searchEl: HTMLInputElement | null = null;
let listEl: HTMLElement | null = null;

export function setSearchEl(el: HTMLInputElement | null): void {
  searchEl = el;
}

export function setListEl(el: HTMLElement | null): void {
  listEl = el;
}

export function focusSearch(): void {
  searchEl?.focus();
}

/** True when the message list (or a row in it) has keyboard focus. */
export function messageListHasFocus(): boolean {
  const active = document.activeElement;
  return !!active && !!listEl && (active === listEl || listEl.contains(active));
}
