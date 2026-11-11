import type { CellStatus, MatrixRow } from "../types";

export type MatrixFilter = "all" | "problems" | "missing";

const PROBLEM: CellStatus[] = ["different", "missing", "empty", "extra"];
const MISSING: CellStatus[] = ["missing", "empty"];

/** Rows where any cell differs from the baseline in a way that needs a look. */
export function isProblemRow(row: MatrixRow): boolean {
  return row.cells.some((c) => PROBLEM.includes(c.status));
}

export function isMissingRow(row: MatrixRow): boolean {
  return row.cells.some((c) => MISSING.includes(c.status));
}

export function filterRows(rows: MatrixRow[], filter: MatrixFilter, query: string): MatrixRow[] {
  const q = query.trim().toLowerCase();
  return rows.filter((r) => {
    if (q && !r.key.toLowerCase().includes(q)) return false;
    if (filter === "problems") return isProblemRow(r);
    if (filter === "missing") return isMissingRow(r);
    return true;
  });
}

/** Per-column counts of non-ok cells, for the header badges. */
export function columnProblemCounts(rows: MatrixRow[], columns: number): number[] {
  const counts = new Array<number>(columns).fill(0);
  for (const r of rows) {
    r.cells.forEach((c, i) => {
      if (PROBLEM.includes(c.status)) counts[i] += 1;
    });
  }
  return counts;
}

/** Visible slice for a virtualized list: rows [start, end) with a little overscan. */
export function windowRange(scrollTop: number, viewportHeight: number, rowHeight: number, count: number, overscan = 6) {
  const first = Math.floor(Math.max(0, scrollTop) / rowHeight);
  const last = Math.ceil((Math.max(0, scrollTop) + viewportHeight) / rowHeight);
  return { start: Math.max(0, first - overscan), end: Math.min(count, last + overscan) };
}

/** Where the scroll position must go so that row `index` is fully visible below a sticky header. */
export function scrollToRow(index: number, scrollTop: number, viewportHeight: number, rowHeight: number, headerHeight: number): number {
  const top = index * rowHeight;
  const bottom = top + rowHeight;
  const visibleBottom = scrollTop + viewportHeight - headerHeight;
  if (top < scrollTop) return top;
  if (bottom > visibleBottom) return bottom - (viewportHeight - headerHeight);
  return scrollTop;
}
