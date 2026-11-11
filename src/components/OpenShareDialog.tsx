// Open a .lokishare: choose the file, type the pass key, then decide per .env where it goes.
// Default: the same project and target it came from (created here if missing), so a new
// teammate can import everything in one click. Values stay in the Rust core until imported or
// saved; closing discards them.
import { useEffect, useMemo, useState } from "react";
import { baseName, formatDate, plural, targetName } from "../lib/format";
import { errMessage, inTauri, onFilesDropped, pickFile, shareDiscard, shareImport, shareOpen, shareSaveEnv, targetCatalog } from "../lib/ipc";
import type { CatalogEntry, SharePreview, SharePreviewItem } from "../types";
import { Modal } from "./Modal";
import { StageBadge } from "./StageBadge";
import { useToast } from "./Toast";

interface Props {
  onClose: () => void;
  /** After importing: the project to open. */
  onImported: (projectId: number | null) => void;
}

/** "auto" = matching project/target; "skip"; or a target id as a string. */
type Choice = "auto" | "skip" | string;

export function OpenShareDialog({ onClose, onImported }: Props) {
  const toast = useToast();
  const [path, setPath] = useState<string | null>(null);
  const [passKey, setPassKey] = useState("");
  const [preview, setPreview] = useState<SharePreview | null>(null);
  const [catalog, setCatalog] = useState<CatalogEntry[]>([]);
  const [choices, setChoices] = useState<Record<number, Choice>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!inTauri()) return;
    let off: (() => void) | undefined;
    void onFilesDropped((p) => {
      if (p[0]) {
        setPath(p[0]);
        setError(null);
      }
    }).then((fn) => (off = fn));
    return () => off?.();
  }, []);

  const findMatch = useMemo(
    () => (item: SharePreviewItem) => {
      const proj = catalog.find((c) => c.project.name === item.project);
      const target = proj?.targets.find((t) => t.branch === item.branch && t.stage === item.stage);
      return { proj, target };
    },
    [catalog],
  );

  async function choose() {
    const p = await pickFile();
    if (p) {
      setPath(p);
      setError(null);
    }
  }

  async function open() {
    if (!path) return setError("Choose the .lokishare file first.");
    setBusy(true);
    setError(null);
    try {
      const pv = await shareOpen(path, passKey);
      const cat = await targetCatalog();
      setCatalog(cat);
      // Default: the matching project/target when the share carries names; otherwise skip.
      setChoices(Object.fromEntries(pv.items.map((it) => [it.index, it.project && it.branch && it.stage ? "auto" : "skip"])));
      setPreview(pv);
      setPassKey("");
    } catch (e) {
      setError(errMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const chosen = preview ? preview.items.filter((it) => choices[it.index] && choices[it.index] !== "skip") : [];

  async function doImport() {
    if (!preview || chosen.length === 0) return;
    setBusy(true);
    setError(null);
    try {
      const mappings = chosen.map((it) => ({ index: it.index, targetId: choices[it.index] === "auto" ? null : Number(choices[it.index]) }));
      const snaps = await shareImport(preview.token, mappings, null);
      toast(`Imported ${plural(snaps.length, ".env file")} as new versions`, "success");
      const cat = await targetCatalog().catch(() => [] as CatalogEntry[]);
      const first = cat.find((c) => c.targets.some((t) => t.id === snaps[0]?.targetId));
      onImported(first?.project.id ?? null);
    } catch (e) {
      setError(errMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function save(index: number) {
    if (!preview) return;
    try {
      const p = await shareSaveEnv(preview.token, index);
      if (p) toast(`Saved ${baseName(p)}`, "success");
    } catch (e) {
      setError(errMessage(e));
    }
  }

  function close() {
    setPassKey("");
    void shareDiscard().catch(() => undefined);
    onClose();
  }

  return (
    <Modal
      title={preview ? "Import shared .env files" : "Open a share"}
      onClose={close}
      width={680}
      footer={
        preview ? (
          <>
            <button className="btn mr-auto" onClick={close} disabled={busy}>
              Discard
            </button>
            <button className="btn btn-primary" onClick={doImport} disabled={busy || preview.expired || chosen.length === 0}>
              {busy ? "Importing…" : chosen.length === 0 ? "Nothing to import" : `Import ${plural(chosen.length, ".env file")}`}
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
        <div className="flex flex-col gap-4">
          <p className="text-[13px] text-muted">Someone sent you a .lokishare file and, separately, its pass key. Nothing is imported until you choose.</p>
          <div>
            <p className="label">1. The share file</p>
            <div className="flex items-center gap-2">
              <button className="btn" onClick={choose} disabled={busy}>
                Choose .lokishare file
              </button>
              <span className="mono truncate text-[12px] text-muted">{path ? baseName(path) : "or drop it on this window"}</span>
            </div>
          </div>
          <div>
            <label className="label" htmlFor="pass-key">
              2. The pass key
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
          <p className="text-muted">
            {plural(preview.items.length, ".env file")} · shared {formatDate(preview.createdAt)} ·{" "}
            <span className={preview.expired ? "text-prod" : ""}>{preview.expiresAt ? `${preview.expired ? "expired" : "expires"} ${formatDate(preview.expiresAt)}` : "no expiry"}</span>
          </p>
          {preview.note && <p className="rounded-md border border-line px-3 py-2">“{preview.note}”</p>}
          {preview.expired && <p className="text-prod">This share has expired. Ask the sender for a new one.</p>}

          <ul className="flex max-h-[48vh] flex-col gap-2 overflow-auto" aria-label="Shared .env files">
            {preview.items.map((it) => {
              const { target } = findMatch(it);
              const choice = choices[it.index] ?? "skip";
              return (
                <li key={it.index} className="rounded-md border border-line p-3">
                  <div className="flex flex-wrap items-center gap-2">
                    {it.stage && <StageBadge stage={it.stage} />}
                    <span className="font-medium">{it.project ?? it.label}</span>
                    {it.branch && <span className="mono text-[12px]">{it.branch}</span>}
                    <span className="text-[12px] text-muted">{plural(it.keys.length, "key")}</span>
                    <button className="btn btn-sm ml-auto" onClick={() => void save(it.index)} disabled={preview.expired}>
                      Save as .env file
                    </button>
                  </div>
                  <div className="mt-2 flex items-center gap-2">
                    <label className="text-[12px] text-muted" htmlFor={`dest-${it.index}`}>
                      Import into
                    </label>
                    <select id={`dest-${it.index}`} className="input" value={choice} onChange={(e) => setChoices((c) => ({ ...c, [it.index]: e.target.value }))}>
                      {it.project && it.branch && it.stage && (
                        <option value="auto">
                          {it.project} / {it.branch} ({it.stage}) {target ? "· adds a new version" : "· new"}
                        </option>
                      )}
                      <option value="skip">Don't import</option>
                      {catalog.map((c) => (
                        <optgroup key={c.project.id} label={c.project.name}>
                          {c.targets
                            .filter((t) => !t.isArchived)
                            .map((t) => (
                              <option key={t.id} value={String(t.id)}>
                                {targetName(t)} ({t.stage})
                              </option>
                            ))}
                        </optgroup>
                      ))}
                    </select>
                  </div>
                  <details className="mt-2">
                    <summary className="cursor-pointer text-[12px] text-muted">Key names</summary>
                    <ul className="mono mt-1 grid grid-cols-2 gap-x-4 text-[12px]">
                      {it.keys.map((k) => (
                        <li key={k.key} className="truncate">
                          {k.key}
                          {k.isEmpty && <span className="text-muted"> ∅</span>}
                        </li>
                      ))}
                    </ul>
                  </details>
                </li>
              );
            })}
          </ul>
          <p className="text-[12px] text-muted">Importing adds a new version; nothing you already have is overwritten. Values stay hidden until you reveal them in LOKI.</p>
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
