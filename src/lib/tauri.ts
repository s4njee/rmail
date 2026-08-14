import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Account } from "./ipc/Account";
import type { AppSettings } from "./ipc/AppSettings";
import type { CalendarEvent } from "./ipc/CalendarEvent";
import type { Folder } from "./ipc/Folder";
import type { MessageDetail } from "./ipc/MessageDetail";
import type { Draft } from "./ipc/Draft";
import type { MessagePage } from "./ipc/MessagePage";
import type { MessageQuery } from "./ipc/MessageQuery";
import type { NewAccount } from "./ipc/NewAccount";
import type { OutgoingMessage } from "./ipc/OutgoingMessage";
import type { StoreEvent } from "./ipc/StoreEvent";
import {
  MOCK_ACCOUNTS,
  MOCK_EVENTS,
  MOCK_FOLDERS,
  MOCK_SETTINGS,
  getMockMessageDetail,
  getMockMessagePage,
} from "./mock";

const isTauri = () =>
  typeof window !== "undefined" &&
  ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

// In-memory mock state for web browser preview
let mockSettings = { ...MOCK_SETTINGS };
let mockEvents = [...MOCK_EVENTS];
const mockAccounts = [...MOCK_ACCOUNTS];

// Settings
export const getSettings = async (): Promise<AppSettings> => {
  if (isTauri()) return invoke<AppSettings>("get_settings");
  return mockSettings;
};

export const setSettings = async (settings: AppSettings): Promise<void> => {
  if (isTauri()) return invoke<void>("set_settings", { settings });
  mockSettings = { ...settings };
};

/** Read-modify-write: patch a few settings without clobbering the rest. */
export async function patchSettings(
  patch: Partial<AppSettings>,
): Promise<void> {
  const current = await getSettings();
  await setSettings({ ...current, ...patch });
}

// Footprint (on-disk cache, Epic 4.3 / 11.2)
export const getFootprint = async (): Promise<number> => {
  if (isTauri()) return invoke<number>("footprint");
  return 412 * 1024 * 1024;
};

// Folders & accounts
export const listFolders = async (): Promise<Folder[]> => {
  if (isTauri()) return invoke<Folder[]>("list_folders");
  return MOCK_FOLDERS;
};

export const listAccounts = async (): Promise<Account[]> => {
  if (isTauri()) return invoke<Account[]>("list_accounts");
  return mockAccounts;
};

// Account add/remove
export const addAccount = async (
  info: NewAccount,
  _password: string,
): Promise<Account> => {
  if (isTauri()) return invoke<Account>("add_account", { info, password: _password });
  const newAcc: Account = {
    id: mockAccounts.length + 1,
    address: info.address,
    protocol: info.protocol,
    sync_mode: "every 2 min",
    color: "#3b5bdb",
    local_bytes: 0,
    connected: true,
    server: info.server,
    port: info.port,
    tls: info.tls,
    folder_count: 1,
  };
  mockAccounts.push(newAcc);
  return newAcc;
};

export const removeAccount = async (id: number): Promise<void> => {
  if (isTauri()) return invoke<void>("remove_account", { id });
  const idx = mockAccounts.findIndex((a) => a.id === id);
  if (idx !== -1) mockAccounts.splice(idx, 1);
};

export const testConnection = async (
  server: string,
  port: number,
): Promise<void> => {
  if (isTauri()) return invoke<void>("test_connection", { server, port });
};

// Drafts
export const saveDraft = async (draft: Draft): Promise<number> => {
  if (isTauri()) return invoke<number>("save_draft", { draft });
  return draft.id || Date.now();
};

// Messages
export const pageMessages = async (query: MessageQuery): Promise<MessagePage> => {
  if (isTauri()) return invoke<MessagePage>("page_messages", { query });
  return getMockMessagePage(query);
};

export const getMessage = async (id: number): Promise<MessageDetail | null> => {
  if (isTauri()) return invoke<MessageDetail | null>("get_message", { id });
  return getMockMessageDetail(id);
};

export const markRead = async (id: number, unread: boolean): Promise<void> => {
  if (isTauri()) return invoke<void>("mark_read", { id, unread });
};

export const star = async (id: number, flagged: boolean): Promise<void> => {
  if (isTauri()) return invoke<void>("star", { id, flagged });
};

export const archive = async (id: number): Promise<void> => {
  if (isTauri()) return invoke<void>("archive", { id });
};

export const deleteMessage = async (id: number): Promise<void> => {
  if (isTauri()) return invoke<void>("delete", { id });
};

export const sendMessage = async (outgoing: OutgoingMessage): Promise<void> => {
  if (isTauri()) return invoke<void>("send", { outgoing });
};

export const attachmentPath = async (id: number): Promise<string | null> => {
  if (isTauri()) return invoke<string | null>("attachment_path", { id });
  return null;
};

// Calendar events
export const listEvents = async (
  startMs: number,
  endMs: number,
): Promise<CalendarEvent[]> => {
  if (isTauri()) return invoke<CalendarEvent[]>("list_events", { startMs, endMs });
  return mockEvents.filter((e) => e.start_ms < endMs && e.end_ms > startMs);
};

export const createEvent = async (
  event: CalendarEvent,
): Promise<CalendarEvent> => {
  if (isTauri()) return invoke<CalendarEvent>("create_event", { event });
  const created = { ...event, id: Date.now() };
  mockEvents.push(created);
  return created;
};

export const updateEvent = async (event: CalendarEvent): Promise<void> => {
  if (isTauri()) return invoke<void>("update_event", { event });
  mockEvents = mockEvents.map((e) => (e.id === event.id ? event : e));
};

export const deleteEvent = async (id: number): Promise<void> => {
  if (isTauri()) return invoke<void>("delete_event", { id });
  mockEvents = mockEvents.filter((e) => e.id !== id);
};

// Push events
export function onStoreEvent(
  handler: (event: StoreEvent) => void,
): Promise<() => void> {
  if (isTauri()) {
    return listen<StoreEvent>("store", (e) => handler(e.payload));
  }
  return Promise.resolve(() => {});
}

