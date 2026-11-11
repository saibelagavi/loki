// Diff checker: compare actual values between any two sources. Equality comes from the core
// (fingerprints), so nothing is shown until you ask. Revealed values show a character diff and
// are dropped after the configured time.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { StageBadge } from "../components/StageBadge";
import { useToast } from "../components/Toast";
import { charDiff, visible } from "../lib/charDiff";
import { baseName, formatDate, plural, targetName } from "../lib/format";
import { errMessage, pickFile, snapshotList, targetCatalog, valueDiff } from "../lib/ipc";
import type { CatalogEntry, DiffSource, Snapshot, ValueDiffResult, ValueRow, ValueStatus } from "../types";

type Mode = "target" | "snapshot" | "file" | "text";

interface Side {
  mode: Mode;
  projectId: number | null;
  targetId: number | null;
  snapshotId: number | null;
  path: string | null;
}

const EMPTY_SIDE: Side = { mode: "target", projectId: null, targetId: null, snapshotId: null, path: null };

const STATUS: Record<ValueStatus, { symbol: string; label: string; tone: string }> = {
  same: { symbol: "=", label: "same", tone: "tone-muted" },
  changed: { symbol: "≠", label: "differs", tone: "tone-staging" },
  removed: { symbol: "○", label: "only left", tone: "tone-prod" },
  added: { symbol: "+", label: "only right", tone: "tone-dev" },
};

interface Revealed {
  values: Record<string, { left: string | null; right: string | null }>;
  expires: number;
}

export default function DiffChecker() {
  const toast = useToast();
  const [catalog, setCatalog] = useState<CatalogEntry[]>([]);
  const [left, setLeft] = useState<Side>(EMPTY_SIDE);
  const [right, setRight] = useState<Side>(EMPTY_SIDE);
  const leftText = useRef<HTMLTextAreaElement>(null);
  const rightText = useRef<HTMLTextAreaElement>(null);
  const [result, setResult] = useState<ValueDiffResult | null>(null);
  const [revealed, setRevealed] = useState<Revealed | null>(null);
  const [onlyDiffs, setOnlyDiffs] = useState(true);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [, setTick] = useState(0);
  const lastSources = useRef<{ l: DiffSource; r: DiffSource } | null>(null);

  // Defaults: staging (or anything) on the left, production on the right, same project.
  useEffect(() => {
    targetCatalog()
      .then((c) => {
        setCatalog(c);
        const proj = c.find((e) => e.targets.filter((t) => t.currentSnapshotId !== null).length >= 2) ?? c.find((e) => e.targets.length > 0);
        if (!proj) return;
        const usable = proj.targets.filter((t) => !t.isArchived);
        const prod = usable.find((t) => t.stage === "production") ?? usable[0];
        const other = usable.find((t) => t.stage === "staging") ?? usable.find((t) => t.id !== prod?.id) ?? prod;
        setLeft({ ...EMPTY_SIDE, projectId: proj.project.id, targetId: other?.id ?? null });
        setRight({ ...EMPTY_SIDE, projectId: proj.project.id, targetId: prod?.id ?? null });
      })
      .catch((e) => toast(errMessage(e), "error"));
  }, [toast]);

  const hide = useCallback(() => setRevealed(null), []);

  // Countdown, then drop the values.
  useEffect(() => {
    if (!revealed) return;
    const t = window.setInterval(() => {
      if (Date.now() >= revealed.expires) setRevealed(null);
      else setTick((n) => n + 1);
    }, 250);
    return () => window.clearInterval(t);
  }, [revealed]);

  // A change of source invalidates the result and any revealed values.
  useEffect(() => {
    setResult(null);
    setRevealed(null);
  }, [left, right]);

  function toSource(side: Side, text: HTMLTextAreaElement | null): DiffSource | null {
    switch (side.mode) {
      case "target":
        return side.targetId !== null ? { kind: "target", targetId: side.targetId } : null;
      case "snapshot":
        return side.snapshotId !== null ? { kind: "snapshot", snapshotId: side.snapshotId } : null;
      case "file":
        return side.path ? { kind: "file", path: side.path } : null;
      case "text":
        return text && text.value.trim() ? { kind: "text", text: text.value } : null;
    }
  }

  async function compare() {
    const l = toSource(left, leftText.current);
    const r = toSource(right, rightText.current);
    if (!l || !r) return toast("Choose something on both sides first.", "error");
    setBusy(true);
    setRevealed(null);
    try {
      setResult(await valueDiff(l, r));
      lastSources.current = { l, r };
    } catch (e) {
      toast(errMessage(e), "error");
    } finally {
      setBusy(false);
    }
  }

  async function reveal(all: boolean, keys: string[]) {
    const src = lastSources.current;
    if (!src || !result) return;
    try {
      const r = await valueDiff(src.l, src.r, all, keys);
      const values: Revealed["values"] = { ...(revealed?.values ?? {}) };
      for (const row of r.rows) {
        if (row.leftValue !== null || row.rightValue !== null) values[row.key] = { left: row.leftValue, right: row.rightValue };
      }
      setRevealed({ values, expires: Date.now() + r.revealSeconds * 1000 });
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }

  function swap() {
    const lt = leftText.current?.value ?? "";
    const rt = rightText.current?.value ?? "";
    setLeft(right);
    setRight(left);
    // Swap pasted text too (it lives in the DOM, not in React state).
    window.setTimeout(() => {
      if (leftText.current) leftText.current.value = rt;
      if (rightText.current) rightText.current.value = lt;
    });
  }

  const rows = useMemo(() => {
    if (!result) return [];
    const q = query.trim().toLowerCase();
    return result.rows.filter((r) => (!onlyDiffs || r.status !== "same") && (!q || r.key.toLowerCase().includes(q)));
  }, [result, onlyDiffs, query]);

  const diffKeys = result ? result.rows.filter((r) => r.status !== "same").map((r) => r.key) : [];
  const left_s = revealed ? Math.max(0, Math.ceil((revealed.expires - Date.now()) / 1000)) : 0;

  return (
    <div className="mx-auto flex max-w-[1200px] flex-col gap-4 p-5">
      <div>
        <h1 className="display text-[18px]">Diff checker</h1>
        <p className="text-[13px] text-muted">Compare real values between any two sources: a target, a specific version, a file on disk, or pasted text, across projects. Nothing is shown until you ask.</p>
      </div>

      <div className="grid grid-cols-1 items-start gap-3 lg:grid-cols-[1fr_auto_1fr]">
        <SourcePicker title="Left" side={left} onChange={setLeft} catalog={catalog} textRef={leftText} />
        <button className="btn self-center" onClick={swap} aria-label="Swap left and right" title="Swap sides">
          ⇄
        </button>
        <SourcePicker title="Right" side={right} onChange={setRight} catalog={catalog} textRef={rightText} />
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <button className="btn btn-primary" onClick={compare} disabled={busy}>
          {busy ? "Comparing…" : "Compare values"}
        </button>
        {result && (
          <>
            <button className="btn" onClick={() => void reveal(false, diffKeys)} disabled={diffKeys.length === 0}>
              Reveal differences ({diffKeys.length})
            </button>
            <button className="btn" onClick={() => void reveal(true, [])}>
              Reveal all
            </button>
            {revealed && (
              <>
                <button className="btn" onClick={hide}>
                  Hide values
                </button>
                <span className="text-[12px] text-muted" aria-live="off">
                  hides in {left_s} s
                </span>
              </>
            )}
          </>
        )}
      </div>

      {result && (
        <section className="panel" aria-label="Diff result">
          <div className="flex flex-wrap items-center gap-2 border-b border-line px-4 py-3 text-[12px]">
            <span className="mono truncate" title={result.leftLabel}>
              {shortLabel(result.leftLabel)}
            </span>
            <span className="text-muted">vs</span>
            <span className="mono truncate" title={result.rightLabel}>
              {shortLabel(result.rightLabel)}
            </span>
            <span className="ml-auto flex flex-wrap gap-2">
              <span className="chip tone-muted">= {result.counts.same} same</span>
              <span className="chip tone-staging">≠ {result.counts.changed} differ</span>
              <span className="chip tone-prod">○ {result.counts.removed} only left</span>
              <span className="chip tone-dev">+ {result.counts.added} only right</span>
            </span>
          </div>
          {(result.leftWarnings > 0 || result.rightWarnings > 0) && (
            <p className="border-b border-line px-4 py-2 text-[12px] tone-staging">
              The parser skipped {plural(result.leftWarnings + result.rightWarnings, "line")} it could not read. Import the file to see the warnings.
            </p>
          )}
          <div className="flex flex-wrap items-center gap-3 border-b border-line px-4 py-2">
            <label className="flex items-center gap-2 text-[13px]">
              <input type="checkbox" checked={onlyDiffs} onChange={(e) => setOnlyDiffs(e.target.checked)} /> Only differences
            </label>
            <input className="input max-w-[240px]" placeholder="Search keys" aria-label="Search keys" value={query} onChange={(e) => setQuery(e.target.value)} />
            <span className="ml-auto text-[12px] text-muted">{plural(rows.length, "key")} shown</span>
          </div>
          {rows.length === 0 ? (
            <p className="p-4 text-[13px] text-muted">{result.rows.length === 0 ? "Both sides are empty." : onlyDiffs ? "No differences. Every key has the same value on both sides." : "No keys match."}</p>
          ) : (
            <div className="overflow-x-auto">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>Key</th>
                    <th>Status</th>
                    <th>Left</th>
                    <th>Right</th>
                    <th>
                      <span className="sr-only">Actions</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((r) => (
                    <Row key={r.key} row={r} values={revealed?.values[r.key]} fading={left_s <= 1} onReveal={() => void reveal(false, [r.key])} />
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>
      )}
    </div>
  );
}

function shortLabel(label: string): string {
  return /[\\/]\.env/.test(label) ? baseName(label) : label;
}

function Row({ row, values, fading, onReveal }: { row: ValueRow; values?: { left: string | null; right: string | null }; fading: boolean; onReveal: () => void }) {
  const st = STATUS[row.status];
  const parts = values && values.left !== null && values.right !== null && row.status === "changed" ? charDiff(values.left, values.right) : null;
  return (
    <tr>
      <td className="mono align-top text-[13px]">
        {row.key}
        {row.hint && <div className="font-sans text-[12px] text-muted">{row.hint}</div>}
      </td>
      <td className="align-top whitespace-nowrap">
        <span className={`${st.tone} text-[12px]`}>
          <span aria-hidden>{st.symbol}</span> {st.label}
        </span>
      </td>
      <td className={`max-w-[380px] align-top ${fading && values ? "fade-out" : ""}`}>
        <ValueCell present={row.status !== "added"} empty={row.leftEmpty} value={values?.left ?? null} parts={parts} side="left" />
      </td>
      <td className={`max-w-[380px] align-top ${fading && values ? "fade-out" : ""}`}>
        <ValueCell present={row.status !== "removed"} empty={row.rightEmpty} value={values?.right ?? null} parts={parts} side="right" />
      </td>
      <td className="align-top">
        {!values && (
          <button className="btn btn-sm" onClick={onReveal}>
            Reveal
          </button>
        )}
      </td>
    </tr>
  );
}

function ValueCell({ present, empty, value, parts, side }: { present: boolean; empty: boolean | null; value: string | null; parts: ReturnType<typeof charDiff> | null; side: "left" | "right" }) {
  if (!present) return <span className="text-[12px] text-muted">not set</span>;
  if (value === null) return <span className="mono text-[12px] text-muted">{empty ? "∅ empty" : "••••••••"}</span>;
  if (value === "") return <span className="mono text-[12px] text-muted">∅ empty</span>;
  if (parts) {
    return (
      <span className="mono text-[12px] break-all whitespace-pre-wrap">
        {parts
          .filter((p) => p.type === "same" || p.type === (side === "left" ? "del" : "add"))
          .map((p, i) => (
            <span key={i} className={p.type === "del" ? "diff-del" : p.type === "add" ? "diff-add" : ""}>
              {visible(p.text)}
            </span>
          ))}
      </span>
    );
  }
  return <span className="mono text-[12px] break-all whitespace-pre-wrap">{visible(value)}</span>;
}

interface PickerProps {
  title: string;
  side: Side;
  onChange: (s: Side) => void;
  catalog: CatalogEntry[];
  textRef: React.RefObject<HTMLTextAreaElement | null>;
}

const MODES: { id: Mode; label: string }[] = [
  { id: "target", label: "Target" },
  { id: "snapshot", label: "Version" },
  { id: "file", label: "File" },
  { id: "text", label: "Paste" },
];

function SourcePicker({ title, side, onChange, catalog, textRef }: PickerProps) {
  const toast = useToast();
  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const project = catalog.find((c) => c.project.id === side.projectId);
  const target = project?.targets.find((t) => t.id === side.targetId);

  useEffect(() => {
    if (side.mode !== "snapshot" || side.targetId === null) return setSnapshots([]);
    snapshotList(side.targetId)
      .then((s) => {
        setSnapshots(s);
        if (!s.some((x) => x.id === side.snapshotId)) onChange({ ...side, snapshotId: s[0]?.id ?? null });
      })
      .catch((e) => toast(errMessage(e), "error"));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [side.mode, side.targetId]);

  const id = title.toLowerCase();
  return (
    <div className="panel flex flex-col gap-2 p-3">
      <div className="flex items-center gap-2">
        <h2 className="display text-[15px]">{title}</h2>
        <div role="tablist" aria-label={`${title} source`} className="ml-auto flex gap-1">
          {MODES.map((m) => (
            <button key={m.id} role="tab" aria-selected={side.mode === m.id} className={`btn btn-sm ${side.mode === m.id ? "border-focus" : "text-muted"}`} onClick={() => onChange({ ...side, mode: m.id })}>
              {m.label}
            </button>
          ))}
        </div>
      </div>

      {(side.mode === "target" || side.mode === "snapshot") && (
        <div className="grid grid-cols-2 gap-2">
          <select
            className="input"
            aria-label={`${title} project`}
            value={side.projectId ?? ""}
            onChange={(e) => {
              const pid = Number(e.target.value);
              const t = catalog.find((c) => c.project.id === pid)?.targets.find((x) => !x.isArchived);
              onChange({ ...side, projectId: pid, targetId: t?.id ?? null, snapshotId: null });
            }}
          >
            {catalog.length === 0 && <option value="">No projects</option>}
            {catalog.map((c) => (
              <option key={c.project.id} value={c.project.id}>
                {c.project.name}
              </option>
            ))}
          </select>
          <select className="input" aria-label={`${title} target`} value={side.targetId ?? ""} onChange={(e) => onChange({ ...side, targetId: Number(e.target.value), snapshotId: null })}>
            {project?.targets.map((t) => (
              <option key={t.id} value={t.id}>
                {targetName(t)} ({t.stage}){t.currentVersion ? ` v${t.currentVersion}` : " (empty)"}
              </option>
            ))}
          </select>
        </div>
      )}
      {side.mode === "snapshot" && (
        <select className="input" aria-label={`${title} version`} value={side.snapshotId ?? ""} onChange={(e) => onChange({ ...side, snapshotId: Number(e.target.value) })}>
          {snapshots.length === 0 && <option value="">No versions</option>}
          {snapshots.map((s) => (
            <option key={s.id} value={s.id}>
              v{s.version} · {formatDate(s.createdAt)} · {plural(s.keyCount, "key")}
              {s.note ? ` · ${s.note}` : ""}
            </option>
          ))}
        </select>
      )}
      {(side.mode === "target" || side.mode === "snapshot") && target && (
        <p className="flex items-center gap-2 text-[12px] text-muted">
          <StageBadge stage={target.stage} /> {target.currentVersion ? `current version v${target.currentVersion}, ${plural(target.keyCount, "key")}` : "nothing imported yet"}
        </p>
      )}
      {side.mode === "file" && (
        <div className="flex items-center gap-2">
          <button
            className="btn"
            onClick={async () => {
              const p = await pickFile();
              if (p) onChange({ ...side, path: p });
            }}
          >
            Choose .env file
          </button>
          <span className="mono truncate text-[12px] text-muted" title={side.path ?? undefined}>
            {side.path ? baseName(side.path) : "Compare your local file against a stored target"}
          </span>
        </div>
      )}
      <textarea
        ref={textRef}
        id={`paste-${id}`}
        aria-label={`${title} pasted text`}
        className={`input mono h-28 resize-y ${side.mode === "text" ? "" : "hidden"}`}
        spellCheck={false}
        placeholder={"KEY=value\nOTHER=value"}
        onChange={() => onChange({ ...side })}
      />
    </div>
  );
}
