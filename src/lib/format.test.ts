import { describe, expect, it } from "vitest";
import { CELL, baseName, formatDate, plural, relativeTo, stageVar, targetFull } from "./format";

describe("format helpers", () => {
  it("maps stages to the stage colors from the design tokens", () => {
    expect(stageVar("production")).toBe("var(--prod)");
    expect(stageVar("staging")).toBe("var(--staging)");
    expect(stageVar("dev")).toBe("var(--dev)");
    expect(stageVar("local")).toBe("var(--dev)");
  });
  it("handles Windows paths with spaces and mixed separators", () => {
    const p = "C:\\Users\\Name\\My Projects\\app\\.env.production";
    expect(baseName(p)).toBe(".env.production");
    expect(baseName("C:/x/y/.env")).toBe(".env");
    expect(relativeTo(p, "C:\\Users\\Name\\My Projects\\app")).toBe(".env.production");
    expect(relativeTo(p, "c:/users/name/my projects/app/")).toBe(".env.production");
    expect(relativeTo(p, "D:\\other")).toBe(p);
    expect(relativeTo(p, null)).toBe(p);
  });
  it("shows a date-only value as a day, without a time-zone-shifted clock time", () => {
    const s = formatDate("2025-01-01");
    expect(s).toContain("2025");
    expect(s).toMatch(/(^|\D)1(\D|$)/);
    expect(s).not.toMatch(/:\d{2}/);
    expect(formatDate(null)).toBe("never");
  });
  it("pluralizes", () => {
    expect(plural(1, "key")).toBe("1 key");
    expect(plural(2, "key")).toBe("2 keys");
    expect(plural(0, "key")).toBe("0 keys");
  });
  it("describes targets", () => {
    expect(targetFull({ branch: "main", stage: "production", label: null })).toBe("main / production");
    expect(targetFull({ branch: "main", stage: "production", label: "Prod" })).toBe("main / production (Prod)");
  });
  it("gives every visible cell state a symbol AND a word, never color alone", () => {
    for (const [status, c] of Object.entries(CELL)) {
      if (status === "none") continue;
      expect(c.symbol.length).toBeGreaterThan(0);
      expect(c.label.length).toBeGreaterThan(0);
    }
    expect(CELL.same.symbol).toBe("=");
    expect(CELL.different.symbol).toBe("≠");
    expect(CELL.empty.symbol).toBe("∅");
    expect(CELL.extra.symbol).toBe("+");
  });
});
