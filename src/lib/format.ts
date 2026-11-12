import type { CellStatus, Stage, Target } from "../types";

/** CSS variable that carries each stage's color. Stage is the information that matters on push day. */
export function stageVar(stage: Stage): string {
  switch (stage) {
    case "production":
      return "var(--prod)";
    case "staging":
      return "var(--staging)";
    case "dev":
    case "local":
      return "var(--dev)";
    case "preview":
      return "var(--focus)";
    default:
      return "var(--muted)";
  }
}

export function targetName(t: Pick<Target, "branch" | "label">): string {
  return t.label ? t.label : t.branch;
}

export function targetFull(t: Pick<Target, "branch" | "stage" | "label">): string {
  return `${t.branch} / ${t.stage}${t.label ? ` (${t.label})` : ""}`;
}

/** "2026-11-10T12:30:00Z" -> "10 Nov 2026, 12:30" in the user's locale. */
export function formatDate(iso: string | null): string {
  if (!iso) return "never";
  // A date without a time (e.g. a key note's last-rotated date) has no time zone: show the day only.
  const dateOnly = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  if (dateOnly) {
    const d = new Date(Number(dateOnly[1]), Number(dateOnly[2]) - 1, Number(dateOnly[3]));
    return d.toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
  }
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit" });
}

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}

/**
 * Splits a finding message on `backticks` (the core writes key names that way, which reads well in
 * the exported Markdown checklist). Odd-indexed parts are code; an unpaired backtick stays text.
 */
export function splitTicks(message: string): { text: string; code: boolean }[] {
  const parts = message.split("`");
  if (parts.length % 2 === 0) return [{ text: message, code: false }];
  return parts.map((text, i) => ({ text, code: i % 2 === 1 })).filter((p) => p.text !== "");
}

/** Last path segment, handling both Windows and POSIX separators. */
export function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/** Path relative to a root, if it is inside it (case-insensitive on Windows). */
export function relativeTo(path: string, root: string | null): string {
  if (!root) return path;
  const a = path.replace(/\//g, "\\");
  const b = root.replace(/[\\/]+$/, "").replace(/\//g, "\\");
  return a.toLowerCase().startsWith(b.toLowerCase() + "\\") ? a.slice(b.length + 1) : path;
}

/** Symbol, word and tone for each compare cell. Never color alone. */
export const CELL: Record<CellStatus, { symbol: string; label: string; tone: "muted" | "staging" | "prod" | "dev" | "chalk" }> = {
  base: { symbol: "●", label: "base", tone: "chalk" },
  same: { symbol: "=", label: "same", tone: "muted" },
  different: { symbol: "≠", label: "differs", tone: "staging" },
  missing: { symbol: "○", label: "missing", tone: "prod" },
  empty: { symbol: "∅", label: "empty", tone: "muted" },
  extra: { symbol: "+", label: "extra", tone: "dev" },
  none: { symbol: "", label: "", tone: "muted" },
};
