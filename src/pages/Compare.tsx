// The core screen: rows = keys, columns = targets, one column is the baseline.
// Rows are virtualized (only the visible slice is rendered), so 500 keys x 6 targets stay smooth.
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { IconCopy, IconEye } from "../components/icons";
import { StageBadge } from "../components/StageBadge";
import { useToast } from "../components/Toast";
import { CELL, targetName } from "../lib/format";
import { compareMatrix, errMessage } from "../lib/ipc";
import { columnProblemCounts, filterRows, scrollToRow, windowRange, type MatrixFilter } from "../lib/matrix";
import { useSettings } from "../lib/settingsContext";
import { useReveal, type RevealStore } from "../lib/useReveal";
import { stageVar } from "../lib/format";
import type { CompareResult, MatrixCell, MatrixColumn } from "../types";
import type { PageProps } from "./Workspace";

const ROW_H = 34;
const HEAD_H = 72;
const KEY_W = 260;
const COL_W = 176;

const FILTERS: { id: MatrixFilter; label: string }[] = [
  { id: "all", label: "All keys" },
  { id: "problems", label: "Only problems" },
  { id: "missing", label: "Only missing" },
];

export default function Compare({ targets, goTo, focus }: PageProps) {
  const { settings } = useSettings();
  const toast = useToast();
  const store = useReveal(settings.revealSeconds);
  const candidates = useMemo(() => targets.filter((t) => !t.isArchived), [targets]);

  const [selected, setSelected] = useState<number[]>([]);
  const [baselineId, setBaselineId] = useState<number | null>(null);
  const [data, setData] = useState<CompareResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [filter, setFilter] = useState<MatrixFilter>("all");
  const [query, setQuery] = useState("");
  const [active, setActive] = useState({ r: 0, c: 0 });

  const scrollRef = useRef<HTMLDivElement>(null);
  // Search jump: remember the request until the right data is on screen.
  const pendingFocus = useRef<PageProps["focus"]>(null);
  const [flashKey, setFlashKey] = useState<string | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportH, setViewportH] = useState(600);

  // Default selection: every non-archived target that has a snapshot (up to 6).
  useEffect(() => {
    setSelected((cur) => {
      const valid = cur.filter((id) => candidates.some((t) => t.id === id));
      if (valid.length > 0) return valid;
      return candidates.filter((t) => t.currentSnapshotId !== null).slice(0, 6).map((t) => t.id);
    });
  }, [candidates]);

  // Baseline: production if selected, else the first selected target.
  useEffect(() => {
    setBaselineId((cur) => {
      if (cur !== null && selected.includes(cur)) return cur;
      const prod = candidates.find((t) => selected.includes(t.id) && t.stage === "production");
      return prod?.id ?? selected[0] ?? null;
    });
  }, [selected, candidates]);

  useEffect(() => {
    if (selected.length === 0 || baselineId === null || !selected.includes(baselineId)) {
      setData(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    compareMatrix(selected, baselineId)
      .then((r) => !cancelled && setData(r))
      .catch((e) => !cancelled && toast(errMessage(e), "error"))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [selected, baselineId, candidates, toast]);

  const rows = useMemo(() => (data ? filterRows(data.rows, filter, query) : []), [data, filter, query]);

  // A new jump request: show all keys so the row cannot be filtered out.
  useEffect(() => {
    if (!focus) return;
    pendingFocus.current = focus;
    setFilter("all");
    setQuery("");
  }, [focus]);

  // Make sure the target that holds the hit is one of the compared columns.
  useEffect(() => {
    const f = pendingFocus.current;
    if (!f || f.targetId === null) return;
    const id = f.targetId;
    if (candidates.some((t) => t.id === id && t.currentSnapshotId !== null) && !selected.includes(id)) setSelected((cur) => [...cur, id]);
  }, [candidates, selected, focus]);

  // Once the matrix has the row (and column), move the cursor there, centre it and flash it.
  useEffect(() => {
    const f = pendingFocus.current;
    if (!f || !data) return;
    const c = f.targetId !== null ? data.columns.findIndex((col) => col.targetId === f.targetId) : data.baselineIndex;
    if (f.targetId !== null && c < 0) return; // column still loading
    const r = rows.findIndex((x) => x.key === f.key);
    if (r < 0) return;
    pendingFocus.current = null;
    setActive({ r, c: Math.max(0, c) });
    const el = scrollRef.current;
    if (el) {
      el.scrollTop = Math.max(0, r * ROW_H - (el.clientHeight - HEAD_H) / 2 + ROW_H / 2);
      el.focus({ preventScroll: true });
    }
    setFlashKey(f.key);
    const t = window.setTimeout(() => setFlashKey(null), 2600);
    return () => window.clearTimeout(t);
  }, [data, rows]);
  const problemCounts = useMemo(() => (data ? columnProblemCounts(data.rows, data.columns.length) : []), [data]);
  const cols = data?.columns.length ?? 0;

  // Keep the cursor inside the grid when rows or columns change.
  useEffect(() => {
    setActive((a) => ({ r: Math.min(a.r, Math.max(0, rows.length - 1)), c: Math.min(a.c, Math.max(0, cols - 1)) }));
  }, [rows.length, cols]);

  // Track the scroll viewport size for windowing.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setViewportH(el.clientHeight));
    ro.observe(el);
    setViewportH(el.clientHeight);
    return () => ro.disconnect();
  }, [data]);

  const { start, end } = windowRange(scrollTop, Math.max(0, viewportH - HEAD_H), ROW_H, rows.length);
  const template = `${KEY_W}px repeat(${cols}, minmax(${COL_W}px, 1fr))`;

  const moveTo = useCallback(
    (r: number, c: number) => {
      const rr = Math.max(0, Math.min(rows.length - 1, r));
      const cc = Math.max(0, Math.min(cols - 1, c));
      setActive({ r: rr, c: cc });
      const el = scrollRef.current;
      if (el) el.scrollTop = scrollToRow(rr, el.scrollTop, el.clientHeight, ROW_H, HEAD_H);
    },
    [rows.length, cols],
  );

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    if (rows.length === 0) return;
    const page = Math.max(1, Math.floor((viewportH - HEAD_H) / ROW_H) - 1);
    const cell = rows[active.r]?.cells[active.c];
    switch (e.key) {
      case "ArrowDown":
        return e.preventDefault(), moveTo(active.r + 1, active.c);
      case "ArrowUp":
        return e.preventDefault(), moveTo(active.r - 1, active.c);
      case "ArrowRight":
        return e.preventDefault(), moveTo(active.r, active.c + 1);
      case "ArrowLeft":
        return e.preventDefault(), moveTo(active.r, active.c - 1);
      case "PageDown":
        return e.preventDefault(), moveTo(active.r + page, active.c);
      case "PageUp":
        return e.preventDefault(), moveTo(active.r - page, active.c);
      case "Home":
        return e.preventDefault(), moveTo(e.ctrlKey ? 0 : active.r, 0);
      case "End":
        return e.preventDefault(), moveTo(e.ctrlKey ? rows.length - 1 : active.r, cols - 1);
      case "Enter":
        if (cell?.variableId != null) {
          e.preventDefault();
          if (store.get(cell.variableId) !== undefined) store.hide(cell.variableId);
          else void store.reveal(cell.variableId);
        }
        return;
      case "Escape":
        return store.hideAll();
      case "c":
      case "C":
        if ((e.ctrlKey || e.metaKey) && cell?.variableId != null) {
          e.preventDefault();
          void store.copy(cell.variableId);
        }
        return;
    }
  }

  if (candidates.length === 0) {
    return (
      <Empty title="Nothing to compare yet" action="Add targets" onAction={() => goTo("targets")}>
        Add targets (a branch plus a stage), then import each one's .env file. The compare matrix will show what differs and what is missing.
      </Empty>
    );
  }

  const withSnapshots = candidates.filter((t) => t.currentSnapshotId !== null);
  if (withSnapshots.length === 0) {
    return (
      <Empty title="Import a .env file first" action="Go to targets" onAction={() => goTo("targets")}>
        Your targets have no snapshots yet. Import a .env file into each one, then come back to compare them.
      </Empty>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2 border-b border-line px-5 py-2.5">
        <fieldset className="flex flex-wrap items-center gap-1.5">
          <legend className="sr-only">Targets to compare</legend>
          {candidates.map((t) => {
            const on = selected.includes(t.id);
            return (
              <button
                key={t.id}
                className={`btn btn-sm ${on ? "border-focus" : "text-muted"}`}
                aria-pressed={on}
                disabled={t.currentSnapshotId === null}
                title={t.currentSnapshotId === null ? "No snapshot yet" : undefined}
                onClick={() => setSelected((cur) => (on ? cur.filter((id) => id !== t.id) : cur.length >= 12 ? cur : [...cur, t.id]))}
              >
                <span aria-hidden className="inline-block h-2 w-2 rounded-full" style={{ background: stageVar(t.stage) }} />
                {targetName(t)}
              </button>
            );
          })}
        </fieldset>
        <label className="flex items-center gap-1.5 text-[12px] text-muted">
          Baseline
          <select className="input w-auto" value={baselineId ?? ""} onChange={(e) => setBaselineId(Number(e.target.value))} disabled={selected.length === 0}>
            {selected.map((id) => {
              const t = candidates.find((x) => x.id === id);
              return t ? (
                <option key={id} value={id}>
                  {targetName(t)} ({t.stage})
                </option>
              ) : null;
            })}
          </select>
        </label>
        <div role="group" aria-label="Filter" className="flex gap-1">
          {FILTERS.map((f) => (
            <button key={f.id} className={`btn btn-sm ${filter === f.id ? "border-focus" : "text-muted"}`} aria-pressed={filter === f.id} onClick={() => setFilter(f.id)}>
              {f.label}
            </button>
          ))}
        </div>
        <input className="input max-w-[220px]" placeholder="Search keys" aria-label="Search keys" value={query} onChange={(e) => setQuery(e.target.value)} />
        <span className="ml-auto text-[12px] text-muted" aria-live="polite">
          {data ? `${rows.length} of ${data.rows.length} keys` : loading ? "Loading…" : ""}
        </span>
      </div>

      {selected.length === 0 || !data ? (
        <p className="p-6 text-[13px] text-muted">{selected.length === 0 ? "Pick at least one target above." : "Loading…"}</p>
      ) : (
        <div
          ref={scrollRef}
          role="grid"
          tabIndex={0}
          aria-label="Compare matrix. Use the arrow keys to move, Enter to reveal a value, Control+C to copy it."
          aria-rowcount={rows.length + 1}
          aria-colcount={cols + 1}
          aria-activedescendant={rows.length > 0 ? `cell-${active.r}-${active.c}` : undefined}
          onKeyDown={onKeyDown}
          onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
          className="relative min-h-0 flex-1 overflow-auto"
        >
          <div style={{ minWidth: KEY_W + cols * COL_W }}>
            <div role="row" className="sticky top-0 z-20 grid border-b border-line bg-surface" style={{ gridTemplateColumns: template, height: HEAD_H }}>
              <div role="columnheader" className="sticky left-0 z-30 flex items-end bg-surface px-3 pb-2 text-[12px] text-muted">
                Key
              </div>
              {data.columns.map((c, i) => (
                <ColumnHeader key={c.targetId} column={c} baseline={i === data.baselineIndex} problems={problemCounts[i] ?? 0} />
              ))}
            </div>

            {rows.length === 0 ? (
              <p className="p-6 text-[13px] text-muted">No keys match. Clear the search or switch to All keys.</p>
            ) : (
              <div role="rowgroup" style={{ position: "relative", height: rows.length * ROW_H }}>
                {rows.slice(start, end).map((row, k) => {
                  const r = start + k;
                  return (
                    <div key={row.key} role="row" aria-rowindex={r + 2} className="absolute left-0 right-0 grid border-b border-line" style={{ top: r * ROW_H, height: ROW_H, gridTemplateColumns: template }}>
                      <div role="rowheader" className={`mono sticky left-0 z-10 flex items-center truncate bg-surface px-3 text-[13px] ${r === active.r ? "font-semibold" : ""}`}>
                        <span className={`truncate ${flashKey === row.key ? "found-flash rounded px-1" : ""}`}>
                          <Highlight text={row.key} query={query} />
                        </span>
                      </div>
                      {row.cells.map((cell, c) => (
                        <Cell
                          key={c}
                          id={`cell-${r}-${c}`}
                          label={`${row.key}, ${targetName(data.columns[c])}: ${CELL[cell.status].label || "not set"}`}
                          cell={cell}
                          isActive={r === active.r && c === active.c}
                          flash={flashKey === row.key && c === active.c}
                          store={store}
                          onSelect={() => {
                            setActive({ r, c });
                            scrollRef.current?.focus({ preventScroll: true });
                          }}
                        />
                      ))}
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </div>
      )}

      <div className="flex flex-wrap items-center gap-x-5 gap-y-1 border-t border-line px-5 py-2 text-[12px] text-muted">
        {(["same", "different", "missing", "empty", "extra"] as const).map((s) => (
          <span key={s} className="flex items-center gap-1.5">
            <span aria-hidden className={`tone-${CELL[s].tone}`}>
              {CELL[s].symbol}
            </span>
            {CELL[s].label}
          </span>
        ))}
        <span className="ml-auto">Values stay masked. Enter reveals for {settings.revealSeconds} s, Ctrl+C copies.</span>
      </div>
    </div>
  );
}

function ColumnHeader({ column, baseline, problems }: { column: MatrixColumn; baseline: boolean; problems: number }) {
  return (
    <div role="columnheader" className="relative flex flex-col justify-end gap-0.5 border-l border-line px-2.5 pb-2" style={{ minWidth: 0 }}>
      <span aria-hidden className="absolute left-0 right-0 top-0 h-1" style={{ background: stageVar(column.stage) }} />
      <span className="truncate text-[13px] font-medium">{targetName(column)}</span>
      <span className="flex items-center gap-2 truncate text-[12px]">
        <StageBadge stage={column.stage} />
        <span className="text-muted">{column.version !== null ? `v${column.version} · ${column.keyCount} keys` : "no snapshot"}</span>
      </span>
      <span className="flex gap-1 text-[12px] text-muted">
        {baseline ? <span className="chip">baseline</span> : problems > 0 ? <span className="chip tone-staging">{problems} to check</span> : <span className="chip">in sync</span>}
      </span>
    </div>
  );
}

/** Marks the part of a key that matches the search box. */
function Highlight({ text, query }: { text: string; query: string }) {
  const q = query.trim();
  if (!q) return <>{text}</>;
  const i = text.toLowerCase().indexOf(q.toLowerCase());
  if (i < 0) return <>{text}</>;
  return (
    <>
      {text.slice(0, i)}
      <mark className="hit">{text.slice(i, i + q.length)}</mark>
      {text.slice(i + q.length)}
    </>
  );
}

interface CellProps {
  flash?: boolean;
  id: string;
  label: string;
  cell: MatrixCell;
  isActive: boolean;
  store: RevealStore;
  onSelect: () => void;
}

function Cell({ id, label, cell, isActive, store, onSelect, flash }: CellProps) {
  const c = CELL[cell.status];
  const vid = cell.variableId;
  const revealed = vid !== null ? store.get(vid) : undefined;
  const left = vid !== null ? store.remaining(vid) : 0;

  return (
    <div
      role="gridcell"
      id={id}
      aria-label={label}
      aria-selected={isActive}
      onMouseDown={onSelect}
      onDoubleClick={() => vid !== null && void store.reveal(vid)}
      className={`group relative flex min-w-0 items-center gap-1.5 border-l border-line px-2.5 ${isActive ? "bg-ink" : ""} ${flash ? "found-flash" : ""}`}
      style={isActive ? { outline: "2px solid var(--focus)", outlineOffset: -2 } : undefined}
    >
      {cell.status !== "none" && (
        <>
          <span aria-hidden className={`tone-${c.tone} w-4 shrink-0 text-center`}>
            {c.symbol}
          </span>
          {revealed !== undefined ? (
            <span className={`flex min-w-0 flex-1 items-center gap-2 ${left <= 1 ? "fade-out" : ""}`}>
              <span className="mono truncate text-[12px]">{revealed === "" ? "(empty)" : revealed}</span>
              <span className="ml-auto shrink-0 text-[12px] text-muted">{left} s</span>
            </span>
          ) : (
            <span className={`truncate text-[12px] tone-${c.tone} ${cell.status === "empty" ? "cell-dashed rounded px-1" : ""}`}>{c.label}</span>
          )}
          {vid !== null && revealed === undefined && (
            <span className={`ml-auto shrink-0 gap-0.5 ${isActive ? "flex" : "hidden group-hover:flex"}`}>
              <button tabIndex={-1} className="btn btn-quiet btn-sm" aria-label="Reveal value" onClick={() => void store.reveal(vid)}>
                <IconEye />
              </button>
              <button tabIndex={-1} className="btn btn-quiet btn-sm" aria-label="Copy value" onClick={() => void store.copy(vid)}>
                <IconCopy />
              </button>
            </span>
          )}
        </>
      )}
    </div>
  );
}

function Empty({ title, children, action, onAction }: { title: string; children: string; action: string; onAction: () => void }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
      <h2 className="text-[18px] font-semibold">{title}</h2>
      <p className="max-w-[460px] text-[13px] text-muted">{children}</p>
      <button className="btn btn-primary" onClick={onAction}>
        {action}
      </button>
    </div>
  );
}
