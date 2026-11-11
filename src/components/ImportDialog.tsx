// Import a .env into a target: pick a file, drop one on the window, or paste text.
// The preview shows key names, counts and parser warnings. Values are never sent back to the UI.
import { useEffect, useRef, useState } from "react";
import { errMessage, importCommit, importPreview, inTauri, onFilesDropped, pickFile } from "../lib/ipc";
import { baseName, plural } from "../lib/format";
import type { Preview, Target } from "../types";
import { Modal } from "./Modal";
import { StageBadge } from "./StageBadge";
import { useToast } from "./Toast";

interface Props {
  target: Target;
  onClose: () => void;
  onDone: () => void;
}

export function ImportDialog({ target, onClose, onDone }: Props) {
  const toast = useToast();
  const [mode, setMode] = useState<"file" | "paste">("file");
  const [path, setPath] = useState<string | null>(null);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dropping, setDropping] = useState(false);
  // Pasted text stays in the textarea (DOM), not in React state, and is cleared when done.
  const pasteRef = useRef<HTMLTextAreaElement>(null);

  const source = () => (mode === "file" ? (path ? { path } : null) : { text: pasteRef.current?.value ?? "" });

  async function runPreview(src: { path?: string; text?: string } | null) {
    if (!src || (src.text !== undefined && src.text.trim() === "")) {
      setPreview(null);
      return setError(mode === "paste" ? "Paste some KEY=value lines first." : "Choose a file first.");
    }
    setBusy(true);
    setError(null);
    try {
      setPreview(await importPreview(src));
    } catch (e) {
      setPreview(null);
      setError(errMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function choose() {
    const p = await pickFile();
    if (p) {
      setPath(p);
      void runPreview({ path: p });
    }
  }

  // Drag and drop a file anywhere on the window while this dialog is open.
  useEffect(() => {
    if (!inTauri()) return;
    let off: (() => void) | undefined;
    void onFilesDropped((paths) => {
      setDropping(false);
      if (paths[0]) {
        setMode("file");
        setPath(paths[0]);
        void runPreview({ path: paths[0] });
      }
    }).then((fn) => (off = fn));
    return () => off?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function commit() {
    const src = source();
    if (!src || !preview) return;
    setBusy(true);
    setError(null);
    try {
      const snap = await importCommit(target.id, src, note.trim() || null);
      if (pasteRef.current) pasteRef.current.value = "";
      toast(`Imported version ${snap.version} with ${plural(snap.keyCount, "key")}`, "success");
      onDone();
    } catch (e) {
      setError(errMessage(e));
    } finally {
      setBusy(false);
    }
  }

  function close() {
    if (pasteRef.current) pasteRef.current.value = "";
    onClose();
  }

  return (
    <Modal
      title={`Import .env into ${target.branch}`}
      onClose={close}
      width={640}
      footer={
        <>
          <button className="btn" onClick={close} disabled={busy}>
            Cancel
          </button>
          <button className="btn btn-primary" onClick={commit} disabled={busy || !preview || preview.keyCount === 0}>
            {preview ? `Import ${plural(preview.keyCount, "key")}` : "Import .env"}
          </button>
        </>
      }
    >
      <p className="mb-3 flex items-center gap-2 text-[13px] text-muted">
        Target: <StageBadge stage={target.stage} /> <span className="mono text-chalk">{target.branch}</span>
        {target.currentVersion !== null && <span>(currently version {target.currentVersion})</span>}
      </p>

      <div role="tablist" className="mb-3 flex gap-1">
        {(["file", "paste"] as const).map((m) => (
          <button
            key={m}
            role="tab"
            aria-selected={mode === m}
            className={`btn btn-sm ${mode === m ? "border-focus" : ""}`}
            onClick={() => {
              setMode(m);
              setPreview(null);
              setError(null);
            }}
          >
            {m === "file" ? "From a file" : "Paste text"}
          </button>
        ))}
      </div>

      {mode === "file" ? (
        <div
          className={`flex flex-col items-center gap-2 rounded-md border border-dashed p-5 text-center ${dropping ? "border-focus" : "border-line"}`}
          onDragEnter={() => setDropping(true)}
          onDragLeave={() => setDropping(false)}
        >
          <p className="text-[13px] text-muted">Drop a .env file here, or</p>
          <button className="btn" onClick={choose} disabled={busy}>
            Choose file
          </button>
          {path && <p className="mono max-w-full break-all text-[12px]">{path}</p>}
        </div>
      ) : (
        <div>
          <label htmlFor="paste" className="label">
            Paste the contents of a .env file
          </label>
          <textarea id="paste" ref={pasteRef} className="input mono h-40 resize-y" spellCheck={false} placeholder={"DATABASE_URL=...\nREDIS_URL=..."} onChange={() => setPreview(null)} />
          <button className="btn mt-2" onClick={() => void runPreview(source())} disabled={busy}>
            Preview keys
          </button>
        </div>
      )}

      {error && (
        <p className="mt-3 text-[13px] text-prod" role="alert">
          {error}
        </p>
      )}

      {preview && (
        <section className="mt-4" aria-label="Preview">
          <div className="mb-2 flex flex-wrap gap-2 text-[12px]">
            <span className="chip">{plural(preview.keyCount, "key")}</span>
            <span className="chip">{preview.emptyCount} empty</span>
            {preview.duplicateCount > 0 && <span className="chip tone-staging">{plural(preview.duplicateCount, "duplicate")}</span>}
            {preview.sourcePath && <span className="chip mono">{baseName(preview.sourcePath)}</span>}
          </div>

          {preview.warnings.length > 0 && (
            <ul className="mb-3 max-h-32 overflow-auto rounded-md border border-line p-2 text-[12px]" aria-label="Parser warnings">
              {preview.warnings.map((w, i) => (
                <li key={i} className="tone-staging">
                  ≠ {w.message}
                </li>
              ))}
            </ul>
          )}

          <details>
            <summary className="cursor-pointer text-[13px]">Show key names</summary>
            <ul className="mono mt-2 grid max-h-48 grid-cols-2 gap-x-4 overflow-auto text-[12px]">
              {preview.keys.map((k) => (
                <li key={k.key} className="truncate">
                  {k.key}
                  {k.isEmpty && <span className="text-muted"> ∅</span>}
                </li>
              ))}
            </ul>
          </details>

          <div className="mt-3">
            <label htmlFor="note" className="label">
              Note (optional)
            </label>
            <input id="note" className="input" value={note} onChange={(e) => setNote(e.target.value)} placeholder="Why this version?" />
          </div>
        </section>
      )}
    </Modal>
  );
}
