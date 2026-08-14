import { describe, expect, it } from "vitest";
import { VERSION } from "./index";

describe("calendar-ui scaffold", () => {
  it("reports its package version", () => {
    expect(VERSION).toMatch(/^\d+\.\d+\.\d+$/);
  });
});
