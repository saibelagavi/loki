import { describe, expect, it } from "vitest";
import { columnProblemCounts, filterRows, scrollToRow, windowRange } from "./matrix";
import type { CellStatus, MatrixRow } from "../types";

const row = (key: string, ...statuses: CellStatus[]): MatrixRow => ({
  key,
  cells: statuses.map((status, i) => ({ status, variableId: status === "missing" || status === "none" ? null : i })),
});

const rows = [
  row("DATABASE_URL", "base", "different", "same"),
  row("REDIS_URL", "base", "same", "missing"),
  row("STRIPE_KEY", "base", "different", "empty"),
  row("LOG_LEVEL", "base", "same", "same"),
  row("NEW_FLAG", "missing", "extra", "none"),
];

describe("filterRows", () => {
  it("returns everything for 'all' with no query", () => {
    expect(filterRows(rows, "all", "")).toHaveLength(5);
  });
  it("keeps only rows with a difference for 'problems'", () => {
    expect(filterRows(rows, "problems", "").map((r) => r.key)).toEqual(["DATABASE_URL", "REDIS_URL", "STRIPE_KEY", "NEW_FLAG"]);
  });
  it("keeps only missing or empty rows for 'missing'", () => {
    expect(filterRows(rows, "missing", "").map((r) => r.key)).toEqual(["REDIS_URL", "STRIPE_KEY", "NEW_FLAG"]);
  });
  it("searches by key, case-insensitively, and combines with the filter", () => {
    expect(filterRows(rows, "all", "redis").map((r) => r.key)).toEqual(["REDIS_URL"]);
    expect(filterRows(rows, "problems", "_url").map((r) => r.key)).toEqual(["DATABASE_URL", "REDIS_URL"]);
    expect(filterRows(rows, "all", "  ")).toHaveLength(5);
    expect(filterRows(rows, "all", "nothing")).toHaveLength(0);
  });
});

describe("columnProblemCounts", () => {
  it("counts problem cells per column", () => {
    expect(columnProblemCounts(rows, 3)).toEqual([1, 3, 2]);
  });
});

describe("windowRange", () => {
  it("shows only the visible rows plus overscan", () => {
    expect(windowRange(0, 320, 32, 500, 6)).toEqual({ start: 0, end: 16 });
    expect(windowRange(3200, 320, 32, 500, 6)).toEqual({ start: 94, end: 116 });
  });
  it("clamps at both ends", () => {
    expect(windowRange(0, 320, 32, 5, 6)).toEqual({ start: 0, end: 5 });
    expect(windowRange(-50, 320, 32, 500, 0)).toEqual({ start: 0, end: 10 });
    expect(windowRange(32 * 495, 320, 32, 500, 6).end).toBe(500);
  });
  it("renders a small, constant number of rows for 500 keys", () => {
    const { start, end } = windowRange(6400, 640, 32, 500, 6);
    expect(end - start).toBeLessThan(40);
  });
});

describe("scrollToRow", () => {
  it("leaves the scroll alone when the row is already visible", () => {
    expect(scrollToRow(5, 0, 400, 32, 40)).toBe(0);
  });
  it("scrolls up to a row above the viewport", () => {
    expect(scrollToRow(2, 320, 400, 32, 40)).toBe(64);
  });
  it("scrolls down so the row is fully visible below the sticky header", () => {
    expect(scrollToRow(20, 0, 400, 32, 40)).toBe(20 * 32 + 32 - 360);
  });
});
