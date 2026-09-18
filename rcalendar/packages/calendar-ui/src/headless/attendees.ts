import { Attendee, AttendeeStatus } from "../types/calendar";

/** Rolled-up RSVP counts for an event (non-organizer invitees). */
export interface AttendeeSummary {
  total: number;
  accepted: number;
  declined: number;
  pending: number;
  tentative: number;
  /** True when `selfEmail` is an invitee who declined. */
  selfDeclined: boolean;
}

export const EMPTY_ATTENDEE_SUMMARY: AttendeeSummary = {
  total: 0,
  accepted: 0,
  declined: 0,
  pending: 0,
  tentative: 0,
  selfDeclined: false,
};

/** Counts invitees by their RSVP status; organizers are excluded from totals. */
export function summarizeAttendees(
  attendees: Attendee[],
  selfEmail?: string | null,
): AttendeeSummary {
  const summary = { ...EMPTY_ATTENDEE_SUMMARY };
  const self = selfEmail?.trim().toLowerCase() || null;
  for (const a of attendees) {
    if (self && a.email.toLowerCase() === self && a.status === "declined") {
      summary.selfDeclined = true;
    }
    if (a.isOrganizer) continue;
    summary.total += 1;
    switch (a.status) {
      case "accepted":
        summary.accepted += 1;
        break;
      case "declined":
        summary.declined += 1;
        break;
      case "tentative":
        summary.tentative += 1;
        break;
      case "delegated":
      case "needs_action":
      default:
        summary.pending += 1;
        break;
    }
  }
  return summary;
}

export const ATTENDEE_STATUS_LABELS: Record<AttendeeStatus, string> = {
  needs_action: "Pending",
  accepted: "Accepted",
  declined: "Declined",
  tentative: "Maybe",
  delegated: "Delegated",
};
