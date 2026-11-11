// Open a .lokishare: file + pass key -> preview (names only) -> import into a target or save as .env.
// The decrypted values stay in the Rust core until you choose; closing discards them.
import { useEffect, useState } from "react";
import { baseName, formatDate, plural, targetName } from "../lib/format";
import { errMessage, inTauri, onFilesDropped, pickFile, shareDiscard, shareImport, shareOpen, shareSaveEnv, targetCatalog } from "../lib/ipc";
import type { CatalogEntry, SharePreview } from "../types";
import { Modal } from "./Modal";
import { useToast } from "./Toast";

interface Props {
  onClose: () => void;
  onImported: (projectId: number) => void;
}

export function OpenShareDialog({ onClose, onImported }: Props) {
  const toast = useToast();
  const [path, setPath] = useState<string | null>(null);
  const [passKey, setPassKey] = useState("");
  const [preview, setPreview] = useState<SharePreview | null>(null);
  const [catalog, setCatalog] = useState<CatalogEntry[]>([]);
  const [projectId, setProjectId] = useState<number | null>(null);
  const [targetId, setTargetId] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!inTauri()) return;
    let off: (() => void) | undefined;
    void onFilesDropped((p) => p[0] && setPath(p[0])).then((fn) => (off = fn));
    return () => off?.();
  }, []);

  useEffect(() => {
    if (!preview) return;
    targetCatalog()
      .then((c) => {
        setCatalog(c);
        const first = c.find((e) => e.targets.some((t) => !t.isArchived));
        setProjectId(first?.project.id ?? null);
        setTargetId(first?.targets.find((t) => !t.isArchived)?.id ?? null);
      })
      .catch(() => undefined);
  }, [preview]);

  async function choose() {
    const p = await pickFile();
    if (p) setPath(p);
  }

  async function open() {
    if (!path) return setError("Choose the .lokishare file first.");
    setBusy(true);
    setError(null);
    try {
      setPreview(await shareOpen(path, passKey));
      setPassKey("");
    } catch (e) {
      setError(errMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function doImport() {
    if (!preview || targetId === null) return;
    setBusy(true);
    try {
      const s = await shareImport(preview.token, targetId, null);
      toast(`Imported as version ${s.version} (${plural(s.keyCount, "key")})`, "success");
      onImported(projectId ?? 0);
    } catch (e) {
      setError(errMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function doSave() {
    if (!preview) return;
    setBusy(true);
    try {
      const p = await shareSaveEnv(preview.token);
      if (p) {
        toast(`Saved ${baseName(p)}`, "success");
        onClose();
      }
    } catch (e) {
      setError(errMessage(e));
    } finally {
      setBusy(false);
    }
  }

  function close() {
    setPassKey("");
    void shareDiscard().catch(() => undefined);
    onClose();
  }

  const project = catalog.find((c) => c.project.id === projectId);

  return (
    <Modal
      title="Open a share"
      onClose={close}
      width={600}
      footer={
        preview ? (
          <>
            <button className="btn" onClick={close} disabled={busy}>
              Discard
            </button>
            <button className="btn" onClick={doSave} disabled={busy || preview.expired}>
              Save as .env file
            </button>
            <button className="btn btn-primary" onClick={doImport} disabled={busy || preview.expired || targetId === null}>
              Import into target
            </button>
          </>
        ) : (
          <>
            <button className="btn" onClick={close} disabled={busy}>
              Cancel
            </button>
            <button className="btn btn-primary" onClick={open} disabled={busy || !path || passKey.trim().length < 25}>
              {busy ? "Decrypting…" : "Open"}
            </button>
          </>
        )
      }
    >
      {!preview ? (
        <div className="flex flex-col gap-3">
          <div className="flex items-center gap-2">
            <button className="btn" onClick={choose} disabled={busy}>
              Choose .lokishare file
            </button>
            <span className="mono truncate text-[12px] text-muted">{path ? baseName(path) : "or drop it on the window"}</span>
          </div>
          <div>
            <label className="label" htmlFor="pass-key">
              Pass key
            </label>
            <input
              id="pass-key"
              className="input mono text-[15px] tracking-wider"
              autoComplete="off"
              spellCheck={false}
              placeholder="XXXXX-XXXXX-XXXXX-XXXXX-XXXXX"
              value={passKey}
              onChange={(e) => setPassKey(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void open()}
            />
            <p className="mt-1 text-[12px] text-muted">Dashes, spaces and letter case do not matter.</p>
          </div>
        </div>
      ) : (
        <div className="flex flex-col gap-3 text-[13px]">
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
            <dt className="text-muted">From</dt>
            <dd className="mono">{preview.source}</dd>
            <dt className="text-muted">Created</dt>
            <dd>{formatDate(preview.createdAt)}</dd>
            <dt className="text-muted">Expires</dt>
            <dd className={preview.expired ? "text-prod" : ""}>{preview.expiresAt ? `${formatDate(preview.expiresAt)}${preview.expired ? " (expired)" : ""}` : "never"}</dd>
            {preview.note && (
              <>
                <dt className="text-muted">Note</dt>
                <dd>{preview.note}</dd>
              </>
            )}
          </dl>
          {preview.expired && <p className="text-prod">This share has expired. Ask the sender for a new one.</p>}
          <details open>
            <summary className="cursor-pointer">{plural(preview.keys.length, "key")} inside (values stay hidden)</summary>
            <ul className="mono mt-2 grid max-h-40 grid-cols-2 gap-x-4 overflow-auto text-[12px]">
              {preview.keys.map((k) => (
                <li key={k.key} className="truncate">
                  {k.key}
                  {k.isEmpty && <span className="text-muted"> ∅</span>}
                </li>
              ))}
            </ul>
          </details>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="label" htmlFor="imp-project">
                Project
              </label>
              <select
                id="imp-project"
                className="input"
                value={projectId ?? ""}
                onChange={(e) => {
                  const id = Number(e.target.value);
                  setProjectId(id);
                  setTargetId(catalog.find((c) => c.project.id === id)?.targets.find((t) => !t.isArchived)?.id ?? null);
                }}
              >
                {catalog.map((c) => (
                  <option key={c.project.id} value={c.project.id}>
                    {c.project.name}
                  </option>
                ))}
              </select>
            </div>
            <div>
              <label className="label" htmlFor="imp-target">
                Target (a new version is added)
              </label>
              <select id="imp-target" className="input" value={targetId ?? ""} onChange={(e) => setTargetId(Number(e.target.value))}>
                {project?.targets
                  .filter((t) => !t.isArchived)
                  .map((t) => (
                    <option key={t.id} value={t.id}>
                      {targetName(t)} ({t.stage})
                    </option>
                  ))}
              </select>
            </div>
          </div>
          {catalog.length === 0 && <p className="text-muted">No targets yet. Add a project and a target to import into, or save it as a .env file.</p>}
        </div>
      )}
      {error && (
        <p className="mt-3 text-[13px] text-prod" role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}
