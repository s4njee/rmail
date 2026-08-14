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
  return new Date(todayYear, todayMonth, todayDate + offsetDays, hour, minute, 0, 0).getTime();
}

export const MOCK_SETTINGS: AppSettings = {
  theme: "hairline",
  sidebarWidth: null,
  listWidth: null,
  trustedImageSenders: [],
};

export const MOCK_ACCOUNTS: Account[] = [
  {
    id: 1,
    address: "work@quill.app",
    protocol: "IMAP",
    sync_mode: "every 2 min",
    color: "#3b5bdb",
    local_bytes: 218 * 1024 * 1024,
    connected: true,
    server: "imap.quill.app",
    port: 993,
    tls: true,
    folder_count: 3,
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
    folder_count: 0,
  },
  {
    id: 3,
    address: "meridian.board@proton.me",
    protocol: "Bridge",
    sync_mode: "manual",
    color: "#b4451f",
    local_bytes: 53 * 1024 * 1024,
    connected: false,
    server: "bridge.proton.me",
    port: 1143,
    tls: true,
    folder_count: 0,
  },
];

export const MOCK_FOLDERS: Folder[] = [
  { id: 1, name: "Inbox", kind: "inbox", unread_count: 3, total_count: 12 },
  { id: 2, name: "Starred", kind: "starred", unread_count: 0, total_count: 4 },
  { id: 3, name: "Drafts", kind: "drafts", unread_count: 0, total_count: 2 },
  { id: 4, name: "Archive", kind: "archive", unread_count: 0, total_count: 48 },
  { id: 5, name: "Sent", kind: "sent", unread_count: 0, total_count: 5 },
];

export const MOCK_EVENTS: CalendarEvent[] = [
  {
    id: 1,
    account_id: 1,
    title: "Thursday walkthrough",
    start_ms: dateMs(0, 11, 0),
    end_ms: dateMs(0, 12, 0),
    all_day: false,
    location: "Site — Meridian Plaza",
    notes: "Review the lease specs and measure corner office layout.",
  },
  {
    id: 2,
    account_id: 1,
    title: "Lease counsel call",
    start_ms: dateMs(0, 14, 0),
    end_ms: dateMs(0, 14, 30),
    all_day: false,
    location: "Google Meet",
    notes: "Review escalation clause in 4.2.",
  },
  {
    id: 3,
    account_id: 1,
    title: "September board meeting",
    start_ms: dateMs(1, 10, 0),
    end_ms: dateMs(1, 11, 30),
    all_day: false,
    location: "Meridian Boardroom",
    notes: "Budget and sublet policy review.",
  },
  {
    id: 4,
    account_id: 2,
    title: "Photos — print pick-up",
    start_ms: dateMs(2, 15, 0),
    end_ms: dateMs(2, 16, 0),
    all_day: false,
    location: "Downtown Print Studio",
    notes: null,
  },
  {
    id: 5,
    account_id: 1,
    title: "Design System Handoff",
    start_ms: dateMs(-2, 13, 0),
    end_ms: dateMs(-2, 14, 0),
    all_day: false,
    location: "Virtual Room A",
    notes: "Token consistency sync.",
  },
  {
    id: 6,
    account_id: 2,
    title: "Weekend cycling tour",
    start_ms: dateMs(3, 8, 0),
    end_ms: dateMs(3, 12, 0),
    all_day: false,
    location: "Riverside Trail",
    notes: "Bring helmet and hydration pack.",
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
      snippet: "Attached is the redlined lease with the changes we discussed on Tuesday. The two open items...",
      received_at_ms: dateMs(0, 11, 38),
      unread: true,
      flagged: false,
      has_attachments: true,
    },
    body: [
      "Hi both,",
      "Attached is the redlined lease with the changes we discussed on Tuesday. The two open items are the escalation clause in 4.2 and the sublet language in 9. Everything else matches the term sheet.",
      "I'd like to send this to their counsel Friday morning, so any comments by Thursday noon would be ideal.",
      "Thanks,\nRosa",
    ],
    body_html:
      "<p>Hi both,</p><p>Attached is the <a href=\"#\">redlined lease</a> with the changes we discussed on Tuesday. The two open items are the escalation clause in 4.2 and the sublet language in 9. Everything else matches the term sheet.</p><p>I'd like to send this to their counsel Friday morning, so any comments by Thursday noon would be ideal.</p><p>Thanks,<br>Rosa</p>",
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
      snippet: "I think we can live with 3% if they drop the hard cap in 9. Let's confirm with Rosa before Friday.",
      received_at_ms: dateMs(0, 10, 52),
      unread: true,
      flagged: true,
      has_attachments: false,
    },
    body: ["I think we can live with 3% if they drop the hard cap in 9.", "Let's confirm with Rosa before Friday."],
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
      snippet: "A device signed in to your account at 09:14 UTC. If this was you, no action needed.",
      received_at_ms: dateMs(0, 9, 14),
      unread: false,
      flagged: false,
      has_attachments: false,
    },
    body: ["A device signed in to your account at 09:14 UTC.", "If this was you, no action needed."],
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
      snippet: "Three items so far: budget, the lease, and the sublet policy. Please add anything else by the 20th.",
      received_at_ms: dateMs(-1, 17, 0),
      unread: true,
      flagged: true,
      has_attachments: false,
    },
    body: ["Three items so far: budget, the lease, and the sublet policy.", "Please add anything else by the 20th."],
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
      snippet: "Sending the ones that came out well, the rest are a bit blurry.",
      received_at_ms: dateMs(-1, 15, 30),
      unread: false,
      flagged: false,
      has_attachments: false,
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
      has_attachments: false,
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
  return {
    row: found.row,
    body: found.body,
    body_html: found.body_html,
    remote_image_count: 0,
    to: found.to,
    cc: found.cc,
    attachments: found.attachments,
  };
}
