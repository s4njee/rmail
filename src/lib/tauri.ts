import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Account } from "./ipc/Account";
import type { AccountEdit } from "./ipc/AccountEdit";
import type { AccountRemovalInfo } from "./ipc/AccountRemovalInfo";
import type { ActionType } from "./ipc/ActionType";
import type { AppSettings } from "./ipc/AppSettings";
import type { BulkAction } from "./ipc/BulkAction";
import type { BulkActionResult } from "./ipc/BulkActionResult";
import type { CalendarCollection } from "./ipc/CalendarCollection";
import type { CalendarEvent } from "./ipc/CalendarEvent";
import type { CalendarSource } from "./ipc/CalendarSource";
import type { CalendarSubscription } from "./ipc/CalendarSubscription";
import type { CalendarTask } from "./ipc/CalendarTask";
import type { ContactGroup } from "./ipc/ContactGroup";
import type { ContactSuggestion } from "./ipc/ContactSuggestion";
import type { ConnectionTestReport } from "./ipc/ConnectionTestReport";
import type { DiscoveredSettings } from "./ipc/DiscoveredSettings";
import type { FreeBusySlot } from "./ipc/FreeBusySlot";
import type { Folder } from "./ipc/Folder";
import type { MailRule } from "./ipc/MailRule";
import type { MessageDetail } from "./ipc/MessageDetail";
import type { Draft } from "./ipc/Draft";
import type { DiagnosticsInfo } from "./ipc/DiagnosticsInfo";
import type { MessagePage } from "./ipc/MessagePage";
import type { MessageQuery } from "./ipc/MessageQuery";
import type { NewAccount } from "./ipc/NewAccount";
import type { OAuthInitPayload } from "./ipc/OAuthInitPayload";
import type { OAuthWaitResult } from "./ipc/OAuthWaitResult";
import type { OutgoingMessage } from "./ipc/OutgoingMessage";
import type { ProviderPreset } from "./ipc/ProviderPreset";
import type { RulePreview } from "./ipc/RulePreview";
import type { RulePreviewResult } from "./ipc/RulePreviewResult";
import type { SavedSearch } from "./ipc/SavedSearch";
import type { SearchIndexUpdate } from "./ipc/SearchIndexUpdate";
import type { SearchMatch } from "./ipc/SearchMatch";
import type { SearchQuery } from "./ipc/SearchQuery";
import type { ScheduledMessage } from "./ipc/ScheduledMessage";
import type { ServerFolder } from "./ipc/ServerFolder";
import type { StoreEvent } from "./ipc/StoreEvent";
import type { SyncedFolder } from "./ipc/SyncedFolder";
import type { TestConnectionSettings } from "./ipc/TestConnectionSettings";
import {
  MOCK_ACCOUNTS,
  MOCK_EVENTS,
  MOCK_FOLDERS,
  MOCK_SETTINGS,
  getMockMessageDetail,
  getMockMessagePage,
  searchMock,
} from "./mock";

export const isTauri = () =>
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

// Diagnostics (Roadmap E2.3)
export type JsErrorInfo = {
  message: string;
  stack: string | null;
  source: string | null;
  line: number | null;
  column: number | null;
};

export const reportJsError = async (payload: JsErrorInfo): Promise<string> => {
  if (isTauri()) return invoke<string>("report_js_error", { payload });
  return "mock";
};

export const setLogLevel = async (level: string): Promise<void> => {
  if (isTauri()) return invoke<void>("set_log_level", { level });
};

export const openLogsFolder = async (): Promise<void> => {
  if (isTauri()) return invoke<void>("open_logs_folder");
};

export const openCrashReportsFolder = async (): Promise<void> => {
  if (isTauri()) return invoke<void>("open_crash_reports_folder");
};

export const sendTestReport = async (): Promise<string> => {
  if (isTauri()) return invoke<string>("send_test_report");
  return "Test report (browser preview — nothing was written locally).";
};

export const flushPendingReports = async (): Promise<number> => {
  if (isTauri()) return invoke<number>("flush_pending_reports");
  return 0;
};

export const getDiagnosticsInfo = async (): Promise<DiagnosticsInfo> => {
  if (isTauri()) return invoke<DiagnosticsInfo>("get_diagnostics_info");
  return {
    appVersion: "0.1.0",
    os: "browser",
    arch: "web",
    channel: "dev",
    logLevel: "info",
    crashReportingEnabled: false,
    usagePingEnabled: false,
    pendingReportCount: 0,
    logFilePath: null,
    crashReportsDir: "(browser preview)",
    endpointConfigured: false,
  };
};

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
  if (isTauri())
    return invoke<Account>("add_account", { info, password: _password });
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
    last_error: null,
  };
  mockAccounts.push(newAcc);
  return newAcc;
};

export const removeAccount = async (id: number): Promise<void> => {
  if (isTauri()) return invoke<void>("remove_account", { id });
  const idx = mockAccounts.findIndex((a) => a.id === id);
  if (idx !== -1) mockAccounts.splice(idx, 1);
};

export const updateAccount = async (
  edit: AccountEdit,
  password: string,
): Promise<void> => {
  if (isTauri()) return invoke<void>("update_account", { edit, password });
  const idx = mockAccounts.findIndex((a) => a.id === edit.id);
  if (idx !== -1) {
    mockAccounts[idx] = {
      ...mockAccounts[idx],
      server: edit.server,
      port: edit.port,
      tls: edit.tls,
      sync_mode: edit.syncMode,
      color: edit.color,
    };
  }
};

// Account setup & recovery (P0.2)
export const listProviderPresets = async (): Promise<ProviderPreset[]> => {
  if (isTauri()) return invoke<ProviderPreset[]>("list_provider_presets");
  return [
    {
      id: "gmail",
      name: "Gmail",
      domains: ["gmail.com"],
      imap: { host: "imap.gmail.com", port: 993, tls: true },
      smtp: { host: "smtp.gmail.com", port: 465, tls: true },
      caldav: null,
      auth: "oauth",
      oauth_provider: "google",
      help: "Sign in with Google (mock).",
    },
  ];
};

export const discoverSettings = async (
  email: string,
): Promise<DiscoveredSettings> => {
  if (isTauri())
    return invoke<DiscoveredSettings>("discover_settings", { email });
  const preset = await listProviderPresets();
  return {
    imap: preset[0]?.imap ?? null,
    smtp: preset[0]?.smtp ?? null,
    caldav: preset[0]?.caldav ?? null,
    provider: preset[0] ?? null,
    steps: [{ source: "preset", status: "ok", detail: "Matched Gmail (mock)" }],
  };
};

export const testConnectionSettings = async (
  settings: TestConnectionSettings,
  password?: string,
): Promise<ConnectionTestReport> => {
  if (isTauri()) {
    return invoke<ConnectionTestReport>("test_connection_settings", {
      settings,
      password,
    });
  }
  return { ok: true, authed: true, issues: [], detail: "Connected (mock)" };
};

export const discoverMailFolders = async (
  email: string,
  server: string,
  port: number,
  tls: boolean,
  password?: string,
): Promise<ServerFolder[]> => {
  if (isTauri()) {
    return invoke<ServerFolder[]>("discover_mail_folders", {
      email,
      server,
      port,
      tls,
      password,
    });
  }
  return [
    { serverName: "INBOX", localName: "Inbox", kind: "inbox" },
    { serverName: "Sent", localName: "Sent", kind: "sent" },
    { serverName: "Drafts", localName: "Drafts", kind: "drafts" },
  ];
};

export const setSyncedFolders = async (
  accountId: number,
  folders: SyncedFolder[],
): Promise<void> => {
  if (isTauri()) {
    return invoke<void>("set_synced_folders", { accountId, folders });
  }
};

export const listSyncedFolders = async (
  accountId: number,
): Promise<SyncedFolder[]> => {
  if (isTauri())
    return invoke<SyncedFolder[]>("list_synced_folders", { accountId });
  return [];
};

export const accountRemovalInfo = async (
  accountId: number,
): Promise<AccountRemovalInfo> => {
  if (isTauri())
    return invoke<AccountRemovalInfo>("account_removal_info", { accountId });
  return { accountId, queuedActions: 0, drafts: 0, localBytes: 0 };
};

export const syncAccountNow = async (accountId: number): Promise<void> => {
  if (isTauri()) return invoke<void>("sync_account_now", { accountId });
};

export const waitOAuthCode = async (
  redirectUri: string,
  state: string,
): Promise<OAuthWaitResult> => {
  if (isTauri()) {
    return invoke<OAuthWaitResult>("wait_oauth_code", { redirectUri, state });
  }
  return {
    ok: false,
    code: null,
    error: "Browser preview has no OAuth listener.",
  };
};

export const reauthorizeAccount = async (
  accountId: number,
  provider: "google" | "microsoft365",
  code: string,
  codeVerifier: string,
  redirectUri: string,
  clientId?: string,
  clientSecret?: string,
): Promise<void> => {
  if (isTauri()) {
    return invoke<void>("reauthorize_account", {
      accountId,
      providerStr: provider,
      code,
      codeVerifier,
      redirectUri,
      clientId,
      clientSecret,
    });
  }
};

// Drafts
export const saveDraft = async (draft: Draft): Promise<number> => {
  if (isTauri()) return invoke<number>("save_draft", { draft });
  return draft.id || Date.now();
};

// Messages
export const pageMessages = async (
  query: MessageQuery,
): Promise<MessagePage> => {
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

export const markAnswered = async (
  id: number,
  answered: boolean,
): Promise<void> => {
  if (isTauri()) return invoke<void>("mark_answered", { id, answered });
};

export const markForwarded = async (
  id: number,
  forwarded: boolean,
): Promise<void> => {
  if (isTauri()) return invoke<void>("mark_forwarded", { id, forwarded });
};

export const archive = async (id: number): Promise<void> => {
  if (isTauri()) return invoke<void>("archive", { id });
};

export const deleteMessage = async (id: number): Promise<void> => {
  if (isTauri()) return invoke<void>("delete", { id });
};

// P1.1: bulk triage + undo-delete
export const bulkAction = async (
  accountId: number,
  ids: number[],
  action: BulkAction,
  destination?: string,
): Promise<BulkActionResult> => {
  if (isTauri()) {
    return invoke<BulkActionResult>("bulk_action", {
      accountId,
      ids,
      action,
      destination,
    });
  }
  return { ok: ids.length, failed: 0, errors: [] };
};

export const restoreMessage = async (id: number): Promise<void> => {
  if (isTauri()) return invoke<void>("restore_message", { id });
};

// P1.1 snooze
export const setSnoozed = async (
  ids: number[],
  untilMs: number,
): Promise<void> => {
  if (isTauri()) return invoke<void>("set_snoozed", { ids, untilMs });
};

// P1.1 send-later (durable Outbox)
export const scheduleSend = async (
  outgoing: OutgoingMessage,
  sendAtMs: number,
  draft: string,
): Promise<number> => {
  if (isTauri()) {
    return invoke<number>("schedule_send", { outgoing, sendAtMs, draft });
  }
  return Date.now();
};

export const listScheduled = async (): Promise<ScheduledMessage[]> => {
  if (isTauri()) return invoke<ScheduledMessage[]>("list_scheduled");
  return [];
};

export const cancelScheduled = async (id: number): Promise<void> => {
  if (isTauri()) return invoke<void>("cancel_scheduled", { id });
};

// P1.2 recipient suggestions + contact groups
export const suggestRecipients = async (
  query: string,
  limit?: number,
): Promise<ContactSuggestion[]> => {
  if (isTauri()) {
    return invoke<ContactSuggestion[]>("suggest_recipients", { query, limit });
  }
  return [
    {
      name: "Ada Lovelace",
      address: "ada@example.com",
      useCount: 3,
      lastUsedAtMs: Date.now(),
    },
    {
      name: "Grace Hopper",
      address: "grace@example.com",
      useCount: 2,
      lastUsedAtMs: Date.now() - 86400000,
    },
  ];
};

export const recentRecipients = async (
  limit?: number,
): Promise<ContactSuggestion[]> => {
  if (isTauri())
    return invoke<ContactSuggestion[]>("recent_recipients", { limit });
  return [
    {
      name: "Ada Lovelace",
      address: "ada@example.com",
      useCount: 3,
      lastUsedAtMs: Date.now(),
    },
  ];
};

export const hideRecipient = async (address: string): Promise<void> => {
  if (isTauri()) return invoke<void>("hide_recipient", { address });
};

export const listContactGroups = async (): Promise<ContactGroup[]> => {
  if (isTauri()) return invoke<ContactGroup[]>("list_contact_groups");
  return [];
};

export const createContactGroup = async (name: string): Promise<number> => {
  if (isTauri()) return invoke<number>("create_contact_group", { name });
  return Date.now();
};

export const deleteContactGroup = async (id: number): Promise<void> => {
  if (isTauri()) return invoke<void>("delete_contact_group", { id });
};

export const addContactToGroup = async (
  groupId: number,
  address: string,
): Promise<void> => {
  if (isTauri()) {
    return invoke<void>("add_contact_to_group", { groupId, address });
  }
};

export const removeContactFromGroup = async (
  groupId: number,
  address: string,
): Promise<void> => {
  if (isTauri()) {
    return invoke<void>("remove_contact_from_group", { groupId, address });
  }
};

export const contactGroupMembers = async (
  groupId: number,
): Promise<ContactSuggestion[]> => {
  if (isTauri())
    return invoke<ContactSuggestion[]>("contact_group_members", { groupId });
  return [];
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
  if (isTauri())
    return invoke<CalendarEvent[]>("list_events", { startMs, endMs });
  return mockEvents.filter((e) => e.start_ms < endMs && e.end_ms > startMs);
};

/** Distinct source calendars present in the store (Roadmap 4.4). */
export const listCalendars = async (): Promise<CalendarSource[]> => {
  if (isTauri()) return invoke<CalendarSource[]>("list_calendars");
  return [];
};

/** Delete a source calendar's local events and exclude it from future syncs. */
export const removeCalendarSource = async (
  accountId: number,
  source: string,
): Promise<void> => {
  if (isTauri()) {
    return invoke<void>("remove_calendar_source", { accountId, source });
  }
};

/** Undo a calendar removal so the next sync re-adds it. */
export const restoreCalendarSource = async (
  accountId: number,
  source: string,
): Promise<void> => {
  if (isTauri()) {
    return invoke<void>("restore_calendar_source", { accountId, source });
  }
};

/** Source calendars the user has removed (Settings "Removed" list). */
export const listRemovedCalendarSources = async (): Promise<
  CalendarSource[]
> => {
  if (isTauri())
    return invoke<CalendarSource[]>("list_removed_calendar_sources");
  return [];
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

// P1.4 calendar undo + duplicate
export const restoreEvent = async (event: CalendarEvent): Promise<void> => {
  if (isTauri()) return invoke<void>("restore_event", { event });
  mockEvents = mockEvents.some((e) => e.id === event.id)
    ? mockEvents.map((e) => (e.id === event.id ? event : e))
    : [...mockEvents, event];
};

export const duplicateEvent = async (id: number): Promise<CalendarEvent> => {
  if (isTauri()) return invoke<CalendarEvent>("duplicate_event", { id });
  const src = mockEvents.find((e) => e.id === id);
  if (!src) throw new Error("event not found");
  const clone = { ...src, id: Date.now(), title: `${src.title} (copy)` };
  mockEvents.push(clone);
  return clone;
};

// Full-text search (Epic 15)
export const searchStore = async (
  query: SearchQuery,
): Promise<SearchMatch[]> => {
  if (isTauri()) return invoke<SearchMatch[]>("search", { query });
  return searchMock(query.query, query.folder, query.account_id);
};

export const rebuildSearchIndex = async (): Promise<void> => {
  if (isTauri()) return invoke<void>("rebuild_search_index");
};

// P1.3: cancellable rebuild + progress + freshness
export const rebuildSearchIndexProgress = async (): Promise<void> => {
  if (isTauri()) return invoke<void>("rebuild_search_index_progress");
};

export const cancelSearchRebuild = async (): Promise<void> => {
  if (isTauri()) return invoke<void>("cancel_search_rebuild");
};

export const searchIndexStatus = async (): Promise<SearchIndexUpdate> => {
  if (isTauri()) return invoke<SearchIndexUpdate>("search_index_status");
  return { state: "fresh", indexed: 0, total: 0 };
};

// P1.3: saved searches
export const listSavedSearches = async (): Promise<SavedSearch[]> => {
  if (isTauri()) return invoke<SavedSearch[]>("list_saved_searches");
  return [];
};

export const saveSearch = async (
  name: string,
  query: string,
): Promise<number> => {
  if (isTauri()) return invoke<number>("save_search", { name, query });
  return Date.now();
};

export const deleteSavedSearch = async (id: number): Promise<void> => {
  if (isTauri()) return invoke<void>("delete_saved_search", { id });
};

// P1.3: rule dry-run + revert
export const previewRules = async (
  accountId: number,
  folder: string,
): Promise<RulePreviewResult> => {
  if (isTauri()) {
    return invoke<RulePreviewResult>("preview_rules", { accountId, folder });
  }
  return { affected: 0, previews: [] };
};

export const revertRules = async (
  accountId: number,
  previews: RulePreview[],
): Promise<number> => {
  if (isTauri()) {
    return invoke<number>("revert_rules", { accountId, previews });
  }
  return 0;
};

// CalDAV synchronization (Roadmap 1.4)
export const syncNow = async (): Promise<void> => {
  if (isTauri()) return invoke<void>("sync_now");
};

export const syncCalendar = async (
  accountId: number,
): Promise<CalendarCollection[]> => {
  if (isTauri())
    return invoke<CalendarCollection[]>("sync_calendar", { accountId });
  return [
    {
      href: "/mock/cal/",
      name: "Primary Calendar",
      color: "#3b5bdb",
      ctag: "ctag-mock-1",
      sync_token: "token-mock-1",
    },
  ];
};

export const discoverCalDav = async (
  server: string,
  address: string,
  password: string,
): Promise<CalendarCollection[]> => {
  if (isTauri()) {
    return invoke<CalendarCollection[]>("discover_caldav", {
      server,
      address,
      password,
    });
  }
  return [
    {
      href: "/mock/cal/",
      name: "Discovered Calendar",
      color: "#0f766e",
      ctag: "ctag-mock",
      sync_token: "token-mock",
    },
  ];
};

// OAuth2 PKCE Sign-in (Roadmap 3.1)
export const getOAuthInit = async (
  provider: "google" | "microsoft365",
  clientId?: string,
  redirectUri?: string,
): Promise<OAuthInitPayload> => {
  if (isTauri()) {
    return invoke<OAuthInitPayload>("get_oauth_init", {
      providerStr: provider,
      clientId,
      redirectUri,
    });
  }
  return {
    auth_url: `https://mock.auth.provider/${provider}?client_id=${clientId || "mock"}`,
    code_verifier: "mock_verifier_xyz",
    redirect_uri: redirectUri || "http://127.0.0.1:8080",
    state: "mock_state",
    client_id: clientId || "mock",
  };
};

export const exchangeOAuthCode = async (
  provider: "google" | "microsoft365",
  code: string,
  codeVerifier: string,
  redirectUri: string,
  clientId?: string,
  clientSecret?: string,
): Promise<Account> => {
  if (isTauri()) {
    return invoke<Account>("exchange_oauth_code", {
      providerStr: provider,
      code,
      codeVerifier,
      redirectUri,
      clientId,
      clientSecret,
    });
  }
  const mockAcc: Account = {
    id: mockAccounts.length + 1,
    address: provider === "google" ? "user@gmail.com" : "user@outlook.com",
    protocol:
      provider === "google" ? "Google (OAuth2)" : "Microsoft 365 (OAuth2)",
    sync_mode: "every 2 min",
    color: provider === "google" ? "#0f766e" : "#3b5bdb",
    connected: true,
    local_bytes: 1048576,
    server: provider === "google" ? "imap.gmail.com" : "outlook.office365.com",
    port: 993,
    tls: true,
    folder_count: 5,
    last_error: null,
  };
  mockAccounts.push(mockAcc);
  return mockAcc;
};

// Threading (Roadmap 3.2)
export const getThreadMessages = async (
  accountId: number,
  threadId: string,
): Promise<MessageDetail[]> => {
  if (isTauri()) {
    return invoke<MessageDetail[]>("get_thread_messages", {
      accountId,
      threadId,
    });
  }
  const detail = getMockMessageDetail(1);
  return detail ? [detail] : [];
};

export const applyThreadAction = async (
  accountId: number,
  threadId: string,
  action: ActionType,
): Promise<void> => {
  if (isTauri()) {
    return invoke<void>("apply_thread_action", {
      accountId,
      threadId,
      action,
    });
  }
};

// Attachments (Roadmap 3.3)
export const saveAttachment = async (
  id: number,
  destinationPath: string,
): Promise<void> => {
  if (isTauri()) {
    return invoke<void>("save_attachment", { id, destinationPath });
  }
};

export const saveAllAttachments = async (
  messageId: number,
  destinationDir: string,
): Promise<number> => {
  if (isTauri()) {
    return invoke<number>("save_all_attachments", {
      messageId,
      destinationDir,
    });
  }
  return 1;
};

// Calendar iTIP/iMIP RSVP (Roadmap 4.1)
export const rsvpInvite = async (
  accountId: number,
  messageId: number,
  partstat: "ACCEPTED" | "TENTATIVE" | "DECLINED",
  comment?: string,
): Promise<void> => {
  if (isTauri()) {
    return invoke<void>("rsvp_invite", {
      accountId,
      messageId,
      partstat,
      comment,
    });
  }
};

// Calendar Subscriptions (Roadmap 4.4)
export const listSubscriptions = async (): Promise<CalendarSubscription[]> => {
  if (isTauri()) {
    return invoke<CalendarSubscription[]>("list_subscriptions");
  }
  return [
    {
      id: 1,
      name: "US National Holidays",
      url: "https://calendar.google.com/calendar/ical/en.usa%23holiday%40group.v.calendar.google.com/public/basic.ics",
      color: "#e03131",
      refreshIntervalMin: 1440,
      lastRefreshedAtMs: Date.now() - 3600000,
      enabled: true,
    },
  ];
};

export const addSubscription = async (
  name: string,
  url: string,
  color: string,
  refreshIntervalMin: number,
): Promise<CalendarSubscription> => {
  if (isTauri()) {
    return invoke<CalendarSubscription>("add_subscription", {
      name,
      url,
      color,
      refreshIntervalMin,
    });
  }
  return {
    id: Date.now(),
    name,
    url,
    color,
    refreshIntervalMin,
    lastRefreshedAtMs: Date.now(),
    enabled: true,
  };
};

export const deleteSubscription = async (id: number): Promise<void> => {
  if (isTauri()) {
    return invoke<void>("delete_subscription", { id });
  }
};

export const syncSubscription = async (id: number): Promise<number> => {
  if (isTauri()) {
    return invoke<number>("sync_subscription", { id });
  }
  return 5;
};

export const syncAllSubscriptions = async (): Promise<number> => {
  if (isTauri()) {
    return invoke<number>("sync_all_subscriptions");
  }
  return 5;
};

// Rules & Filters (Roadmap 3.6)
export const applyRulesToFolder = async (
  accountId: number,
  folder: string,
): Promise<number> => {
  if (isTauri()) {
    return invoke<number>("apply_rules_to_folder", { accountId, folder });
  }
  return 2;
};

export const parseSieveScript = async (script: string): Promise<MailRule[]> => {
  if (isTauri()) {
    return invoke<MailRule[]>("parse_sieve_script", { script });
  }
  return [
    {
      id: "rule_imported_1",
      name: "Imported Rule 1",
      enabled: true,
      matchMode: "all",
      conditions: [
        { field: "from", operator: "contains", value: "alerts@service.com" },
      ],
      actions: [{ moveToFolder: { folderName: "Alerts" } }],
      stopProcessing: true,
    },
  ];
};

export const exportSieveScript = async (rules: MailRule[]): Promise<string> => {
  if (isTauri()) {
    return invoke<string>("export_sieve_script", { rules });
  }
  return `# Generated by Quill Mail (RFC 5228 Sieve)\nrequire ["fileinto", "reject"];\n\nif header :contains "from" "news@site.com" {\n    fileinto "News";\n    stop;\n}\n`;
};

export const markJunk = async (id: number, junk: boolean): Promise<void> => {
  if (isTauri()) {
    return invoke("mark_junk", { id, junk });
  }
};

export const unsubscribe = async (messageId: number): Promise<string> => {
  if (isTauri()) {
    return invoke<string>("unsubscribe", { messageId });
  }
  return "Successfully unsubscribed via One-Click POST (mock).";
};

// Tasks & To-Dos (Roadmap 4.5)
export const listTasks = async (
  accountId?: number,
): Promise<CalendarTask[]> => {
  if (isTauri()) {
    return invoke<CalendarTask[]>("list_tasks", { accountId });
  }
  return [
    {
      id: 1,
      accountId: 1,
      title: "Review Q3 financial statement",
      dueAtMs: Date.now() + 86400000,
      completedAtMs: null,
      priority: 1,
    },
    {
      id: 2,
      accountId: 1,
      title: "Send lease amendment to Tomás",
      dueAtMs: Date.now() + 172800000,
      completedAtMs: null,
      priority: 2,
    },
  ];
};

export const createTask = async (task: CalendarTask): Promise<CalendarTask> => {
  if (isTauri()) {
    return invoke<CalendarTask>("create_task", { task });
  }
  return { ...task, id: Math.floor(Math.random() * 10000) };
};

export const updateTask = async (task: CalendarTask): Promise<void> => {
  if (isTauri()) {
    return invoke<void>("update_task", { task });
  }
};

export const toggleTask = async (id: number): Promise<CalendarTask> => {
  if (isTauri()) {
    return invoke<CalendarTask>("toggle_task", { id });
  }
  return {
    id,
    accountId: 1,
    title: "Toggled task",
    dueAtMs: null,
    completedAtMs: Date.now(),
    priority: null,
  };
};

export const deleteTask = async (id: number): Promise<void> => {
  if (isTauri()) {
    return invoke<void>("delete_task", { id });
  }
};

// Free/Busy Scheduling (Roadmap 4.5)
export const queryFreeBusy = async (
  startMs: number,
  endMs: number,
  slotDurationMinutes = 30,
): Promise<FreeBusySlot[]> => {
  if (isTauri()) {
    return invoke<FreeBusySlot[]>("query_free_busy", {
      startMs,
      endMs,
      slotDurationMinutes,
    });
  }
  const slots: FreeBusySlot[] = [];
  const durationMs = slotDurationMinutes * 60000;
  for (let s = startMs; s < endMs; s += durationMs) {
    const hour = new Date(s).getHours();
    // Simulate busy at 10am and 2pm
    const isBusy = hour === 10 || hour === 14;
    slots.push({
      startMs: s,
      endMs: Math.min(s + durationMs, endMs),
      busy: isBusy,
      attendee: isBusy ? "Busy (Meeting)" : null,
    });
  }
  return slots;
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
