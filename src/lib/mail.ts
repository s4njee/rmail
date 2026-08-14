import { createSignal } from "solid-js";
import { createStore } from "solid-js/store";
import type { Account } from "./ipc/Account";
import type { Folder } from "./ipc/Folder";
import type { MessageDetail } from "./ipc/MessageDetail";
import type { MessageQuery } from "./ipc/MessageQuery";
import type { MessageRow } from "./ipc/MessageRow";
import {
  getMessage,
  listAccounts,
  listFolders,
  markRead,
  pageMessages,
} from "./tauri";

// Mail navigation state (Epic 5–6): the folders and accounts the sidebar
// renders, the active filter, the loaded message rows, and the selection.

export type MailFilter =
  { kind: "folder"; folderId: number } | { kind: "account"; accountId: number };

// Default: the unified Inbox. The store returns Inbox as folder id 1.
const [folders, setFolders] = createSignal<Folder[]>([]);
const [accounts, setAccounts] = createSignal<Account[]>([]);
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
const [detail, setDetail] = createSignal<MessageDetail | null>(null);

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
 * message actions so counts stay live (Epic 5.2 / 6.4). */
export async function refreshMail(): Promise<void> {
  const [f, a] = await Promise.all([listFolders(), listAccounts()]);
  setFolders(f);
  setAccounts(a);
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

/** The full message for the selection, fetched only on selection (Epic 3.3). */
export function useDetail(): () => MessageDetail | null {
  return detail;
}

export async function loadDetail(id: number): Promise<void> {
  setDetail(await getMessage(id));
}

/** Build the store query for a filter. A folder filter maps id → folder name
 * ("Starred" is the derived flagged set); an account filter is that account
 * across folders. */
export function buildQuery(
  current: MailFilter,
  allFolders: Folder[],
): MessageQuery {
  if (current.kind === "folder") {
    const name =
      allFolders.find((f) => f.id === current.folderId)?.name ?? null;
    return { folder: name, account_id: null, offset: 0, limit: 500 };
  }
  return { folder: null, account_id: current.accountId, offset: 0, limit: 500 };
}

/** (Re)load the message rows for the current filter. */
export async function loadRows(): Promise<void> {
  const page = await pageMessages(buildQuery(filter(), folders()));
  setRows(page.items);
  setTotal(page.total);
}

let dwellTimer: ReturnType<typeof setTimeout> | undefined;

/** Select a message. Selection marks an unread message read after ~1s of
 * dwell (Epic 6.4) — leaving before then cancels, so arrow-key scanning
 * doesn't burn the unread list. */
export function selectMessage(id: number): void {
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
  setRows((current) =>
    current.map((r) => (r.id === id ? { ...r, unread: false } : r)),
  );
  await refreshMail();
}

/** Move the selection by `delta` rows in the current list (Epic 9.1). */
export function selectRelative(delta: number): void {
  if (rows.length === 0) return;
  const current = selectedId();
  const idx = current == null ? -1 : rows.findIndex((r) => r.id === current);
  const next = Math.min(Math.max(idx + delta, 0), rows.length - 1);
  const row = rows[next];
  if (row) selectMessage(row.id);
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
