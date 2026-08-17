import type { Account } from "./ipc/Account";
import type { Folder } from "./ipc/Folder";

/** Unified specials (Inbox, Starred, …) — the existing sidebar section. */
export function unifiedFolders(all: Folder[]): Folder[] {
  return all.filter((f) => f.account_id == null);
}

/** Persisted per-account mailboxes. */
export function mailboxFolders(all: Folder[]): Folder[] {
  return all.filter((f) => f.account_id != null);
}

export function isMailbox(folder: Folder): boolean {
  return folder.selectable && folder.kind !== "starred" && folder.kind !== "snoozed";
}

export function childrenOf(all: Folder[], parentId: number | null, accountId: number): Folder[] {
  return all
    .filter(
      (f) =>
        f.account_id === accountId &&
        (parentId == null ? f.parent_id == null : f.parent_id === parentId),
    )
    .sort((a, b) => {
      const kindRank = (k: Folder["kind"]) => {
        const order = [
          "inbox",
          "drafts",
          "sent",
          "archive",
          "junk",
          "trash",
          "custom",
        ];
        const i = order.indexOf(k);
        return i === -1 ? 99 : i;
      };
      const d = kindRank(a.kind) - kindRank(b.kind);
      if (d !== 0) return d;
      if (a.sort_order !== b.sort_order) return a.sort_order - b.sort_order;
      return a.name.localeCompare(b.name);
    });
}

export function folderLabel(folder: Folder, accounts: Account[]): string {
  if (folder.account_id == null) return folder.name;
  const addr = accounts.find((a) => a.id === folder.account_id)?.address;
  const path = folder.path || folder.name;
  return addr ? `${addr}: ${path}` : path;
}

export function favourites(all: Folder[]): Folder[] {
  return mailboxFolders(all)
    .filter((f) => f.favourite)
    .sort((a, b) => a.name.localeCompare(b.name));
}

export function recentFolders(all: Folder[], limit = 5): Folder[] {
  return mailboxFolders(all)
    .filter((f) => f.last_opened_at_ms != null && !f.favourite)
    .sort((a, b) => (b.last_opened_at_ms ?? 0) - (a.last_opened_at_ms ?? 0))
    .slice(0, limit);
}

export function matchFolders(all: Folder[], q: string): Folder[] {
  const needle = q.trim().toLowerCase();
  if (!needle) return [];
  return all.filter((f) => {
    const hay = `${f.name} ${f.path} ${f.server_name ?? ""}`.toLowerCase();
    return hay.includes(needle);
  });
}

/** Count shown on a row: unread (rolled up when collapsed), else total. */
export function folderCount(
  folder: Folder,
  rolledUp: boolean,
): { text: string; unread: boolean } | null {
  const unread = rolledUp ? folder.unread_count_tree : folder.unread_count;
  const total = rolledUp ? folder.total_count_tree : folder.total_count;
  if (unread > 0) return { text: String(unread), unread: true };
  if (total > 0) return { text: String(total), unread: false };
  return null;
}

const ACCOUNT_EXPANDED_KEY = "quill_account_expanded";

export function loadAccountExpanded(): Record<number, boolean> {
  try {
    const raw = localStorage.getItem(ACCOUNT_EXPANDED_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, boolean>;
    const out: Record<number, boolean> = {};
    for (const [k, v] of Object.entries(parsed)) {
      const id = Number(k);
      if (!Number.isNaN(id)) out[id] = Boolean(v);
    }
    return out;
  } catch {
    return {};
  }
}

export function saveAccountExpanded(state: Record<number, boolean>): void {
  try {
    localStorage.setItem(ACCOUNT_EXPANDED_KEY, JSON.stringify(state));
  } catch {
    /* ignore */
  }
}
