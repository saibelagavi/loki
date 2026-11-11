// Share a .env securely: encrypt a snapshot (or chosen keys) into a .lokishare file that opens
// only with a one-time pass key. The pass key is shown once here and never stored.
import { useEffect, useMemo, useState } from "react";
import { baseName, plural } from "../lib/format";
import { clipboardCopyText, errMessage, shareCreate, snapshotKeys } from "../lib/ipc";
import type { KeyInfo, ShareCreated } from "../types";
import { IconCheck, IconCopy } from "./icons";
import { Modal } from "./Modal";
import { useToast } from "./Toast";

interface Props {
  snapshotId: number;
  /** e.g. "main / production v3" */
  label: string;
  onClose: () => void;
}

const EXPIRY = [
  { days: 1, label: "1 day" },
  { days: 7, label: "7 days" },
  { days: 30, label: "30 days" },
  { days: 0, label: "Never" },
];

export function ShareDialog({ snapshotId, label, onClose }: Props) {
  const toast = useToast();
  const [keys, setKeys] = useState<KeyInfo[]>([]);
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [expiry, setExpiry] = useState(7);
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [created, setCreated] = useState<ShareCreated | null>(null);
  const [filter, setFilter] = useState("");

  useEffect(() => {
    snapshotKeys(snapshotId)
      .then((k) => {
        setKeys(k);
        setChosen(new Set(k.map((x) => x.key)));
      })
      .catch((e) => toast(errMessage(e), "error"));
  }, [snapshotId, toast]);

  const shown = useMemo(() => keys.filter((k) => k.key.toLowerCase().includes(filter.trim().toLowerCase())), [keys, filter]);

  async function create() {
    setBusy(true);
    try {
      const all = chosen.size === keys.length;
      const r = await shareCreate(snapshotId, all ? null : [...chosen], expiry || null, note.trim() || null);
      if (r) setCreated(r);
    } catch (e) {
      toast(errMessage(e), "error");
    } finally {
      setBusy(false);
    }
  }

  async function copyKey() {
    if (!created) return;
    try {
      const s = await clipboardCopyText(created.passKey);
      toast(`Pass key copied, clears in ${s} s`, "success");
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }

  function close() {
    setCreated(null); // drop the pass key from memory
    onClose();
  }

  const toggle = (k: string) =>
    setChosen((cur) => {
      const n = new Set(cur);
      if (n.has(k)) n.delete(k);
      else n.add(k);
      return n;
    });

  if (created) {
    return (
      <Modal title="Share file created" onClose={close} width={560} footer={<button className="btn btn-primary" onClick={close}>Done</button>}>
        <div className="flex flex-col gap-4 text-[13px]">
          <p className="flex items-center gap-2">
            <span className="tone-dev">
              <IconCheck />
            </span>
            Saved <span className="mono">{baseName(created.path)}</span> with {plural(created.keyCount, "key")}.
          </p>
          <div>
            <p className="label">Pass key (shown once, LOKI does not keep it)</p>
            <div className="flex items-center gap-2">
              <code className="mono flex-1 select-all rounded-md border border-focus bg-ink px-3 py-2 text-center text-[18px] tracking-wider">{created.passKey}</code>
              <button className="btn" onClick={copyKey}>
                <IconCopy /> Copy
              </button>
            </div>
          </div>
          <ul className="list-disc space-y-1 pl-5 text-muted">
            <li>
              Send the file and the pass key through <strong className="text-chalk">different channels</strong>, for example the file by email and the pass key in a chat or a call.
            </li>
            <li>The recipient opens it in LOKI with Open share. Without the pass key the file cannot be read.</li>
            <li>{created.expiresAt ? `LOKI refuses to import it after ${new Date(created.expiresAt).toLocaleString()}.` : "It has no expiry date."}</li>
          </ul>
        </div>
      </Modal>
    );
  }

  return (
    <Modal
      title="Share securely"
      onClose={close}
      width={600}
      footer={
        <>
          <button className="btn" onClick={close} disabled={busy}>
            Cancel
          </button>
          <button className="btn btn-primary" onClick={create} disabled={busy || chosen.size === 0}>
            {busy ? "Encrypting…" : `Create share file (${plural(chosen.size, "key")})`}
          </button>
        </>
      }
    >
      <p className="mb-3 text-[13px] text-muted">
        Encrypts <span className="mono text-chalk">{label}</span> into a <span className="mono">.lokishare</span> file with a one-time pass key (Argon2id + XChaCha20-Poly1305).
      </p>

      <div className="mb-2 flex items-center gap-2">
        <input className="input" placeholder="Filter keys" aria-label="Filter keys" value={filter} onChange={(e) => setFilter(e.target.value)} />
        <button className="btn btn-sm" onClick={() => setChosen(new Set(keys.map((k) => k.key)))}>
          All
        </button>
        <button className="btn btn-sm" onClick={() => setChosen(new Set())}>
          None
        </button>
      </div>
      <ul className="mono mb-3 grid max-h-56 grid-cols-2 gap-x-4 overflow-auto rounded-md border border-line p-2 text-[12px]" aria-label="Keys to share">
        {shown.map((k) => (
          <li key={k.key}>
            <label className="flex items-center gap-2 truncate py-0.5">
              <input type="checkbox" checked={chosen.has(k.key)} onChange={() => toggle(k.key)} />
              <span className="truncate">{k.key}</span>
              {k.isEmpty && <span className="text-muted">∅</span>}
            </label>
          </li>
        ))}
      </ul>

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
          <input id="share-note" className="input" value={note} onChange={(e) => setNote(e.target.value)} placeholder="Staging keys for the new laptop" />
        </div>
      </div>
    </Modal>
  );
}
