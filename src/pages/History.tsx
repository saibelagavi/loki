// Versions: every import is an immutable snapshot. Diff any two, restore one, export one.
import { useCallback, useEffect, useMemo, useState } from "react";
import { Confirm } from "../components/Confirm";
import { RevealValue } from "../components/Reveal";
import { ShareDialog } from "../components/ShareDialog";
import { StageBadge } from "../components/StageBadge";
import { useToast } from "../components/Toast";
import { baseName, formatDate, plural, targetName } from "../lib/format";
import { errMessage, exportEnv, snapshotDiff, snapshotKeys, snapshotList, snapshotReimport, snapshotRestore } from "../lib/ipc";
import { useSettings } from "../lib/settingsContext";
import { useReveal } from "../lib/useReveal";
import type { DiffResult, ExportMode, KeyInfo, Snapshot } from "../types";
import type { PageProps } from "./Workspace";

export default function History({ targets, reloadTargets, goTo }: PageProps) {
  const { settings } = useSettings();
  const toast = useToast();
  const store = useReveal(settings.revealSeconds);
  const usable = useMemo(() => targets.filter((t) => t.snapshotCount > 0), [targets]);

  const [targetId, setTargetId] = useState<number | null>(null);
  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [picked, setPicked] = useState<number[]>([]);
  const [diff, setDiff] = useState<{ result: DiffResult; a: Snapshot; b: Snapshot; aKeys: Map<number, KeyInfo>; bKeys: Map<number, KeyInfo> } | null>(null);
  const [viewing, setViewing] = useState<{ snapshot: Snapshot; keys: KeyInfo[] } | null>(null);
  const [restoring, setRestoring] = useState<Snapshot | null>(null);
  const [sharing, setSharing] = useState<Snapshot | null>(null);

  useEffect(() => {
    setTargetId((cur) => (cur !== null && usable.some((t) => t.id === cur) ? cur : (usable[0]?.id ?? null)));
  }, [usable]);

  const load = useCallback(async () => {
    if (targetId === null) return setSnapshots([]);
    try {
      setSnapshots(await snapshotList(targetId));
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }, [targetId, toast]);

  useEffect(() => {
    setPicked([]);
    setDiff(null);
    setViewing(null);
    void load();
  }, [load]);

  // Compute the diff whenever exactly two snapshots are ticked (older -> newer).
  useEffect(() => {
    if (picked.length !== 2) return setDiff(null);
    const [a, b] = picked.map((id) => snapshots.find((s) => s.id === id)).filter((s): s is Snapshot => !!s).sort((x, y) => x.version - y.version);
    if (!a || !b) return setDiff(null);
    let cancelled = false;
    Promise.all([snapshotDiff(a.id, b.id), snapshotKeys(a.id), snapshotKeys(b.id)])
      .then(([result, ak, bk]) => !cancelled && setDiff({ result, a, b, aKeys: new Map(ak.map((k) => [k.variableId, k])), bKeys: new Map(bk.map((k) => [k.variableId, k])) }))
      .catch((e) => !cancelled && toast(errMessage(e), "error"));
    return () => {
      cancelled = true;
    };
  }, [picked, snapshots, toast]);

  const target = targets.find((t) => t.id === targetId) ?? null;

  async function reimport() {
    if (!target) return;
    try {
      const r = await snapshotReimport(target.id);
      if (r.unchanged) toast("The file has not changed since the last import (same SHA-256), so nothing was added.", "info");
      else toast(`Re-imported as version ${r.snapshot?.version}`, "success");
      await Promise.all([load(), reloadTargets()]);
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }

  async function doExport(s: Snapshot, mode: ExportMode) {
    try {
      const path = await exportEnv(s.id, mode);
      if (path) toast(`Saved ${baseName(path)}`, "success");
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }

  async function view(s: Snapshot) {
    if (viewing?.snapshot.id === s.id) return setViewing(null);
    try {
      setViewing({ snapshot: s, keys: await snapshotKeys(s.id) });
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }

  function togglePick(id: number) {
    setPicked((cur) => (cur.includes(id) ? cur.filter((x) => x !== id) : cur.length >= 2 ? [cur[1], id] : [...cur, id]));
  }

  if (usable.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
        <h2 className="text-[18px] font-semibold">No versions yet</h2>
        <p className="max-w-[440px] text-[13px] text-muted">Every import creates a version you can diff, restore and export. Import a .env file to start the history.</p>
        <button className="btn btn-primary" onClick={() => goTo("targets")}>
          Go to targets
        </button>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-[1000px] p-5">
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-2 text-[13px]">
          Target
          <select className="input w-auto" value={targetId ?? ""} onChange={(e) => setTargetId(Number(e.target.value))}>
            {usable.map((t) => (
              <option key={t.id} value={t.id}>
                {targetName(t)} ({t.stage})
              </option>
            ))}
          </select>
        </label>
        {target && <StageBadge stage={target.stage} />}
        <button className="btn ml-auto" onClick={reimport}>
          Re-import from file
        </button>
      </div>

      <p className="mb-2 text-[12px] text-muted">Tick two versions to see what changed between them. Snapshots are immutable: restoring one creates a new version.</p>

      <div className="panel overflow-x-auto">
        <table className="data-table">
          <thead>
            <tr>
              <th aria-label="Compare" />
              <th>Version</th>
              <th>Date</th>
              <th>Keys</th>
              <th>Note</th>
              <th>Source</th>
              <th>
                <span className="sr-only">Actions</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {snapshots.map((s, i) => (
              <tr key={s.id}>
                <td>
                  <input type="checkbox" checked={picked.includes(s.id)} onChange={() => togglePick(s.id)} aria-label={`Select version ${s.version} for diff`} />
                </td>
                <td className="font-medium tabular-nums">
                  v{s.version} {i === 0 && <span className="chip ml-1">current</span>}
                </td>
                <td className="whitespace-nowrap text-muted">{formatDate(s.createdAt)}</td>
                <td className="tabular-nums">{s.keyCount}</td>
                <td className="max-w-[100px] truncate" title={s.note ?? undefined}>{s.note ?? <span className="text-muted">-</span>}</td>
                <td className="mono max-w-[140px] truncate text-[12px] text-muted">{s.sourcePath ? baseName(s.sourcePath) : "pasted or restored"}</td>
                <td>
                  <div className="flex flex-wrap justify-end gap-1">
                    <button className="btn btn-sm" onClick={() => view(s)} aria-expanded={viewing?.snapshot.id === s.id}>
                      Keys
                    </button>
                    {i > 0 && (
                      <button className="btn btn-sm" onClick={() => setRestoring(s)}>
                        Restore
                      </button>
                    )}
                    <button className="btn btn-sm" onClick={() => doExport(s, "full")}>
                      Export .env
                    </button>
                    <button className="btn btn-sm" onClick={() => doExport(s, "example")}>
                      Export .env.example
                    </button>
                    <button className="btn btn-sm" onClick={() => setSharing(s)} title="Share this version securely">
                      Share
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {viewing && (
        <section className="panel mt-4 p-3" aria-label={`Keys in version ${viewing.snapshot.version}`}>
          <h3 className="mb-2 text-[13px] font-semibold">
            Version {viewing.snapshot.version}: {plural(viewing.keys.length, "key")}
          </h3>
          <ul className="max-h-72 divide-y divide-line overflow-auto">
            {viewing.keys.map((k) => (
              <li key={k.variableId} className="flex flex-wrap items-center gap-3 py-1.5">
                <span className="mono w-[240px] truncate text-[13px]">{k.key}</span>
                <span className="mono text-[12px] text-muted" title="Fingerprint: equal fingerprint means equal value">
                  {k.isEmpty ? "∅ empty" : `#${k.fingerprint}`}
                </span>
                <span className="ml-auto">
                  <RevealValue id={k.variableId} store={store} />
                </span>
              </li>
            ))}
          </ul>
        </section>
      )}

      {diff && (
        <section className="panel mt-4 p-3" aria-label="Diff">
          <h3 className="mb-1 text-[13px] font-semibold">
            Version {diff.a.version} to version {diff.b.version}
          </h3>
          <p className="mb-3 flex gap-2 text-[12px]">
            <span className="chip tone-dev">+ {diff.result.added} added</span>
            <span className="chip tone-prod">○ {diff.result.removed} removed</span>
            <span className="chip tone-staging">≠ {diff.result.changed} changed</span>
          </p>
          {diff.result.entries.length === 0 ? (
            <p className="text-[13px] text-muted">These two versions are identical.</p>
          ) : (
            <ul className="divide-y divide-line">
              {diff.result.entries.map((e) => (
                <li key={`${e.change}-${e.key}`} className="flex flex-wrap items-center gap-3 py-2">
                  <span aria-hidden className={e.change === "added" ? "tone-dev" : e.change === "removed" ? "tone-prod" : "tone-staging"}>
                    {e.change === "added" ? "+" : e.change === "removed" ? "○" : "≠"}
                  </span>
                  <span className="mono w-[240px] truncate text-[13px]">{e.key}</span>
                  <span className="w-[70px] text-[12px] text-muted">{e.change}</span>
                  {e.change === "changed" && (
                    <span className="mono text-[12px] text-muted">
                      #{diff.aKeys.get(e.aVariableId ?? -1)?.fingerprint} → #{diff.bKeys.get(e.bVariableId ?? -1)?.fingerprint}
                    </span>
                  )}
                  <span className="ml-auto flex flex-wrap items-center gap-3">
                    {e.aVariableId !== null && (
                      <span className="flex items-center gap-1.5 text-[12px]">
                        <span className="text-muted">v{diff.a.version}</span>
                        <RevealValue id={e.aVariableId} store={store} />
                      </span>
                    )}
                    {e.bVariableId !== null && (
                      <span className="flex items-center gap-1.5 text-[12px]">
                        <span className="text-muted">v{diff.b.version}</span>
                        <RevealValue id={e.bVariableId} store={store} />
                      </span>
                    )}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}

      {sharing && target && <ShareDialog preselect={{ snapshotId: sharing.id, label: `${target.branch} / ${target.stage} v${sharing.version}` }} onClose={() => setSharing(null)} />}
      {restoring && (
        <Confirm
          title={`Restore version ${restoring.version}`}
          confirmLabel="Restore as new version"
          noteLabel="Note (optional)"
          onClose={() => setRestoring(null)}
          onConfirm={async (note) => {
            try {
              const s = await snapshotRestore(restoring.id, note.trim() || null);
              toast(`Restored. Created version ${s.version}.`, "success");
              setRestoring(null);
              await Promise.all([load(), reloadTargets()]);
            } catch (e) {
              toast(errMessage(e), "error");
            }
          }}
        >
          <p>
            This creates a <strong>new version</strong> that copies version {restoring.version} ({plural(restoring.keyCount, "key")}). Nothing is overwritten, so you can always go back.
          </p>
        </Confirm>
      )}
    </div>
  );
}
