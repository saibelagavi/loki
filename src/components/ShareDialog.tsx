// Share .env files securely, in three steps:
//   1. Choose: whole projects, single .env files, or only some keys of a file.
//   2. Pass key: LOKI generates it and shows it here first, so it can be copied before the file
//      exists. Creating the file needs a tick in "I copied the pass key".
//   3. Send: the file one way, the pass key another way.
// The pass key lives only in this dialog's state and is dropped when the dialog closes.
import { useEffect, useMemo, useState } from "react";
import { baseName, formatDate, plural } from "../lib/format";
import { clipboardCopyText, errMessage, shareCreate, shareGenerateKey, snapshotKeys, targetCatalog } from "../lib/ipc";
import type { CatalogEntry, KeyInfo, ShareCreated, Stage, Target } from "../types";
import { IconCheck, IconCopy } from "./icons";
import { Modal } from "./Modal";
import { StageBadge } from "./StageBadge";
import { useToast } from "./Toast";

interface Props {
  /** Start with this version ticked (from a target row or a version in History). */
  preselect?: { snapshotId: number; label: string };
  onClose: () => void;
}

interface Pick {
  snapshotId: number;
  label: string;
  stage: Stage;
  keyCount: number;
  /** null = every key */
  keys: Set<string> | null;
}

const EXPIRY = [
  { days: 1, label: "1 day" },
  { days: 7, label: "7 days" },
  { days: 30, label: "30 days" },
  { days: 0, label: "Never" },
];

type Step = 1 | 2 | 3;

export function ShareDialog({ preselect, onClose }: Props) {
  const toast = useToast();
  const [step, setStep] = useState<Step>(1);
  const [catalog, setCatalog] = useState<CatalogEntry[] | null>(null);
  const [picks, setPicks] = useState<Map<number, Pick>>(new Map());
  const [filter, setFilter] = useState("");
  const [keyEditor, setKeyEditor] = useState<{ snapshotId: number; keys: KeyInfo[] } | null>(null);
  const [passKey, setPassKey] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [expiry, setExpiry] = useState(7);
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [created, setCreated] = useState<ShareCreated | null>(null);

  useEffect(() => {
    targetCatalog()
      .then((c) => {
        setCatalog(c);
        if (!preselect) return;
        // Find the target for the preselected version, if it is a current version.
        const t = c.flatMap((e) => e.targets).find((x) => x.currentSnapshotId === preselect.snapshotId);
        setPicks(new Map([[preselect.snapshotId, { snapshotId: preselect.snapshotId, label: preselect.label, stage: t?.stage ?? "other", keyCount: t?.keyCount ?? 0, keys: null }]]));
        if (!t) {
          // An older version: count its keys so the summary is right.
          void snapshotKeys(preselect.snapshotId).then((k) =>
            setPicks((cur) => {
              const p = cur.get(preselect.snapshotId);
              return p ? new Map(cur).set(preselect.snapshotId, { ...p, keyCount: k.length }) : cur;
            }),
          );
        }
      })
      .catch((e) => toast(errMessage(e), "error"));
  }, [preselect, toast]);

  // Projects that have at least one .env to share.
  const projects = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return (catalog ?? [])
      .map((e) => ({ project: e.project, targets: e.targets.filter((t) => !t.isArchived && t.currentSnapshotId !== null) }))
      .filter((e) => e.targets.length > 0)
      .filter((e) => !q || e.project.name.toLowerCase().includes(q) || e.targets.some((t) => `${t.branch} ${t.stage} ${t.label ?? ""}`.toLowerCase().includes(q)));
  }, [catalog, filter]);

  // A preselected older version is not in the catalog's current versions; list it on its own.
  const extraVersion = preselect && catalog && !catalog.flatMap((e) => e.targets).some((t) => t.currentSnapshotId === preselect.snapshotId) ? preselect : null;

  const pickOf = (t: Target): Pick => ({
    snapshotId: t.currentSnapshotId as number,
    label: `${t.branch} / ${t.stage} v${t.currentVersion}`,
    stage: t.stage,
    keyCount: t.keyCount,
    keys: null,
  });

  function toggleTarget(t: Target) {
    const id = t.currentSnapshotId as number;
    setPicks((cur) => {
      const n = new Map(cur);
      if (n.has(id)) n.delete(id);
      else n.set(id, pickOf(t));
      return n;
    });
  }

  function toggleProject(targets: Target[]) {
    const ids = targets.map((t) => t.currentSnapshotId as number);
    const all = ids.every((id) => picks.has(id));
    setPicks((cur) => {
      const n = new Map(cur);
      for (const t of targets) {
        const id = t.currentSnapshotId as number;
        if (all) n.delete(id);
        else if (!n.has(id)) n.set(id, pickOf(t));
      }
      return n;
    });
  }

  async function openKeys(snapshotId: number) {
    if (keyEditor?.snapshotId === snapshotId) return setKeyEditor(null);
    try {
      setKeyEditor({ snapshotId, keys: await snapshotKeys(snapshotId) });
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }

  function toggleKey(snapshotId: number, key: string, all: string[]) {
    setPicks((cur) => {
      const n = new Map(cur);
      const p = n.get(snapshotId);
      if (!p) return cur;
      const keys = new Set(p.keys ?? all);
      if (keys.has(key)) keys.delete(key);
      else keys.add(key);
      n.set(snapshotId, { ...p, keys: keys.size === all.length ? null : keys });
      return n;
    });
  }

  const totalKeys = [...picks.values()].reduce((sum, p) => sum + (p.keys ? p.keys.size : p.keyCount), 0);
  const emptyPick = [...picks.values()].find((p) => p.keys !== null && p.keys.size === 0);

  async function newKey() {
    try {
      setPassKey(await shareGenerateKey());
      setCopied(false);
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }

  async function goToKey() {
    if (!passKey) await newKey();
    setStep(2);
  }

  async function copyKey() {
    if (!passKey) return;
    try {
      const s = await clipboardCopyText(passKey);
      setCopied(true);
      toast(`Pass key copied, clears from the clipboard in ${s} s`, "success");
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }

  async function create() {
    if (!passKey) return;
    setBusy(true);
    try {
      const selections = [...picks.values()].map((p) => ({ snapshotId: p.snapshotId, keys: p.keys ? [...p.keys] : null }));
      const r = await shareCreate(selections, passKey, expiry || null, note.trim() || null);
      if (r) {
        setCreated(r);
        setStep(3);
      }
    } catch (e) {
      toast(errMessage(e), "error");
    } finally {
      setBusy(false);
    }
  }

  function close() {
    setPassKey(null); // drop the pass key from memory
    onClose();
  }

  const footer =
    step === 1 ? (
      <>
        <span className="mr-auto self-center text-[12px] text-muted" aria-live="polite">
          {picks.size === 0 ? "Nothing selected yet" : `${plural(picks.size, ".env file")} · ${plural(totalKeys, "key")}`}
        </span>
        <button className="btn" onClick={close}>
          Cancel
        </button>
        <button className="btn btn-primary" onClick={goToKey} disabled={picks.size === 0 || !!emptyPick}>
          Next: pass key
        </button>
      </>
    ) : step === 2 ? (
      <>
        <button className="btn mr-auto" onClick={() => setStep(1)} disabled={busy}>
          Back
        </button>
        <button className="btn" onClick={close} disabled={busy}>
          Cancel
        </button>
        <button className="btn btn-primary" onClick={create} disabled={busy || !copied || !passKey}>
          {busy ? "Encrypting…" : "Create share file"}
        </button>
      </>
    ) : (
      <button className="btn btn-primary" onClick={close}>
        Done
      </button>
    );

  return (
    <Modal title="Share .env files securely" onClose={close} width={680} footer={footer}>
      <Stepper step={step} />

      {step === 1 && (
        <div className="flex flex-col gap-3">
          <p className="text-[13px] text-muted">Pick whole projects or single .env files. Every file goes into one encrypted share file, readable only with its pass key.</p>
          <input className="input" placeholder="Filter projects and targets" aria-label="Filter projects and targets" value={filter} onChange={(e) => setFilter(e.target.value)} />

          {catalog === null ? (
            <p className="text-[13px] text-muted">Loading…</p>
          ) : projects.length === 0 && !extraVersion ? (
            <p className="text-[13px] text-muted">Nothing to share yet. Import a .env file into a target first.</p>
          ) : (
            <ul className="max-h-[46vh] overflow-auto rounded-md border border-line" aria-label="Projects and .env files">
              {extraVersion && (
                <li className="border-b border-line px-3 py-2">
                  <p className="mb-1 text-[12px] text-muted">Selected version</p>
                  <EnvRow
                    checked={picks.has(extraVersion.snapshotId)}
                    onToggle={() =>
                      setPicks((cur) => {
                        const n = new Map(cur);
                        if (n.has(extraVersion.snapshotId)) n.delete(extraVersion.snapshotId);
                        else n.set(extraVersion.snapshotId, { snapshotId: extraVersion.snapshotId, label: extraVersion.label, stage: "other", keyCount: 0, keys: null });
                        return n;
                      })
                    }
                    title={<span className="mono">{extraVersion.label}</span>}
                    pick={picks.get(extraVersion.snapshotId)}
                    editorOpen={keyEditor?.snapshotId === extraVersion.snapshotId}
                    onEditKeys={() => void openKeys(extraVersion.snapshotId)}
                    editorKeys={keyEditor?.snapshotId === extraVersion.snapshotId ? keyEditor.keys : null}
                    onToggleKey={(k, all) => toggleKey(extraVersion.snapshotId, k, all)}
                    onAllKeys={(on, all) => setPicks((cur) => new Map(cur).set(extraVersion.snapshotId, { ...(cur.get(extraVersion.snapshotId) as Pick), keyCount: all.length, keys: on ? null : new Set() }))}
                  />
                </li>
              )}
              {projects.map(({ project, targets }) => {
                const selected = targets.filter((t) => picks.has(t.currentSnapshotId as number)).length;
                return (
                  <li key={project.id} className="border-b border-line px-3 py-2 last:border-b-0">
                    <label className="flex items-center gap-2 py-1 text-[13px] font-medium">
                      <input
                        type="checkbox"
                        checked={selected === targets.length}
                        ref={(el) => {
                          if (el) el.indeterminate = selected > 0 && selected < targets.length;
                        }}
                        onChange={() => toggleProject(targets)}
                      />
                      {project.name}
                      <span className="font-normal text-muted">· {plural(targets.length, ".env file")}</span>
                    </label>
                    <ul className="ml-6">
                      {targets.map((t) => {
                        const id = t.currentSnapshotId as number;
                        return (
                          <li key={t.id}>
                            <EnvRow
                              checked={picks.has(id)}
                              onToggle={() => toggleTarget(t)}
                              title={
                                <>
                                  <StageBadge stage={t.stage} />
                                  <span className="mono">{t.branch}</span>
                                  {t.label && <span className="text-muted">{t.label}</span>}
                                  <span className="text-muted">
                                    v{t.currentVersion} · {plural(t.keyCount, "key")}
                                  </span>
                                </>
                              }
                              pick={picks.get(id)}
                              editorOpen={keyEditor?.snapshotId === id}
                              onEditKeys={() => void openKeys(id)}
                              editorKeys={keyEditor?.snapshotId === id ? keyEditor.keys : null}
                              onToggleKey={(k, all) => toggleKey(id, k, all)}
                              onAllKeys={(on) => setPicks((cur) => new Map(cur).set(id, { ...(cur.get(id) as Pick), keys: on ? null : new Set() }))}
                            />
                          </li>
                        );
                      })}
                    </ul>
                  </li>
                );
              })}
            </ul>
          )}
          {emptyPick && <p className="text-[12px] text-prod">{emptyPick.label} has no keys selected. Tick at least one key or untick the file.</p>}
        </div>
      )}

      {step === 2 && (
        <div className="flex flex-col gap-4 text-[13px]">
          <div>
            <p className="label">Pass key for this share</p>
            <div className="flex flex-wrap items-center gap-2">
              <code className="mono min-w-[300px] flex-1 select-all rounded-md border border-focus bg-ink px-3 py-2.5 text-center text-[18px] tracking-wider" aria-label="Pass key">
                {passKey ?? "…"}
              </code>
              <button className="btn btn-primary" onClick={copyKey} disabled={!passKey}>
                <IconCopy /> Copy
              </button>
              <button className="btn" onClick={newKey} title="Generate a different pass key">
                New key
              </button>
            </div>
            <p className="mt-2 text-muted">
              LOKI made this key and <strong className="text-chalk">does not keep it</strong>. Copy it now: you will send it to the recipient separately from the file. Without it, the file cannot be opened.
            </p>
          </div>

          <div className="grid grid-cols-1 gap-3 sm:grid-cols-[auto_1fr]">
            <div>
              <label className="label" htmlFor="share-exp">
                Expires after
              </label>
              <select id="share-exp" className="input" value={expiry} onChange={(e) => setExpiry(Number(e.target.value))}>
                {EXPIRY.map((x) => (
                  <option key={x.days} value={x.days}>
                    {x.label}
                  </option>
                ))}
              </select>
            </div>
            <div>
              <label className="label" htmlFor="share-note">
                Note for the recipient (optional, encrypted)
              </label>
              <input id="share-note" className="input" value={note} onChange={(e) => setNote(e.target.value)} placeholder="Keys for your first day" />
            </div>
          </div>

          <p className="rounded-md border border-line px-3 py-2 text-muted">
            Sharing {plural(picks.size, ".env file")} with {plural(totalKeys, "key")}. You will choose where to save the file next.
          </p>

          <label className="flex items-center gap-2 font-medium">
            <input type="checkbox" checked={copied} onChange={(e) => setCopied(e.target.checked)} />I copied the pass key
          </label>
        </div>
      )}

      {step === 3 && created && (
        <div className="flex flex-col gap-4 text-[13px]">
          <p className="flex items-start gap-2">
            <span className="mt-0.5 tone-dev">
              <IconCheck />
            </span>
            <span>
              Saved <span className="mono">{baseName(created.path)}</span> with {plural(created.envCount, ".env file")} and {plural(created.keyCount, "key")}
              {created.expiresAt ? `, expires ${formatDate(created.expiresAt)}` : ", no expiry"}.
            </span>
          </p>
          <div>
            <h3 className="display mb-2 text-[15px]">How to send it</h3>
            <ol className="flex flex-col gap-3">
              <li className="flex gap-3">
                <Num n={1} />
                <span>
                  <strong>Send the file</strong> the usual way: email, Slack, a shared drive.
                </span>
              </li>
              <li className="flex gap-3">
                <Num n={2} />
                <div className="flex-1">
                  <strong>Send the pass key a different way</strong>: a chat message, a call, or in person. Never in the same message as the file.
                  <div className="mt-2 flex items-center gap-2">
                    <code className="mono select-all rounded-md border border-line bg-ink px-2.5 py-1.5 tracking-wider">{passKey}</code>
                    <button className="btn btn-sm" onClick={copyKey}>
                      <IconCopy /> Copy
                    </button>
                  </div>
                </div>
              </li>
              <li className="flex gap-3">
                <Num n={3} />
                <span>
                  The recipient opens it in LOKI with <strong>Open share</strong> in the left rail, types the pass key, and imports.
                </span>
              </li>
            </ol>
          </div>
          <p className="text-[12px] text-muted">After you close this window the pass key is gone from LOKI. If it gets lost, make a new share.</p>
        </div>
      )}
    </Modal>
  );
}

function Stepper({ step }: { step: Step }) {
  const steps = ["Choose .env files", "Pass key", "Send"];
  return (
    <ol className="mb-4 flex items-center gap-2 text-[12px]" aria-label="Steps">
      {steps.map((s, i) => {
        const n = (i + 1) as Step;
        const state = n < step ? "done" : n === step ? "now" : "later";
        return (
          <li key={s} className="flex items-center gap-2" aria-current={state === "now" ? "step" : undefined}>
            <span
              className={`flex h-5 w-5 items-center justify-center rounded-full border text-[12px] ${state === "now" ? "border-focus bg-focus text-ink" : state === "done" ? "border-focus text-focus" : "border-line text-muted"}`}
            >
              {state === "done" ? "✓" : n}
            </span>
            <span className={state === "now" ? "font-medium" : "text-muted"}>{s}</span>
            {i < steps.length - 1 && <span className="mx-1 h-px w-8 bg-line" aria-hidden />}
          </li>
        );
      })}
    </ol>
  );
}

function Num({ n }: { n: number }) {
  return <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full border border-focus text-[12px] text-focus">{n}</span>;
}

interface EnvRowProps {
  checked: boolean;
  onToggle: () => void;
  title: React.ReactNode;
  pick: Pick | undefined;
  editorOpen: boolean;
  onEditKeys: () => void;
  editorKeys: KeyInfo[] | null;
  onToggleKey: (key: string, all: string[]) => void;
  onAllKeys: (on: boolean, all: string[]) => void;
}

function EnvRow({ checked, onToggle, title, pick, editorOpen, onEditKeys, editorKeys, onToggleKey, onAllKeys }: EnvRowProps) {
  const all = editorKeys?.map((k) => k.key) ?? [];
  return (
    <div className="py-1">
      <div className="flex items-center gap-2 text-[13px]">
        <label className="flex min-w-0 flex-1 items-center gap-2">
          <input type="checkbox" checked={checked} onChange={onToggle} />
          {title}
        </label>
        {checked && (
          <button className="btn btn-quiet btn-sm" onClick={onEditKeys} aria-expanded={editorOpen}>
            {pick?.keys ? `${pick.keys.size} keys chosen` : "All keys"} {editorOpen ? "▴" : "▾"}
          </button>
        )}
      </div>
      {checked && editorOpen && editorKeys && (
        <div className="mb-2 ml-6 mt-1 rounded-md bg-ink p-2">
          <div className="mb-1 flex gap-2">
            <button className="btn btn-sm" onClick={() => onAllKeys(true, all)}>
              All
            </button>
            <button className="btn btn-sm" onClick={() => onAllKeys(false, all)}>
              None
            </button>
          </div>
          <ul className="mono grid max-h-40 grid-cols-2 gap-x-4 overflow-auto text-[12px]">
            {editorKeys.map((k) => (
              <li key={k.key}>
                <label className="flex items-center gap-2 truncate py-0.5">
                  <input type="checkbox" checked={!pick?.keys || pick.keys.has(k.key)} onChange={() => onToggleKey(k.key, all)} />
                  <span className="truncate">{k.key}</span>
                  {k.isEmpty && <span className="text-muted">∅</span>}
                </label>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
