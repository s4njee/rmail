import { describe, expect, it } from "vitest";
import { Attendee } from "../types/calendar";
import { summarizeAttendees } from "./attendees";

function person(overrides: Partial<Attendee>): Attendee {
  return {
    id: overrides.id ?? "a1",
    eventId: "e1",
    email: "alice@example.com",
    role: "required",
    status: "needs_action",
    rsvp: true,
    isOrganizer: false,
    ...overrides,
  };
}

describe("summarizeAttendees", () => {
  it("rolls up RSVP counts and ignores the organizer", () => {
    const summary = summarizeAttendees([
      person({ id: "org", email: "me@example.com", isOrganizer: true, status: "accepted" }),
      person({ id: "a", email: "a@example.com", status: "accepted" }),
      person({ id: "b", email: "b@example.com", status: "declined" }),
      person({ id: "c", email: "c@example.com", status: "needs_action" }),
      person({ id: "d", email: "d@example.com", status: "tentative" }),
    ]);
    expect(summary).toEqual({
      total: 4,
      accepted: 1,
      declined: 1,
      pending: 1,
      tentative: 1,
      selfDeclined: false,
    });
  });

  it("marks selfDeclined when the identity email declined", () => {
    const summary = summarizeAttendees(
      [
        person({ id: "me", email: "me@example.com", status: "declined" }),
        person({ id: "a", email: "a@example.com", status: "accepted" }),
      ],
      "me@example.com",
    );
    expect(summary.selfDeclined).toBe(true);
    expect(summary.declined).toBe(1);
  });
});
