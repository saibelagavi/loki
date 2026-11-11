// Scan a folder for .env* files, guess stage and branch, and import the ones you tick.
import { useCallback, useEffect, useState } from "react";
import { errMessage, importCommit, pickFolder, scanFolder, targetCreate } from "../lib/ipc";
import { plural, relativeTo } from "../lib/format";
import { STAGES, type Project, type ScanHit, type Stage, type Target } from "../types";
import { IconFolder } from "./icons";
import { Modal } from "./Modal";
import { useToast } from "./Toast";

interface Row extends ScanHit {
  selected: boolean;
  stage: Stage;
  branch: string;
}

interface Props {
  project: Project;
  targets: Target[];
  onClose: () => void;
  onDone: () => void;
}

export function ScanDialog({ project, targets, onClose, onDone }: Props) {
  const toast = useToast();
  const [folder, setFolder] = useState<string | null>(project.rootPath);
  const [rows, setRows] = useState<Row[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [results, setResults] = useState<string[]>([]);

  const scan = useCallback(async (dir: string) => {
    setBusy(true);
    setError(null);
    setResults([]);
    try {
      const hits = await scanFolder(dir);
      setRows(hits.map((h) => ({ ...h, selected: h.guessedStage !== "other", stage: h.guessedStage, branch: h.guessedBranch ?? "main" })));
    } catch (e) {
      setRows(null);
      setError(errMessage(e));
    } finally {
      setBusy(false);
    }
  }, []);

  // If the project already has a folder, scan it straight away.
  useEffect(() => {
    if (project.rootPath) void scan(project.rootPath);
  }, [project.rootPath, scan]);

  async function choose() {
    const p = await pickFolder();
    if (p) {
      setFolder(p);
      void scan(p);
    }
  }

  const update = (i: number, patch: Partial<Row>) => setRows((cur) => cur?.map((r, j) => (j === i ? { ...r, ...patch } : r)) ?? null);
  const chosen = rows?.filter((r) => r.selected) ?? [];
  const invalid = chosen.some((r) => !r.branch.trim());

  async function importChosen() {
    setBusy(true);
    setError(null);
    const known = [...targets];
    const out: string[] = [];
    let ok = 0;
    for (const r of chosen) {
      try {
        let t = known.find((k) => k.branch === r.branch.trim() && k.stage === r.stage);
        if (!t) {
          t = await targetCreate(project.id, r.branch.trim(), r.stage, null);
          known.push(t);
        }
        const snap = await importCommit(t.id, { path: r.path }, "Imported from folder scan");
        out.push(`${r.fileName}: version ${snap.version}, ${plural(snap.keyCount, "key")}`);
        ok += 1;
      } catch (e) {
        out.push(`${r.fileName}: ${errMessage(e)}`);
      }
    }
    setResults(out);
    setBusy(false);
    if (ok > 0) {
      toast(`Imported ${plural(ok, "file")}`, "success");
      onDone();
    }
  }

  return (
    <Modal
      title="Scan folder for .env files"
      onClose={onClose}
      width={760}
      footer={
        <>
          <button className="btn" onClick={onClose} disabled={busy}>
            Close
          </button>
          <button className="btn btn-primary" onClick={importChosen} disabled={busy || chosen.length === 0 || invalid}>
            Import {plural(chosen.length, "file")}
          </button>
        </>
      }
    >
      <div className="mb-3 flex items-center gap-2">
        <button className="btn" onClick={choose} disabled={busy}>
          <IconFolder /> {folder ? "Choose another folder" : "Choose folder"}
        </button>
        {folder && <span className="mono truncate text-[12px] text-muted">{folder}</span>}
      </div>
      <p className="mb-3 text-[12px] text-muted">Looks for .env and .env.* and skips node_modules, .git, target, dist and venv. Stage comes from the file name; branch comes from .git/HEAD.</p>

      {busy && !rows && <p className="text-[13px] text-muted">Scanning…</p>}
      {error && (
        <p className="text-[13px] text-prod" role="alert">
          {error}
        </p>
      )}
      {rows && rows.length === 0 && <p className="text-[13px]">No .env files found in that folder.</p>}

      {rows && rows.length > 0 && (
        <table className="data-table">
          <thead>
            <tr>
              <th aria-label="Import" />
              <th>File</th>
              <th>Stage</th>
              <th>Branch</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r, i) => (
              <tr key={r.path}>
                <td>
                  <input type="checkbox" checked={r.selected} onChange={(e) => update(i, { selected: e.target.checked })} aria-label={`Import ${r.fileName}`} />
                </td>
                <td className="mono text-[12px]">{relativeTo(r.path, folder)}</td>
                <td>
                  <select className="input" value={r.stage} onChange={(e) => update(i, { stage: e.target.value as Stage })} aria-label="Stage">
                    {STAGES.map((s) => (
                      <option key={s}>{s}</option>
                    ))}
                  </select>
                </td>
                <td>
                  <input className="input mono" value={r.branch} onChange={(e) => update(i, { branch: e.target.value })} aria-label="Branch" />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {results.length > 0 && (
        <ul className="mt-3 rounded-md border border-line p-2 text-[12px]" aria-label="Import results">
          {results.map((r, i) => (
            <li key={i}>{r}</li>
          ))}
        </ul>
      )}
    </Modal>
  );
}
