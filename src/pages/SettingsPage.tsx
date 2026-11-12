// Settings: security, appearance, push-check rules, master password, encrypted backup, data folder,
// audit log.
import { useEffect, useState, type ReactNode } from "react";
import { PasswordField } from "../components/PasswordField";
import { useToast } from "../components/Toast";
import { baseName, formatDate, plural } from "../lib/format";
import { appInfo, auditList, backupExport, backupImport, clipboardCopyText, errMessage, rulesCatalog, settingsSet, vaultChangePassword } from "../lib/ipc";
import { useSettings } from "../lib/settingsContext";
import { getTheme, setTheme, type ThemeChoice } from "../lib/theme";
import type { AuditEntry, CustomRule, RuleInfo, Settings, Severity } from "../types";

function Section({ title, description, children }: { title: string; description?: string; children: ReactNode }) {
  return (
    <section className="panel mb-5 p-4" aria-label={title}>
      <h2 className="text-[15px] font-semibold">{title}</h2>
      {description && <p className="mb-3 text-[12px] text-muted">{description}</p>}
      <div className={description ? "" : "mt-3"}>{children}</div>
    </section>
  );
}

function NumberField({ id, label, value, min, max, unit, onChange }: { id: string; label: string; value: number; min: number; max: number; unit: string; onChange: (n: number) => void }) {
  return (
    <div>
      <label className="label" htmlFor={id}>
        {label}
      </label>
      <div className="flex items-center gap-2">
        <input id={id} className="input w-24" type="number" min={min} max={max} value={value} onChange={(e) => onChange(Number(e.target.value))} />
        <span className="text-[12px] text-muted">{unit}</span>
      </div>
    </div>
  );
}

export default function SettingsPage({ onClose }: { onClose: () => void }) {
  const toast = useToast();
  const { settings, setSettings } = useSettings();
  const [form, setForm] = useState<Settings>(settings);
  const [saving, setSaving] = useState(false);
  const [theme, setThemeState] = useState<ThemeChoice>(getTheme());
  const [catalog, setCatalog] = useState<RuleInfo[]>([]);

  useEffect(() => {
    rulesCatalog()
      .then(setCatalog)
      .catch(() => undefined);
  }, []);

  const dirty = JSON.stringify(form) !== JSON.stringify(settings);
  const patch = (p: Partial<Settings>) => setForm((f) => ({ ...f, ...p }));

  async function save() {
    setSaving(true);
    try {
      const saved = await settingsSet(form);
      setSettings(saved);
      setForm(saved);
      toast("Settings saved", "success");
    } catch (e) {
      toast(errMessage(e), "error");
    } finally {
      setSaving(false);
    }
  }

  const ruleOn = (id: string) => !form.disabledRules.includes(id);
  const toggleRule = (id: string) => patch({ disabledRules: ruleOn(id) ? [...form.disabledRules, id] : form.disabledRules.filter((x) => x !== id) });

  return (
    <div className="min-h-0 flex-1 overflow-auto">
      <div className="mx-auto max-w-[760px] p-5">
        <div className="mb-4 flex items-center gap-3">
          <h1 className="display text-[18px]">Settings</h1>
          <button className="btn ml-auto" onClick={onClose}>
            Close
          </button>
          <button className="btn btn-primary" onClick={save} disabled={!dirty || saving}>
            Save settings
          </button>
        </div>

        <Section title="Security" description="Locking drops the keys from memory. Revealed values and copied secrets clear themselves.">
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
            <NumberField id="autolock" label="Auto-lock after" value={form.autoLockMinutes} min={1} max={240} unit="minutes idle" onChange={(n) => patch({ autoLockMinutes: n })} />
            <NumberField id="clip" label="Clear clipboard after" value={form.clipboardClearSeconds} min={5} max={300} unit="seconds" onChange={(n) => patch({ clipboardClearSeconds: n })} />
            <NumberField id="reveal" label="Hide revealed values after" value={form.revealSeconds} min={3} max={60} unit="seconds" onChange={(n) => patch({ revealSeconds: n })} />
            <NumberField id="diffreveal" label="Diff checker shows values for" value={form.diffRevealSeconds} min={10} max={600} unit="seconds" onChange={(n) => patch({ diffRevealSeconds: n })} />
            <NumberField id="rotation" label="Remind me to rotate after" value={form.rotationDays} min={7} max={730} unit="days" onChange={(n) => patch({ rotationDays: n })} />
          </div>
          <div className="mt-4 flex flex-col gap-2">
            <label className="flex items-center gap-2 text-[13px]">
              <input type="checkbox" checked={form.lockOnMinimize} onChange={(e) => patch({ lockOnMinimize: e.target.checked })} /> Lock when the window is minimized
            </label>
            <label className="flex items-center gap-2 text-[13px]">
              <input type="checkbox" checked={form.excludeFromCapture} onChange={(e) => patch({ excludeFromCapture: e.target.checked })} /> Hide this window from screenshots and screen sharing
            </label>
          </div>
        </Section>

        <Section title="Appearance">
          <label className="label" htmlFor="theme">
            Theme
          </label>
          <select
            id="theme"
            className="input w-auto"
            value={theme}
            onChange={(e) => {
              const t = e.target.value as ThemeChoice;
              setThemeState(t);
              setTheme(t);
            }}
          >
            <option value="system">Follow Windows</option>
            <option value="dark">Dark</option>
            <option value="light">Light</option>
          </select>
        </Section>

        <Section title="Push-check rules" description="Switch rules off, or add your own. Rules look at production values only, except the key checks.">
          <ul className="divide-y divide-line">
            {catalog.map((r) => (
              <li key={r.id} className="py-2">
                <label className="flex items-start gap-3 text-[13px]">
                  <input type="checkbox" className="mt-0.5" checked={ruleOn(r.id)} onChange={() => toggleRule(r.id)} />
                  <span>
                    <span className="font-medium">{r.title}</span> <span className="chip ml-1">{r.severity}</span>
                    <span className="block text-[12px] text-muted">{r.description}</span>
                  </span>
                </label>
              </li>
            ))}
          </ul>
          <CustomRules rules={form.customRules} ruleOn={ruleOn} toggleRule={toggleRule} onChange={(customRules) => patch({ customRules })} />
          {dirty && <p className="mt-3 text-[12px] text-muted">You have unsaved changes. Use Save settings above.</p>}
        </Section>

        <ChangePassword />
        <Backup />
        <YourData />
        <Audit />
      </div>
    </div>
  );
}

function CustomRules({ rules, ruleOn, toggleRule, onChange }: { rules: CustomRule[]; ruleOn: (id: string) => boolean; toggleRule: (id: string) => void; onChange: (r: CustomRule[]) => void }) {
  const [keyPattern, setKeyPattern] = useState("");
  const [op, setOp] = useState<CustomRule["op"]>("contains");
  const [text, setText] = useState("");
  const [severity, setSeverity] = useState<Severity>("warning");
  const [prodOnly, setProdOnly] = useState(true);

  function add() {
    if (!keyPattern.trim() || !text.trim()) return;
    onChange([...rules, { id: `custom_${Date.now().toString(36)}`, keyPattern: keyPattern.trim(), op, text: text.trim(), severity, productionOnly: prodOnly }]);
    setKeyPattern("");
    setText("");
  }

  return (
    <div className="mt-4 border-t border-line pt-3">
      <h3 className="mb-1 text-[13px] font-semibold">Custom rules ({rules.length})</h3>
      <p className="mb-2 text-[12px] text-muted">A key pattern (use * as a wildcard, like *_URL or STRIPE_*) plus text the value contains or starts with.</p>
      {rules.length > 0 && (
        <ul className="mb-3 divide-y divide-line rounded-md border border-line">
          {rules.map((r) => (
            <li key={r.id} className="flex flex-wrap items-center gap-2 px-3 py-2 text-[13px]">
              <input type="checkbox" checked={ruleOn(r.id)} onChange={() => toggleRule(r.id)} aria-label="Rule enabled" />
              <span className="mono">{r.keyPattern}</span>
              <span className="text-muted">{r.op === "contains" ? "contains" : "starts with"}</span>
              <span className="mono">“{r.text}”</span>
              <span className="chip">{r.severity}</span>
              {r.productionOnly && <span className="chip">production only</span>}
              <button className="btn btn-sm btn-danger ml-auto" onClick={() => onChange(rules.filter((x) => x.id !== r.id))}>
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="grid grid-cols-1 items-end gap-2 sm:grid-cols-[1fr_auto_1fr_auto]">
        <div>
          <label className="label" htmlFor="cr-key">
            Key pattern
          </label>
          <input id="cr-key" className="input mono" value={keyPattern} onChange={(e) => setKeyPattern(e.target.value)} placeholder="*_URL" />
        </div>
        <div>
          <label className="label" htmlFor="cr-op">
            Value
          </label>
          <select id="cr-op" className="input" value={op} onChange={(e) => setOp(e.target.value as CustomRule["op"])}>
            <option value="contains">contains</option>
            <option value="starts_with">starts with</option>
          </select>
        </div>
        <div>
          <label className="label" htmlFor="cr-text">
            Text
          </label>
          <input id="cr-text" className="input mono" value={text} onChange={(e) => setText(e.target.value)} placeholder="staging" />
        </div>
        <div>
          <label className="label" htmlFor="cr-sev">
            Severity
          </label>
          <select id="cr-sev" className="input" value={severity} onChange={(e) => setSeverity(e.target.value as Severity)}>
            <option value="error">error</option>
            <option value="warning">warning</option>
            <option value="info">info</option>
          </select>
        </div>
      </div>
      <div className="mt-2 flex items-center gap-4">
        <label className="flex items-center gap-2 text-[13px]">
          <input type="checkbox" checked={prodOnly} onChange={(e) => setProdOnly(e.target.checked)} /> Production only
        </label>
        <button className="btn" onClick={add} disabled={!keyPattern.trim() || !text.trim()}>
          Add rule
        </button>
      </div>
    </div>
  );
}

function ChangePassword() {
  const toast = useToast();
  const [oldPw, setOldPw] = useState("");
  const [newPw, setNewPw] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function change() {
    setError(null);
    if (newPw.length < 8) return setError("Use at least 8 characters for the new password.");
    if (newPw !== confirm) return setError("The new passwords do not match.");
    setBusy(true);
    try {
      await vaultChangePassword(oldPw, newPw);
      toast("Master password changed. Every value was re-encrypted.", "success");
      setOldPw("");
      setNewPw("");
      setConfirm("");
    } catch (e) {
      setError(errMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Section title="Master password" description="Changing it re-encrypts every value in one step. If anything fails, nothing changes.">
      <div className="grid max-w-[420px] gap-3">
        <PasswordField label="Current password" value={oldPw} onChange={setOldPw} disabled={busy} autoComplete="current-password" />
        <PasswordField label="New password" value={newPw} onChange={setNewPw} showStrength disabled={busy} autoComplete="new-password" />
        <PasswordField label="Confirm new password" value={confirm} onChange={setConfirm} disabled={busy} autoComplete="new-password" />
        {error && (
          <p className="text-[13px] text-prod" role="alert">
            {error}
          </p>
        )}
        <div>
          <button className="btn" onClick={change} disabled={busy || !oldPw || !newPw}>
            {busy ? "Re-encrypting…" : "Change master password"}
          </button>
        </div>
      </div>
    </Section>
  );
}

function Backup() {
  const toast = useToast();
  const [pw, setPw] = useState("");
  const [confirm, setConfirm] = useState("");
  const [impPw, setImpPw] = useState("");
  const [busy, setBusy] = useState(false);

  async function doExport() {
    if (pw.length < 8) return toast("Use at least 8 characters for the backup password.", "error");
    if (pw !== confirm) return toast("The backup passwords do not match.", "error");
    setBusy(true);
    try {
      const path = await backupExport(pw);
      if (path) toast(`Backup saved: ${baseName(path)}`, "success");
      setPw("");
      setConfirm("");
    } catch (e) {
      toast(errMessage(e), "error");
    } finally {
      setBusy(false);
    }
  }

  async function doImport() {
    if (!impPw) return toast("Enter the backup password first.", "error");
    setBusy(true);
    try {
      const r = await backupImport(impPw);
      if (r) {
        toast(`Imported ${plural(r.projects, "project")}, ${plural(r.targets, "target")}, ${plural(r.snapshots, "snapshot")}, ${plural(r.credentials ?? 0, "password")}${r.renamed ? ` (${r.renamed} renamed to avoid a clash)` : ""}. Switch projects to see them.`, "success");
        setImpPw("");
      }
    } catch (e) {
      toast(errMessage(e), "error");
    } finally {
      setBusy(false);
    }
  }

  return (
    <Section title="Encrypted backup" description="A .loki file holds your whole inventory, passwords included, under its own password (Argon2id + XChaCha20-Poly1305). Importing adds projects next to what you have; it never overwrites.">
      <div className="grid gap-6 sm:grid-cols-2">
        <div className="grid content-start gap-3">
          <h3 className="text-[13px] font-semibold">Export</h3>
          <PasswordField label="Backup password" value={pw} onChange={setPw} showStrength disabled={busy} autoComplete="new-password" />
          <PasswordField label="Confirm backup password" value={confirm} onChange={setConfirm} disabled={busy} autoComplete="new-password" />
          <div>
            <button className="btn" onClick={doExport} disabled={busy || !pw}>
              Export backup (.loki)
            </button>
          </div>
        </div>
        <div className="grid content-start gap-3">
          <h3 className="text-[13px] font-semibold">Import</h3>
          <PasswordField label="Backup password" value={impPw} onChange={setImpPw} disabled={busy} autoComplete="off" />
          <div>
            <button className="btn" onClick={doImport} disabled={busy || !impPw}>
              Import backup…
            </button>
          </div>
        </div>
      </div>
    </Section>
  );
}

// Uninstalling never shows a LOKI window (the Store version cannot), so this is where people learn
// that the vault outlives the app and how to remove it for good.
function YourData() {
  const toast = useToast();
  const [dir, setDir] = useState<string | null>(null);

  useEffect(() => {
    appInfo()
      .then((i) => setDir(i.dataDir))
      .catch(() => undefined);
  }, []);

  async function copy() {
    if (!dir) return;
    try {
      await clipboardCopyText(dir);
      toast("Folder path copied", "success");
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }

  return (
    <Section title="Your data" description="Everything LOKI keeps is in this folder: the encrypted vault, the safety copies made before upgrades, and window preferences.">
      <div className="flex flex-wrap items-center gap-2">
        <code className="mono min-w-0 flex-1 select-all break-all rounded-md border border-line px-3 py-2 text-[13px]" aria-label="Data folder">
          {dir ?? "…"}
        </code>
        <button className="btn" onClick={copy} disabled={!dir}>
          Copy path
        </button>
      </div>
      <ul className="mt-3 flex list-disc flex-col gap-1 pl-5 text-[13px]">
        <li>Uninstalling or reinstalling LOKI keeps this folder, so your vault is safe. This holds for the Microsoft Store version and the installer.</li>
        <li>To remove everything, uninstall LOKI, then delete this folder. Export an encrypted backup first if you may need your data again.</li>
      </ul>
    </Section>
  );
}

function Audit() {
  const toast = useToast();
  const [rows, setRows] = useState<AuditEntry[] | null>(null);

  async function load() {
    try {
      setRows(await auditList(300));
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }

  return (
    <Section title="Audit log" description="Reveals, copies, exports, imports and deletes are logged here. It records which key, never the value.">
      <button className="btn mb-3" onClick={load}>
        {rows ? "Refresh" : "Show audit log"}
      </button>
      {rows && (
        <div className="max-h-80 overflow-auto rounded-md border border-line">
          <table className="data-table">
            <thead>
              <tr>
                <th>When</th>
                <th>Action</th>
                <th>Detail</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.id}>
                  <td className="whitespace-nowrap text-[12px] text-muted">{formatDate(r.ts)}</td>
                  <td className="mono text-[12px]">{r.action}</td>
                  <td className="mono text-[12px]">{r.detail ?? r.entity}</td>
                </tr>
              ))}
              {rows.length === 0 && (
                <tr>
                  <td colSpan={3} className="text-muted">
                    Nothing logged yet.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}
    </Section>
  );
}
