import type { RevealStore } from "../lib/useReveal";
import { IconCopy, IconEye } from "./icons";

interface Props {
  id: number;
  store: RevealStore;
  /** Shown while masked. */
  maskedLabel?: string;
}

/** Masked value with Reveal and Copy. A revealed value shows a visible countdown, then fades back. */
export function RevealValue({ id, store, maskedLabel = "••••••••" }: Props) {
  const value = store.get(id);
  return (
    <span className="inline-flex max-w-full items-center gap-2">
      {value !== undefined ? (
        <>
          <span className="mono max-w-[360px] truncate rounded border border-line bg-ink px-1.5 py-0.5 text-[12px]">{value === "" ? "(empty)" : value}</span>
          <span className="text-[12px] text-muted" aria-live="off">
            hides in {store.remaining(id)} s
          </span>
          <button className="btn btn-sm" onClick={() => store.hide(id)}>
            Hide
          </button>
        </>
      ) : (
        <>
          <span className="mono text-[12px] text-muted" aria-label="masked value">
            {maskedLabel}
          </span>
          <button className="btn btn-sm" onClick={() => void store.reveal(id)} aria-label="Reveal value">
            <IconEye /> Reveal
          </button>
        </>
      )}
      <button className="btn btn-sm" onClick={() => void store.copy(id)} aria-label="Copy value">
        <IconCopy /> Copy
      </button>
    </span>
  );
}
