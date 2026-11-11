import { useState, type ReactNode } from "react";
import { Modal } from "./Modal";

interface Props {
  title: string;
  children: ReactNode;
  confirmLabel: string;
  danger?: boolean;
  /** Optional note field (used by "Restore"). */
  noteLabel?: string;
  onConfirm: (note: string) => Promise<void> | void;
  onClose: () => void;
}

export function Confirm({ title, children, confirmLabel, danger, noteLabel, onConfirm, onClose }: Props) {
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);

  async function go() {
    setBusy(true);
    try {
      await onConfirm(note);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      title={title}
      onClose={onClose}
      width={460}
      footer={
        <>
          <button className="btn" onClick={onClose} disabled={busy}>
            Cancel
          </button>
          <button className={`btn ${danger ? "btn-danger" : "btn-primary"}`} onClick={go} disabled={busy}>
            {confirmLabel}
          </button>
        </>
      }
    >
      <div className="text-[13px]">{children}</div>
      {noteLabel && (
        <div className="mt-3">
          <label className="label" htmlFor="confirm-note">
            {noteLabel}
          </label>
          <input id="confirm-note" className="input" value={note} onChange={(e) => setNote(e.target.value)} />
        </div>
      )}
    </Modal>
  );
}
