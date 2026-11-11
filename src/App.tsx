import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { ToastProvider } from "./components/Toast";
import { inTauri, onVaultLocked, vaultStatus, vaultTouch } from "./lib/ipc";
import Unlock from "./pages/Unlock";

// Every page except Unlock is lazy-loaded.
const Workspace = lazy(() => import("./pages/Workspace"));

type Phase = "loading" | "locked" | "unlocked";

export default function App() {
  const [phase, setPhase] = useState<Phase>("loading");
  const [exists, setExists] = useState(true);
  const [notice, setNotice] = useState<string | null>(null);

  const refreshStatus = useCallback(async () => {
    try {
      const s = await vaultStatus();
      setExists(s.exists);
      setPhase(s.unlocked ? "unlocked" : "locked");
    } catch {
      setPhase("locked");
    }
  }, []);

  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus]);

  // The core locks the vault on idle or minimize; drop the whole workspace (and every value in it).
  useEffect(() => {
    if (!inTauri()) return;
    let off: (() => void) | undefined;
    void onVaultLocked((reason) => {
      setNotice(reason === "minimize" ? "Locked because the window was minimized." : "Locked after a period of inactivity.");
      setPhase("locked");
    }).then((fn) => (off = fn));
    return () => off?.();
  }, []);

  // Tell the core about real user activity (throttled) so auto-lock counts it.
  const lastTouch = useRef(0);
  useEffect(() => {
    if (phase !== "unlocked" || !inTauri()) return;
    const touch = () => {
      const now = Date.now();
      if (now - lastTouch.current > 5000) {
        lastTouch.current = now;
        void vaultTouch().catch(() => undefined);
      }
    };
    const events = ["keydown", "pointerdown", "wheel"] as const;
    events.forEach((e) => window.addEventListener(e, touch, { passive: true }));
    return () => events.forEach((e) => window.removeEventListener(e, touch));
  }, [phase]);

  return (
    <ToastProvider>
      {phase === "loading" && null}
      {phase === "locked" && (
        <Unlock
          exists={exists}
          notice={notice}
          onUnlocked={() => {
            setNotice(null);
            setExists(true);
            setPhase("unlocked");
          }}
        />
      )}
      {phase === "unlocked" && (
        <Suspense fallback={null}>
          <Workspace
            onLocked={() => {
              setNotice(null);
              setPhase("locked");
            }}
          />
        </Suspense>
      )}
    </ToastProvider>
  );
}
