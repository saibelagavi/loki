import { createContext, useCallback, useContext, useMemo, useRef, useState, type ReactNode } from "react";

type Kind = "info" | "error" | "success";
interface ToastItem {
  id: number;
  kind: Kind;
  message: string;
}

interface ToastApi {
  toast: (message: string, kind?: Kind) => void;
}

const Ctx = createContext<ToastApi>({ toast: () => undefined });
export const useToast = () => useContext(Ctx).toast;

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([]);
  const next = useRef(1);

  const toast = useCallback((message: string, kind: Kind = "info") => {
    const id = next.current++;
    setItems((cur) => [...cur.slice(-3), { id, kind, message }]);
    window.setTimeout(() => setItems((cur) => cur.filter((t) => t.id !== id)), kind === "error" ? 7000 : 4000);
  }, []);

  const api = useMemo(() => ({ toast }), [toast]);

  return (
    <Ctx.Provider value={api}>
      {children}
      <div className="pointer-events-none fixed bottom-4 right-4 z-50 flex w-[360px] max-w-[90vw] flex-col gap-2" role="status" aria-live="polite">
        {items.map((t) => (
          <div
            key={t.id}
            className="panel pointer-events-auto border-l-4 px-3 py-2 text-[13px] shadow-lg"
            style={{ borderLeftColor: t.kind === "error" ? "var(--prod)" : t.kind === "success" ? "var(--dev)" : "var(--focus)" }}
          >
            {t.message}
          </div>
        ))}
      </div>
    </Ctx.Provider>
  );
}
