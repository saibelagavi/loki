// Holds revealed secret values in memory for a short, visible time and then drops them.
// A value lives in this hook's ref (and the React state that renders it) only until the reveal
// timeout fires, the component unmounts, or the vault locks (the workspace unmounts on lock).
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { copyValue, errMessage, revealValue } from "./ipc";
import { useToast } from "../components/Toast";

interface Entry {
  value: string;
  expires: number;
  timer: number;
}

export interface RevealStore {
  /** Revealed value, or undefined when masked. */
  get: (id: number) => string | undefined;
  /** Whole seconds left before it is hidden again. */
  remaining: (id: number) => number;
  reveal: (id: number) => Promise<void>;
  hide: (id: number) => void;
  hideAll: () => void;
  copy: (id: number) => Promise<void>;
  anyRevealed: boolean;
}

export interface RevealSource {
  reveal: (id: number) => Promise<string>;
  /** Resolves to the clipboard clear timeout in seconds. */
  copy: (id: number) => Promise<number>;
}

const VARIABLES: RevealSource = { reveal: revealValue, copy: copyValue };

/** `source` defaults to .env variables; the password store passes its own reveal/copy. */
export function useReveal(seconds: number, source: RevealSource = VARIABLES): RevealStore {
  const toast = useToast();
  const entries = useRef(new Map<number, Entry>());
  const [, setTick] = useState(0);
  const bump = useCallback(() => setTick((n) => n + 1), []);

  const hide = useCallback(
    (id: number) => {
      const e = entries.current.get(id);
      if (e) {
        window.clearTimeout(e.timer);
        entries.current.delete(id);
        bump();
      }
    },
    [bump],
  );

  const hideAll = useCallback(() => {
    for (const e of entries.current.values()) window.clearTimeout(e.timer);
    entries.current.clear();
    bump();
  }, [bump]);

  const reveal = useCallback(
    async (id: number) => {
      try {
        const value = await source.reveal(id);
        const old = entries.current.get(id);
        if (old) window.clearTimeout(old.timer);
        const timer = window.setTimeout(() => hide(id), seconds * 1000);
        entries.current.set(id, { value, expires: Date.now() + seconds * 1000, timer });
        bump();
      } catch (e) {
        toast(errMessage(e), "error");
      }
    },
    [seconds, hide, bump, toast, source],
  );

  const copy = useCallback(
    async (id: number) => {
      try {
        const clearAfter = await source.copy(id);
        toast(`Copied, clears in ${clearAfter} s`, "success");
      } catch (e) {
        toast(errMessage(e), "error");
      }
    },
    [toast, source],
  );

  // Drive the visible countdown while anything is revealed.
  const anyRevealed = entries.current.size > 0;
  useEffect(() => {
    if (!anyRevealed) return;
    const t = window.setInterval(bump, 250);
    return () => window.clearInterval(t);
  }, [anyRevealed, bump]);

  // Drop everything on unmount.
  useEffect(() => {
    const map = entries.current;
    return () => {
      for (const e of map.values()) window.clearTimeout(e.timer);
      map.clear();
    };
  }, []);

  return useMemo(
    () => ({
      get: (id) => entries.current.get(id)?.value,
      remaining: (id) => {
        const e = entries.current.get(id);
        return e ? Math.max(0, Math.ceil((e.expires - Date.now()) / 1000)) : 0;
      },
      reveal,
      hide,
      hideAll,
      copy,
      anyRevealed,
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [reveal, hide, hideAll, copy, anyRevealed, entries.current.size],
  );
}
