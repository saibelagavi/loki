// Push-day check: pick "from" and "to" and get findings grouped by severity.
import { useEffect, useMemo, useState } from "react";
import { IconCheck, IconWarn } from "../components/icons";
import { StageBadge } from "../components/StageBadge";
import { useToast } from "../components/Toast";
import { baseName, plural, targetName } from "../lib/format";
import { errMessage, exportEnv, pushCheck, pushCheckExport } from "../lib/ipc";
import type { Finding, PushResult, Severity, Target } from "../types";
import type { PageProps } from "./Workspace";

const GROUPS: { severity: Severity; title: string; symbol: string; tone: string }[] = [
  { severity: "error", title: "Errors", symbol: "○", tone: "tone-prod" },
  { severity: "warning", title: "Warnings", symbol: "≠", tone: "tone-staging" },
  { severity: "info", title: "Info", symbol: "=", tone: "tone-muted" },
];

export default function PushCheck({ targets, goTo }: PageProps) {
  const toast = useToast();
  const candidates = useMemo(() => targets.filter((t) => !t.isArchived), [targets]);
  const [fromId, setFromId] = useState<number | null>(null);
  const [toId, setToId] = useState<number | null>(null);
  const [result, setResult] = useState<PushResult | null>(null);
  const [busy, setBusy] = useState(false);

  // Sensible defaults: staging to production.
  useEffect(() => {
    const prod = candidates.find((t) => t.stage === "production");
    const stage = candidates.find((t) => t.stage === "staging") ?? candidates.find((t) => t.id !== prod?.id);
    setToId((cur) => (cur !== null && candidates.some((t) => t.id === cur) ? cur : (prod?.id ?? candidates[0]?.id ?? null)));
    setFromId((cur) => (cur !== null && candidates.some((t) => t.id === cur) ? cur : (stage?.id ?? null)));
  }, [candidates]);

  useEffect(() => setResult(null), [fromId, toId]);

  const from = candidates.find((t) => t.id === fromId) ?? null;
  const to = candidates.find((t) => t.id === toId) ?? null;

  async function run() {
    if (fromId === null || toId === null) return;
    setBusy(true);
    try {
      setResult(await pushCheck(fromId, toId));
    } catch (e) {
      toast(errMessage(e), "error");
    } finally {
      setBusy(false);
    }
  }

  async function exportChecklist() {
    if (fromId === null || toId === null) return;
    try {
      const path = await pushCheckExport(fromId, toId);
      if (path) toast(`Saved ${baseName(path)} (keys only, no values)`, "success");
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }

  async function exportMissing() {
    if (!from?.currentSnapshotId || !to) return;
    try {
      const path = await exportEnv(from.currentSnapshotId, "missing_only", to.currentSnapshotId);
      if (path) toast(`Saved ${baseName(path)}: keys missing or empty in ${to.branch}`, "success");
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }

  if (candidates.length < 2) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
        <h2 className="text-[18px] font-semibold">Two targets needed</h2>
        <p className="max-w-[440px] text-[13px] text-muted">The push check compares a source (like staging) with a destination (like production). Add and import at least two targets.</p>
        <button className="btn btn-primary" onClick={() => goTo("targets")}>
          Go to targets
        </button>
      </div>
    );
  }

  const missingSnapshot = (t: Target | null) => t !== null && t.currentSnapshotId === null;

  return (
    <div className="mx-auto max-w-[900px] p-5">
      <div className="panel mb-4 flex flex-wrap items-end gap-4 p-4">
        <TargetSelect label="From" value={fromId} targets={candidates} onChange={setFromId} />
        <span aria-hidden className="pb-2 text-muted">
          →
        </span>
        <TargetSelect label="To" value={toId} targets={candidates} onChange={setToId} />
        <button className="btn btn-primary ml-auto" onClick={run} disabled={busy || fromId === null || toId === null || fromId === toId}>
          {busy ? "Checking…" : "Run push check"}
        </button>
      </div>
      {fromId !== null && fromId === toId && <p className="mb-3 text-[13px] text-prod">Pick two different targets.</p>}
      {(missingSnapshot(from) || missingSnapshot(to)) && <p className="mb-3 text-[13px] tone-staging">A target without an imported .env counts as having no keys, so everything will show as missing.</p>}

      {!result ? (
        <p className="text-[13px] text-muted">Choose where you are pushing from and to, then run the check. Rules can be switched off or extended in Settings.</p>
      ) : (
        <section aria-label="Findings" aria-live="polite">
          <div className="mb-3 flex flex-wrap items-center gap-2">
            <h2 className="text-[15px] font-semibold">
              {result.fromLabel} → {result.toLabel}
            </h2>
            <span className="ml-auto flex gap-2">
              <button className="btn" onClick={exportMissing} disabled={!from?.currentSnapshotId}>
                Export missing keys (.env)
              </button>
              <button className="btn" onClick={exportChecklist}>
                Export checklist (.md)
              </button>
            </span>
          </div>

          {result.findings.length === 0 ? (
            <div className="panel flex items-center gap-3 p-4">
              <span className="tone-dev">
                <IconCheck width={20} height={20} />
              </span>
              <div>
                <p className="font-medium">No findings</p>
                <p className="text-[13px] text-muted">Every key in the source exists in the destination and no rule fired.</p>
              </div>
            </div>
          ) : (
            GROUPS.map((g) => {
              const items = result.findings.filter((f) => f.severity === g.severity);
              if (items.length === 0) return null;
              return (
                <div key={g.severity} className="mb-4">
                  <h3 className={`mb-1 flex items-center gap-2 text-[13px] font-semibold ${g.tone}`}>
                    <span aria-hidden>{g.symbol}</span>
                    {g.title} ({items.length})
                  </h3>
                  <ul className="panel divide-y divide-line">
                    {items.map((f, i) => (
                      <FindingRow key={`${f.ruleId}-${f.key}-${i}`} finding={f} tone={g.tone} />
                    ))}
                  </ul>
                </div>
              );
            })
          )}
          <p className="mt-2 text-[12px] text-muted">
            {plural(result.findings.length, "finding")}. Findings name keys only, never values.
          </p>
        </section>
      )}
    </div>
  );
}

function FindingRow({ finding, tone }: { finding: Finding; tone: string }) {
  return (
    <li className="flex items-start gap-3 px-3 py-2">
      <span className={tone} aria-hidden>
        <IconWarn />
      </span>
      <div className="min-w-0">
        <p className="text-[13px]">{finding.message}</p>
        <p className="mono text-[12px] text-muted">{finding.ruleId}</p>
      </div>
    </li>
  );
}

function TargetSelect({ label, value, targets, onChange }: { label: string; value: number | null; targets: Target[]; onChange: (id: number) => void }) {
  const t = targets.find((x) => x.id === value);
  return (
    <div>
      <label className="label" htmlFor={`sel-${label}`}>
        {label}
      </label>
      <div className="flex items-center gap-2">
        <select id={`sel-${label}`} className="input w-auto min-w-[200px]" value={value ?? ""} onChange={(e) => onChange(Number(e.target.value))}>
          {targets.map((x) => (
            <option key={x.id} value={x.id}>
              {targetName(x)} ({x.stage})
            </option>
          ))}
        </select>
        {t && <StageBadge stage={t.stage} />}
      </div>
    </div>
  );
}
