// Push-day overview across every project: is each one ready to ship, which .env files on disk
// changed since their last import, and which secrets are overdue for rotation.
import { useCallback, useEffect, useState } from "react";
import { IconCheck, IconWarn } from "../components/icons";
import { StageBadge } from "../components/StageBadge";
import { useToast } from "../components/Toast";
import { baseName, formatDate, plural } from "../lib/format";
import { driftCheck, errMessage, overview, snapshotReimport } from "../lib/ipc";
import type { DriftRow, Overview as OverviewData, ProjectHealth } from "../types";
import type { Tab } from "./Workspace";

interface Props {
  onOpenProject: (id: number, tab: Tab) => void;
  onOpenPassword: (id: number) => void;
}

const DRIFT_LABEL: Record<DriftRow["status"], { text: string; tone: string; symbol: string }> = {
  changed: { text: "changed on disk", tone: "tone-staging", symbol: "≠" },
  missing: { text: "file not found", tone: "tone-prod", symbol: "○" },
  unreadable: { text: "cannot read", tone: "tone-prod", symbol: "○" },
  unchanged: { text: "in sync", tone: "tone-muted", symbol: "=" },
};

export default function Overview({ onOpenProject, onOpenPassword }: Props) {
  const toast = useToast();
  const [data, setData] = useState<OverviewData | null>(null);
  const [drift, setDrift] = useState<DriftRow[] | null>(null);
  const [checking, setChecking] = useState(false);

  const load = useCallback(async () => {
    try {
      setData(await overview());
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }, [toast]);

  useEffect(() => {
    void load();
  }, [load]);

  async function runDrift() {
    setChecking(true);
    try {
      setDrift(await driftCheck());
    } catch (e) {
      toast(errMessage(e), "error");
    } finally {
      setChecking(false);
    }
  }

  async function reimport(row: DriftRow) {
    try {
      const r = await snapshotReimport(row.targetId);
      toast(r.unchanged ? "Already up to date." : `${row.projectName} / ${row.branch}: imported version ${r.snapshot?.version}`, "success");
      await Promise.all([runDrift(), load()]);
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }

  if (!data) return <p className="p-6 text-[13px] text-muted">Loading…</p>;

  const drifted = drift?.filter((d) => d.status !== "unchanged") ?? [];
  const blocking = data.projects.filter((p) => (p.check?.errors ?? 0) > 0).length;

  return (
    <div className="mx-auto max-w-[1100px] p-5">
      <div className="mb-4">
        <h1 className="display text-[24px]">Push-day overview</h1>
        <p className="text-[13px] text-muted">Every project at a glance: what blocks a release, which local files drifted, and which secrets are overdue for rotation.</p>
      </div>

      <div className="mb-6 grid grid-cols-2 gap-3 sm:grid-cols-4">
        <Tile label="Projects" value={data.totals.projects} />
        <Tile label="Targets" value={data.totals.targets} />
        <Tile label="Keys tracked" value={data.totals.keys} />
        <Tile label="Passwords" value={data.totals.passwords} note={data.totals.weakPasswords + data.totals.reusedPasswords > 0 ? `${data.totals.weakPasswords} weak, ${data.totals.reusedPasswords} reused` : undefined} />
      </div>

      <section className="mb-8" aria-labelledby="ready-h">
        <div className="mb-2 flex items-end gap-3">
          <div>
            <h2 id="ready-h" className="display text-[18px]">
              Release readiness
            </h2>
            <p className="text-[12px] text-muted">The push check from staging (or the closest stage) to production, for every project. Values never leave the core.</p>
          </div>
          <button className="btn ml-auto" onClick={load}>
            Refresh
          </button>
        </div>
        {data.projects.length === 0 ? (
          <p className="panel p-4 text-[13px] text-muted">Add a project, then import its .env files.</p>
        ) : (
          <>
            {blocking > 0 && (
              <p className="mb-2 flex items-center gap-2 text-[13px] tone-prod">
                <IconWarn /> {plural(blocking, "project")} would ship with keys missing in production.
              </p>
            )}
            <div className="panel overflow-x-auto">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>Project</th>
                    <th>Stages</th>
                    <th>Check</th>
                    <th>Findings</th>
                    <th>Last import</th>
                    <th>
                      <span className="sr-only">Actions</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {data.projects.map((p) => (
                    <ReadinessRow key={p.id} p={p} onOpen={onOpenProject} />
                  ))}
                </tbody>
              </table>
            </div>
          </>
        )}
      </section>

      <section className="mb-8" aria-labelledby="drift-h">
        <div className="mb-2 flex items-end gap-3">
          <div>
            <h2 id="drift-h" className="display text-[18px]">
              Drift
            </h2>
            <p className="text-[12px] text-muted">Re-reads every imported .env file on disk and compares its SHA-256 with the stored version. Only runs when you click.</p>
          </div>
          <button className="btn btn-primary ml-auto" onClick={runDrift} disabled={checking}>
            {checking ? "Checking files…" : drift ? "Check again" : "Check files on disk"}
          </button>
        </div>
        {drift &&
          (drift.length === 0 ? (
            <p className="panel p-4 text-[13px] text-muted">No target was imported from a file yet, so there is nothing to compare. Imports from Scan folder or a file are tracked.</p>
          ) : (
            <div className="panel">
              {drifted.length === 0 ? (
                <p className="flex items-center gap-2 p-4 text-[13px]">
                  <span className="tone-dev">
                    <IconCheck />
                  </span>
                  All {plural(drift.length, "file")} match what LOKI has stored.
                </p>
              ) : (
                <ul className="divide-y divide-line">
                  {drifted.map((d) => (
                    <li key={d.targetId} className="flex flex-wrap items-center gap-3 px-3 py-2 text-[13px]">
                      <span className={DRIFT_LABEL[d.status].tone} aria-hidden>
                        {DRIFT_LABEL[d.status].symbol}
                      </span>
                      <span className="w-[160px] truncate font-medium">{d.projectName}</span>
                      <span className="mono w-[120px] truncate text-[12px]">{d.branch}</span>
                      <StageBadge stage={d.stage} />
                      <span className="mono min-w-0 flex-1 truncate text-[12px] text-muted" title={d.path}>
                        {baseName(d.path)}
                      </span>
                      <span className={`text-[12px] ${DRIFT_LABEL[d.status].tone}`}>{DRIFT_LABEL[d.status].text}</span>
                      {d.status === "changed" && (
                        <button className="btn btn-sm btn-primary" onClick={() => void reimport(d)}>
                          Re-import
                        </button>
                      )}
                      <button className="btn btn-sm" onClick={() => onOpenProject(d.projectId, "history")}>
                        History
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              {drifted.length > 0 && <p className="border-t border-line px-3 py-2 text-[12px] text-muted">{drift.length - drifted.length} other files are in sync.</p>}
            </div>
          ))}
      </section>

      <section aria-labelledby="rot-h">
        <h2 id="rot-h" className="display text-[18px]">
          Rotation reminders
        </h2>
        <p className="mb-2 text-[12px] text-muted">
          Secrets with a last-rotated date in their key note, and saved passwords, older than {data.rotationDays} days (change this in Settings).
        </p>
        {data.rotation.length === 0 ? (
          <p className="panel p-4 text-[13px] text-muted">Nothing is overdue. Set a last-rotated date on a key note to track a secret here.</p>
        ) : (
          <ul className="panel divide-y divide-line">
            {data.rotation.map((r, i) => (
              <li key={`${r.kind}-${r.name}-${i}`} className="flex flex-wrap items-center gap-3 px-3 py-2 text-[13px]">
                <span className="chip">{r.kind === "key" ? "secret key" : "password"}</span>
                <span className={r.kind === "key" ? "mono" : "font-medium"}>{r.name}</span>
                {r.projectName && <span className="text-muted">{r.projectName}</span>}
                <span className="ml-auto tone-staging">{r.ageDays} days since {formatDate(r.lastRotated)}</span>
                {r.kind === "password" && r.credentialId !== null ? (
                  <button className="btn btn-sm" onClick={() => onOpenPassword(r.credentialId as number)}>
                    Open
                  </button>
                ) : (
                  r.projectId !== null && (
                    <button className="btn btn-sm" onClick={() => onOpenProject(r.projectId as number, "targets")}>
                      Key notes
                    </button>
                  )
                )}
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function Tile({ label, value, note }: { label: string; value: number; note?: string }) {
  return (
    <div className="panel px-4 py-3">
      <p className="text-[12px] text-muted">{label}</p>
      <p className="display text-[24px] tabular-nums">{value}</p>
      {note && <p className="text-[12px] tone-staging">{note}</p>}
    </div>
  );
}

function ReadinessRow({ p, onOpen }: { p: ProjectHealth; onOpen: (id: number, tab: Tab) => void }) {
  const c = p.check;
  return (
    <tr>
      <td className="font-medium">
        <button className="btn btn-quiet btn-sm font-medium" onClick={() => onOpen(p.id, "compare")}>
          {p.name}
        </button>
      </td>
      <td>
        <span className="flex flex-wrap gap-2">
          {p.stages.length === 0 ? <span className="text-[12px] text-muted">no targets</span> : p.stages.map((s) => <StageBadge key={s} stage={s} />)}
        </span>
      </td>
      <td className="text-[12px] text-muted">{c ? `${c.fromLabel} → ${c.toLabel}` : p.checkError ?? "needs a production target and one more stage with a .env"}</td>
      <td>
        {c ? (
          c.errors + c.warnings === 0 ? (
            <span className="flex items-center gap-1 text-[12px] tone-dev">
              <IconCheck /> ready
            </span>
          ) : (
            <span className="flex flex-wrap gap-1 text-[12px]">
              {c.errors > 0 && <span className="chip tone-prod">{plural(c.errors, "error")}</span>}
              {c.warnings > 0 && <span className="chip tone-staging">{plural(c.warnings, "warning")}</span>}
              {c.info > 0 && <span className="chip">{c.info} info</span>}
            </span>
          )
        ) : (
          <span className="text-[12px] text-muted">-</span>
        )}
      </td>
      <td className="text-[12px] text-muted">{p.daysSinceImport === null ? "never" : p.daysSinceImport === 0 ? "today" : `${p.daysSinceImport} days ago`}</td>
      <td>
        {c && (
          <button className="btn btn-sm" onClick={() => onOpen(p.id, "push")}>
            Push check
          </button>
        )}
      </td>
    </tr>
  );
}
