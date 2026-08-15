import type { Account } from "./ipc/Account";
import type { AppSettings } from "./ipc/AppSettings";
import type { CalendarEvent } from "./ipc/CalendarEvent";
import type { Folder } from "./ipc/Folder";
import type { MessageDetail } from "./ipc/MessageDetail";
import type { MessagePage } from "./ipc/MessagePage";
import type { MessageQuery } from "./ipc/MessageQuery";
import type { MessageRow } from "./ipc/MessageRow";

const now = new Date();
const todayYear = now.getFullYear();
const todayMonth = now.getMonth();
const todayDate = now.getDate();

function dateMs(offsetDays: number, hour: number, minute: number): number {
  return new Date(
    todayYear,
    todayMonth,
    todayDate + offsetDays,
    hour,
    minute,
    0,
    0,
  ).getTime();
}

export const MOCK_SETTINGS: AppSettings = {
  theme: "hairline",
  sidebarWidth: null,
  listWidth: null,
  trustedImageSenders: [],
  conversationThreading: true,
  undoSendDelaySec: 10,
  blockRemoteImages: true,
  identities: [
    {
      id: "id_work_default",
      accountId: 1,
      name: "Sanjee",
      email: "work@quill.app",
      replyTo: null,
      signature: {
        plainText: "-- \nSanjee\nLead Engineer @ Quill",
        html: '<br>-- <br><b>Sanjee</b><br><span style="color:#6b7280">Lead Engineer @ Quill</span>',
        includeInNewMail: true,
        includeInReplies: true,
        replyPlacement: "above_quote",
      },
      isDefault: true,
    },
    {
      id: "id_work_alias",
      accountId: 1,
      name: "Sanjee (Support)",
      email: "support@quill.app",
      replyTo: "support-team@quill.app",
      signature: {
        plainText: "-- \nQuill Support Team\nhttps://quill.app/help",
        html: '<br>-- <br><b>Quill Support Team</b><br><a href="https://quill.app/help">quill.app/help</a>',
        includeInNewMail: true,
        includeInReplies: true,
        replyPlacement: "above_quote",
      },
      isDefault: false,
    },
  ],
  notifications: {
    enabled: true,
    sound: true,
    dockBadge: true,
    quietHoursEnabled: false,
    quietHoursStart: "22:00",
    quietHoursEnd: "08:00",
    knownContactsOnly: false,
    defaultAlarmMinutes: 15,
    perAccount: [],
  },
  rules: [
    {
      id: "rule_newsletters",
      name: "Route Newsletters",
      enabled: true,
      matchMode: "any",
      conditions: [
        { field: "from", operator: "contains", value: "newsletter" },
        { field: "subject", operator: "contains", value: "digest" },
      ],
      actions: [{ moveToFolder: { folderName: "Archive" } }, "markRead"],
      stopProcessing: false,
    },
    {
      id: "rule_urgent_team",
      name: "Star Urgent Team Mail",
      enabled: true,
      matchMode: "all",
      conditions: [
        { field: "from", operator: "contains", value: "quill.app" },
        { field: "subject", operator: "contains", value: "Urgent" },
      ],
      actions: ["markFlagged"],
      stopProcessing: true,
    },
  ],
  primaryTimezone: null,
  secondaryTimezone: null,
  showSecondaryTimezone: false,
  crashReportingEnabled: false,
  usagePingEnabled: false,
  logLevel: "info",
};

export const MOCK_ACCOUNTS: Account[] = [
  {
    id: 1,
    address: "work@quill.app",
    protocol: "IMAP",
    sync_mode: "every 2 min",
    color: "#3b5bdb",
    local_bytes: 384 * 1024 * 1024,
    connected: true,
    server: "imap.quill.app",
    port: 993,
    tls: true,
    folder_count: 5,
    last_error: null,
  },
  {
    id: 2,
    address: "rosa.personal@fastmail.com",
    protocol: "IMAP",
    sync_mode: "on open",
    color: "#0f766e",
    local_bytes: 141 * 1024 * 1024,
    connected: true,
    server: "imap.fastmail.com",
    port: 993,
    tls: true,
    folder_count: 2,
    last_error: null,
  },
  {
    id: 3,
    address: "meridian.board@proton.me",
    protocol: "Bridge",
    sync_mode: "manual",
    color: "#b4451f",
    local_bytes: 53 * 1024 * 1024,
    connected: true,
    server: "127.0.0.1",
    port: 1143,
    tls: false,
    folder_count: 1,
    last_error: null,
  },
];

export const MOCK_FOLDERS: Folder[] = [
  { id: 1, name: "Inbox", kind: "inbox", unread_count: 12, total_count: 12 },
  { id: 2, name: "Starred", kind: "starred", unread_count: 4, total_count: 4 },
  { id: 3, name: "Drafts", kind: "drafts", unread_count: 0, total_count: 2 },
  { id: 4, name: "Sent", kind: "sent", unread_count: 0, total_count: 8 },
  { id: 5, name: "Archive", kind: "archive", unread_count: 0, total_count: 31 },
  { id: 6, name: "Junk", kind: "junk", unread_count: 0, total_count: 1 },
  { id: 7, name: "Trash", kind: "trash", unread_count: 0, total_count: 0 },
];

export const MOCK_EVENTS: CalendarEvent[] = [
  {
    id: 1,
    account_id: 1,
    title: "1:1 with Rosa",
    start_ms: dateMs(0, 14, 0),
    end_ms: dateMs(0, 14, 30),
    all_day: false,
    location: "Meeting Room B",
    notes: "Review Q3 goals and roadmap sync",
    alarm_minutes_before: 15,
    timezone: "America/New_York",
    travel_time_minutes: 15,
    calendar_source: null,
    calendar_name: null,
    calendar_color: null,
    color: null,
  },
  {
    id: 2,
    account_id: 1,
    title: "Product Design Review",
    start_ms: dateMs(0, 16, 0),
    end_ms: dateMs(0, 17, 0),
    all_day: false,
    location: "https://meet.google.com/abc-defg-hij",
    notes: "Quarterly review of typography, theme colors, and layout polish",
    alarm_minutes_before: 10,
    timezone: "America/New_York",
    travel_time_minutes: null,
    calendar_source: null,
    calendar_name: null,
    calendar_color: null,
    color: null,
  },
  {
    id: 3,
    account_id: 3,
    title: "Meridian Board — September Meeting",
    start_ms: dateMs(1, 10, 0),
    end_ms: dateMs(1, 12, 0),
    all_day: false,
    location: "Conference Hall A, 100 Main St",
    notes: "Annual budget review and lease approvals",
    alarm_minutes_before: 30,
    timezone: "America/New_York",
    travel_time_minutes: 30,
    calendar_source: null,
    calendar_name: null,
    calendar_color: null,
    color: null,
  },
  {
    id: 4,
    account_id: 2,
    title: "Dinner at Bistro",
    start_ms: dateMs(0, 19, 0),
    end_ms: dateMs(0, 21, 0),
    all_day: false,
    location: "Bistro Central, 45 Elm Street",
    notes: null,
    alarm_minutes_before: null,
    timezone: "America/New_York",
    travel_time_minutes: 20,
    calendar_source: null,
    calendar_name: null,
    calendar_color: null,
    color: null,
  },
];

interface MockRawMsg {
  row: MessageRow;
  body: string[];
  body_html: string | null;
  to: { name: string; address: string }[];
  cc: { name: string; address: string }[];
  attachments: {
    id: number;
    message_id: number;
    filename: string;
    size_bytes: number;
    on_disk: boolean;
  }[];
}

export const MOCK_MESSAGES: MockRawMsg[] = [
  {
    row: {
      id: 1,
      account_id: 1,
      folder: "Inbox",
      sender_name: "Rosa Delgado",
      sender_address: "rosa@meridianproperty.co",
      subject: "Draft agreement for the Meridian lease",
      snippet:
        "Attached is the redlined lease with the changes we discussed on Tuesday. The two open items...",
      received_at_ms: dateMs(0, 11, 38),
      unread: true,
      flagged: false,
      answered: false,
      forwarded: false,
      has_attachments: true,
      thread_id: "th_lease_agreement",
      thread_count: 2,
    },
    body: [
      "Hi both,",
      "Attached is the redlined lease with the changes we discussed on Tuesday. The two open items are the escalation clause in 4.2 and the sublet language in 9. Everything else matches the term sheet.",
      "I'd like to send this to their counsel Friday morning, so any comments by Thursday noon would be ideal.",
      "Thanks,\nRosa",
    ],
    body_html:
      '<p>Hi both,</p><p>Attached is the <a href="#">redlined lease</a> with the changes we discussed on Tuesday. The two open items are the escalation clause in 4.2 and the sublet language in 9. Everything else matches the term sheet.</p><p>I\'d like to send this to their counsel Friday morning, so any comments by Thursday noon would be ideal.</p><p>Thanks,<br>Rosa</p>',
    to: [
      { name: "me", address: "work@quill.app" },
      { name: "David Okoye", address: "david.okoye@okoyeassociates.com" },
    ],
    cc: [],
    attachments: [
      {
        id: 1,
        message_id: 1,
        filename: "meridian-lease-v4.pdf",
        size_bytes: 253952,
        on_disk: true,
      },
    ],
  },
  {
    row: {
      id: 2,
      account_id: 1,
      folder: "Inbox",
      sender_name: "David Okoye",
      sender_address: "david.okoye@okoyeassociates.com",
      subject: "Re: escalation clause in 4.2",
      snippet:
        "I think we can live with 3% if they drop the hard cap in 9. Let's confirm with Rosa before Friday.",
      received_at_ms: dateMs(0, 10, 52),
      unread: true,
      flagged: true,
      answered: true,
      forwarded: false,
      has_attachments: false,
      thread_id: "th_lease_agreement",
      thread_count: 2,
    },
    body: [
      "I think we can live with 3% if they drop the hard cap in 9.",
      "Let's confirm with Rosa before Friday.",
    ],
    body_html: null,
    to: [{ name: "me", address: "work@quill.app" }],
    cc: [],
    attachments: [],
  },
  {
    row: {
      id: 3,
      account_id: 2,
      folder: "Inbox",
      sender_name: "Fastmail Security",
      sender_address: "security@fastmail.com",
      subject: "New sign-in from Lisbon",
      snippet:
        "A device signed in to your account at 09:14 UTC. If this was you, no action needed.",
      received_at_ms: dateMs(0, 9, 14),
      unread: false,
      flagged: false,
      answered: false,
      forwarded: false,
      has_attachments: false,
      thread_id: "th_fastmail_security",
      thread_count: 1,
    },
    body: [
      "A device signed in to your account at 09:14 UTC.",
      "If this was you, no action needed.",
    ],
    body_html: null,
    to: [{ name: "me", address: "rosa.personal@fastmail.com" }],
    cc: [],
    attachments: [],
  },
  {
    row: {
      id: 4,
      account_id: 3,
      folder: "Inbox",
      sender_name: "Meridian Board",
      sender_address: "board@meridianproperty.co",
      subject: "Agenda — September meeting",
      snippet:
        "Three items so far: budget, the lease, and the sublet policy. Please add anything else by the 20th.",
      received_at_ms: dateMs(-1, 17, 0),
      unread: true,
      flagged: true,
      answered: false,
      forwarded: false,
      has_attachments: false,
      thread_id: "th_meridian_agenda",
      thread_count: 1,
    },
    body: [
      "Three items so far: budget, the lease, and the sublet policy.",
      "Please add anything else by the 20th.",
    ],
    body_html: null,
    to: [{ name: "me", address: "meridian.board@proton.me" }],
    cc: [],
    attachments: [],
  },
  {
    row: {
      id: 5,
      account_id: 2,
      folder: "Inbox",
      sender_name: "Priya Raman",
      sender_address: "priya.raman@gmail.com",
      subject: "Photos from the weekend",
      snippet:
        "Sending the ones that came out well, the rest are a bit blurry.",
      received_at_ms: dateMs(-1, 15, 30),
      unread: false,
      flagged: false,
      answered: false,
      forwarded: true,
      has_attachments: false,
      thread_id: "th_priya_photos",
      thread_count: 1,
    },
    body: ["Sending the ones that came out well, the rest are a bit blurry."],
    body_html: null,
    to: [{ name: "me", address: "rosa.personal@fastmail.com" }],
    cc: [],
    attachments: [],
  },
  {
    row: {
      id: 6,
      account_id: 1,
      folder: "Inbox",
      sender_name: "Ledger Accounts",
      sender_address: "invoices@ledger.app",
      subject: "Invoice 2841 paid",
      snippet: "€4,200.00 received from Meridian Property Co.",
      received_at_ms: dateMs(-2, 14, 0),
      unread: false,
      flagged: true,
      answered: false,
      forwarded: false,
      has_attachments: false,
      thread_id: "th_ledger_invoice",
      thread_count: 1,
    },
    body: ["€4,200.00 received from Meridian Property Co."],
    body_html: null,
    to: [{ name: "me", address: "work@quill.app" }],
    cc: [],
    attachments: [],
  },
];

export function getMockMessagePage(query: MessageQuery): MessagePage {
  let filtered = [...MOCK_MESSAGES];
  if (query.account_id !== null) {
    filtered = filtered.filter((m) => m.row.account_id === query.account_id);
  }
  if (query.folder !== null) {
    filtered = filtered.filter((m) => m.row.folder === query.folder);
  }

  const items = filtered.map((m) => m.row);
  return {
    items,
    total: items.length,
  };
}

export function getMockMessageDetail(id: number): MessageDetail | null {
  const found = MOCK_MESSAGES.find((m) => m.row.id === id);
  if (!found) return null;

  const calendar_invite =
    found.row.id === 4
      ? {
          method: "REQUEST",
          uid: "evt-meridian-board-sep",
          sequence: 1,
          title: "Meridian Board — September Meeting",
          startMs: found.row.received_at_ms + 86400000,
          endMs: found.row.received_at_ms + 86400000 + 7200000,
          allDay: false,
          location: "Meridian Boardroom & Virtual",
          organizerName: "Meridian Board",
          organizerEmail: "board@meridianproperty.co",
          userPartstat: "NEEDS-ACTION",
          attendees: [
            {
              name: "Me",
              email: "meridian.board@proton.me",
              partstat: "NEEDS-ACTION",
              role: "REQ-PARTICIPANT",
            },
            {
              name: "Rosa Delgado",
              email: "rosa@meridianproperty.co",
              partstat: "ACCEPTED",
              role: "REQ-PARTICIPANT",
            },
          ],
          rawIcs: "",
          timezone: "America/New_York",
        }
      : null;

  return {
    row: found.row,
    body: found.body,
    body_html: found.body_html,
    remote_image_count: 0,
    to: found.to,
    cc: found.cc,
    bcc: [],
    attachments: found.attachments,
    message_id_header: `<mock-${found.row.id}@quill.app>`,
    in_reply_to: null,
    references: null,
    thread_id: found.row.thread_id,
    calendar_invite,
    list_unsubscribe:
      found.row.id === 9
        ? "<https://proton.me/unsubscribe?id=bridge-updates>, <mailto:unsub@proton.me?subject=unsubscribe>"
        : null,
    list_unsubscribe_post:
      found.row.id === 9 ? "List-Unsubscribe=One-Click" : null,
  };
}

export function searchMock(
  query: string,
  folder: string | null = null,
  accountId: number | null = null,
): import("./ipc/SearchMatch").SearchMatch[] {
  const q = query.toLowerCase().trim();
  if (!q) return [];
  const results: import("./ipc/SearchMatch").SearchMatch[] = [];

  for (const m of MOCK_MESSAGES) {
    if (folder && m.row.folder !== folder) continue;
    if (accountId && m.row.account_id !== accountId) continue;

    const fullText =
      `${m.row.subject} ${m.row.sender_name} ${m.row.sender_address} ${m.row.snippet} ${m.body.join(" ")}`.toLowerCase();
    if (fullText.includes(q)) {
      const snippet = m.row.snippet.replace(
        new RegExp(`(${q})`, "gi"),
        "<mark>$1</mark>",
      );
      results.push({
        kind: "message",
        id: m.row.id,
        account_id: m.row.account_id,
        folder: m.row.folder,
        title: m.row.subject,
        subtitle: m.row.sender_name
          ? `${m.row.sender_name} <${m.row.sender_address}>`
          : m.row.sender_address,
        snippet,
        timestamp_ms: m.row.received_at_ms,
      });
    }
  }

  for (const ev of MOCK_EVENTS) {
    if (folder) continue;
    if (accountId && ev.account_id !== accountId) continue;
    const text =
      `${ev.title} ${ev.location || ""} ${ev.notes || ""}`.toLowerCase();
    if (text.includes(q)) {
      results.push({
        kind: "event",
        id: ev.id,
        account_id: ev.account_id,
        folder: null,
        title: ev.title,
        subtitle: ev.location || "Calendar event",
        snippet: ev.notes
          ? ev.notes.replace(new RegExp(`(${q})`, "gi"), "<mark>$1</mark>")
          : "Calendar event",
        timestamp_ms: ev.start_ms,
      });
    }
  }

  return results;
}
