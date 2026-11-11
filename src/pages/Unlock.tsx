import { useState, type FormEvent } from "react";
import iconUrl from "../../assets/loki-icon.svg";
import { PasswordField } from "../components/PasswordField";
import { useToast } from "../components/Toast";
import { baseName } from "../lib/format";
import { errMessage, vaultCreate, vaultUnlock } from "../lib/ipc";

interface Props {
  /** True when a vault file already exists (unlock); false means first run (create). */
  exists: boolean;
  notice?: string | null;
  onUnlocked: () => void;
}

export default function Unlock({ exists, notice, onUnlocked }: Props) {
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const toast = useToast();
  const creating = !exists;

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    if (creating) {
      if (password.length < 8) return setError("Use at least 8 characters for the master password.");
      if (password !== confirm) return setError("The two passwords do not match.");
    } else if (password.length === 0) {
      return setError("Enter your master password.");
    }
    setBusy(true);
    try {
      const backup = creating ? (await vaultCreate(password), null) : await vaultUnlock(password);
      if (backup) toast(`LOKI upgraded your vault for the new features. A full copy was saved first in the backups folder: ${baseName(backup)}`, "success");
      setPassword("");
      setConfirm("");
      onUnlocked();
    } catch (err) {
      setError(errMessage(err));
      setPassword("");
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="flex h-full items-center justify-center p-6">
      <form onSubmit={submit} className="panel w-full max-w-[400px] p-6" aria-busy={busy}>
        <div className="mb-5 flex items-center gap-3">
          <img src={iconUrl} alt="" width={44} height={44} />
          <div>
            <h1 className="wordmark text-[32px] leading-none">LOKI</h1>
            <p className="text-[12px] text-muted">Local Only Key Inventory</p>
          </div>
        </div>

        <h2 className="display mb-1 text-[15px]">{creating ? "Create your vault" : "Unlock your vault"}</h2>
        <p className="mb-4 text-[13px] text-muted">
          {creating
            ? "Choose a master password. It encrypts everything on this computer and is never stored, so LOKI cannot recover it."
            : "Enter your master password to open your .env inventory."}
        </p>

        {notice && (
          <p className="mb-3 rounded-md border border-line px-3 py-2 text-[13px]" role="status">
            {notice}
          </p>
        )}

        <div className="flex flex-col gap-3">
          <PasswordField
            label="Master password"
            value={password}
            onChange={setPassword}
            autoFocus
            showStrength={creating}
            disabled={busy}
            autoComplete={creating ? "new-password" : "current-password"}
          />
          {creating && <PasswordField label="Confirm password" value={confirm} onChange={setConfirm} disabled={busy} autoComplete="new-password" />}
        </div>

        {error && (
          <p className="mt-3 text-[13px] text-prod" role="alert">
            {error}
          </p>
        )}

        <button type="submit" className="btn btn-primary mt-4 w-full py-2" disabled={busy}>
          {busy ? (creating ? "Creating vault…" : "Unlocking…") : creating ? "Create vault" : "Unlock"}
        </button>

        <div className={`mt-3 h-1 overflow-hidden rounded-full bg-line ${busy ? "" : "invisible"}`} aria-hidden>
          <div className="indeterminate h-full w-full" />
        </div>
        {busy && <p className="mt-1 text-[12px] text-muted">Deriving the encryption key. This takes about half a second.</p>}
      </form>
    </main>
  );
}
