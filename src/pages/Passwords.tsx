// Password store: credentials that are not .env values (dashboards, DB consoles, registrar,
// cloud root accounts). Encrypted in the same vault. Passwords and notes are only decrypted on
// Reveal or Copy, both audited.
import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import { Confirm } from "../components/Confirm";
import { IconCopy, IconEye, IconPlus } from "../components/icons";
import { Modal } from "../components/Modal";
import { PasswordField } from "../components/PasswordField";
import { useToast } from "../components/Toast";
import { formatDate, plural } from "../lib/format";
import {
  clipboardCopyText,
  credentialCopy,
  credentialCreate,
  credentialDelete,
  credentialList,
  credentialReveal,
  credentialUpdate,
  errMessage,
  passwordGenerate,
  projectList,
} from "../lib/ipc";
import { useSettings } from "../lib/settingsContext";
import { useReveal, type RevealSource } from "../lib/useReveal";
import type { Credential, CredentialInput, Project } from "../types";

// Reveal-store ids: positive = password, negative = notes of that credential.
const SOURCE: RevealSource = {
  reveal: (id) => credentialReveal(Math.abs(id), id > 0 ? "password" : "notes"),
  copy: (id) => credentialCopy(Math.abs(id), id > 0 ? "password" : "notes"),
};

export default function Passwords({ focusId }: { focusId?: number | null }) {
  const toast = useToast();
  const { settings } = useSettings();
  const store = useReveal(settings.revealSeconds, SOURCE);
  const [items, setItems] = useState<Credential[] | null>(null);
  const [projects, setProjects] = useState<Project[]>([]);
  const [query, setQuery] = useState("");
  const [projectFilter, setProjectFilter] = useState<string>("all");
  const [editing, setEditing] = useState<Credential | "new" | null>(null);
  const [deleting, setDeleting] = useState<Credential | null>(null);

  const load = useCallback(async () => {
    try {
      const [c, p] = await Promise.all([credentialList(), projectList()]);
      setItems(c);
      setProjects(p);
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }, [toast]);

  useEffect(() => {
    void load();
  }, [load]);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (items ?? []).filter((c) => {
      if (projectFilter === "none" && c.projectId !== null) return false;
      if (projectFilter !== "all" && projectFilter !== "none" && String(c.projectId) !== projectFilter) return false;
      if (!q) return true;
      return [c.title, c.username, c.url, c.projectName ?? "", ...c.tags].some((f) => f.toLowerCase().includes(q));
    });
  }, [items, query, projectFilter]);

  const weak = items?.filter((c) => c.isWeak).length ?? 0;
  const reused = items?.filter((c) => c.isReused).length ?? 0;
  const old = items?.filter((c) => c.ageDays > settings.rotationDays).length ?? 0;

  async function copyText(text: string, what: string) {
    try {
      const s = await clipboardCopyText(text);
      toast(`${what} copied, clears in ${s} s`, "success");
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }

  return (
    <div className="mx-auto max-w-[1100px] p-5">
      <div className="mb-4 flex flex-wrap items-end gap-3">
        <div>
          <h1 className="display text-[18px]">Passwords</h1>
          <p className="text-[13px] text-muted">Logins that are not in a .env: cloud consoles, databases, registrars, CI. Encrypted in your vault, never synced anywhere.</p>
        </div>
        <button className="btn btn-primary ml-auto" onClick={() => setEditing("new")}>
          <IconPlus /> Add password
        </button>
      </div>

      {items && items.length > 0 && (
        <div className="mb-3 flex flex-wrap items-center gap-2">
          <input className="input max-w-[280px]" placeholder="Search title, user, URL, tag" aria-label="Search passwords" value={query} onChange={(e) => setQuery(e.target.value)} />
          <select className="input w-auto" aria-label="Filter by project" value={projectFilter} onChange={(e) => setProjectFilter(e.target.value)}>
            <option value="all">All projects</option>
            <option value="none">Not linked to a project</option>
            {projects.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
          <span className="ml-auto flex gap-2 text-[12px]">
            {weak > 0 && <span className="chip tone-staging">{weak} weak</span>}
            {reused > 0 && <span className="chip tone-prod">{reused} reused</span>}
            {old > 0 && <span className="chip tone-staging">{old} older than {settings.rotationDays} days</span>}
          </span>
        </div>
      )}

      {items === null ? (
        <p className="text-[13px] text-muted">Loading…</p>
      ) : items.length === 0 ? (
        <div className="panel p-8 text-center">
          <h2 className="display mb-1 text-[15px]">No passwords yet</h2>
          <p className="mx-auto mb-4 max-w-[440px] text-[13px] text-muted">Keep the logins you need on push day next to your .env files: the production database console, the cloud root account, the DNS registrar.</p>
          <button className="btn btn-primary" onClick={() => setEditing("new")}>
            Add your first password
          </button>
        </div>
      ) : (
        <div className="panel overflow-x-auto">
          <table className="data-table">
            <thead>
              <tr>
                <th>Title</th>
                <th>Username</th>
                <th>Password</th>
                <th>Project</th>
                <th>
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {shown.map((c) => {
                const value = store.get(c.id);
                return (
                  <tr key={c.id} className={focusId === c.id ? "found-flash" : ""}>
                    <td className="align-top">
                      <div className="font-medium">{c.title}</div>
                      {c.url && <div className="mono truncate text-[12px] text-muted">{c.url}</div>}
                      <div className="mt-1 flex flex-wrap gap-1">
                        {c.isWeak && <span className="chip tone-staging">weak</span>}
                        {c.isReused && <span className="chip tone-prod">reused</span>}
                        {c.ageDays > settings.rotationDays && <span className="chip tone-staging">rotate ({c.ageDays} days)</span>}
                        {c.tags.map((t) => (
                          <span key={t} className="chip">
                            {t}
                          </span>
                        ))}
                      </div>
                    </td>
                    <td className="align-top">
                      {c.username ? (
                        <button className="btn btn-quiet btn-sm mono" onClick={() => void copyText(c.username, "Username")} title="Copy username">
                          {c.username}
                        </button>
                      ) : (
                        <span className="text-muted">-</span>
                      )}
                    </td>
                    <td className="align-top">
                      <div className="flex items-center gap-1.5">
                        {value !== undefined ? (
                          <>
                            <span className={`mono max-w-[240px] truncate rounded border border-line bg-ink px-1.5 py-0.5 text-[12px] ${store.remaining(c.id) <= 1 ? "fade-out" : ""}`}>{value || "(empty)"}</span>
                            <span className="text-[12px] text-muted">{store.remaining(c.id)} s</span>
                            <button className="btn btn-sm" onClick={() => store.hide(c.id)}>
                              Hide
                            </button>
                          </>
                        ) : (
                          <>
                            <span className="mono text-[12px] text-muted">••••••••</span>
                            <button className="btn btn-sm" onClick={() => void store.reveal(c.id)} aria-label={`Reveal password for ${c.title}`}>
                              <IconEye />
                            </button>
                          </>
                        )}
                        <button className="btn btn-sm" onClick={() => void store.copy(c.id)} aria-label={`Copy password for ${c.title}`}>
                          <IconCopy />
                        </button>
                      </div>
                      <div className="mt-1 text-[12px] text-muted">changed {formatDate(c.passwordChangedAt)}</div>
                    </td>
                    <td className="align-top text-[13px]">{c.projectName ?? <span className="text-muted">-</span>}</td>
                    <td className="align-top">
                      <div className="flex justify-end gap-1">
                        <button className="btn btn-sm" onClick={() => setEditing(c)}>
                          Edit
                        </button>
                        <button className="btn btn-sm btn-danger" onClick={() => setDeleting(c)}>
                          Delete
                        </button>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {shown.length === 0 && <p className="p-4 text-[13px] text-muted">Nothing matches. Clear the search or the project filter.</p>}
        </div>
      )}
      {items && items.length > 0 && <p className="mt-2 text-[12px] text-muted">{plural(items.length, "entry", "entries")}. Reveal hides again after {settings.revealSeconds} s; copies clear after {settings.clipboardClearSeconds} s and stay out of clipboard history.</p>}

      {editing && (
        <CredentialForm
          credential={editing === "new" ? null : editing}
          projects={projects}
          onClose={() => setEditing(null)}
          onSaved={async () => {
            setEditing(null);
            await load();
          }}
        />
      )}
      {deleting && (
        <Confirm
          title="Delete password"
          danger
          confirmLabel="Delete password"
          onClose={() => setDeleting(null)}
          onConfirm={async () => {
            try {
              await credentialDelete(deleting.id);
              toast(`Deleted ${deleting.title}`, "success");
              setDeleting(null);
              await load();
            } catch (e) {
              toast(errMessage(e), "error");
            }
          }}
        >
          <p>
            This permanently deletes <strong>{deleting.title}</strong> from your vault. It cannot be undone, except from a backup.
          </p>
        </Confirm>
      )}
    </div>
  );
}

function CredentialForm({ credential, projects, onClose, onSaved }: { credential: Credential | null; projects: Project[]; onClose: () => void; onSaved: () => void | Promise<void> }) {
  const toast = useToast();
  const [title, setTitle] = useState(credential?.title ?? "");
  const [username, setUsername] = useState(credential?.username ?? "");
  const [url, setUrl] = useState(credential?.url ?? "");
  const [tags, setTags] = useState(credential?.tags.join(", ") ?? "");
  const [projectId, setProjectId] = useState<number | null>(credential?.projectId ?? null);
  // Empty on edit = keep the stored password.
  const [password, setPassword] = useState("");
  const [notes, setNotes] = useState<string | null>(credential ? null : "");
  const [genLength, setGenLength] = useState(24);
  const [genSymbols, setGenSymbols] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function generate() {
    try {
      setPassword(await passwordGenerate(genLength, genSymbols));
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }

  async function loadNotes() {
    if (!credential) return;
    try {
      setNotes(await credentialReveal(credential.id, "notes"));
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!title.trim()) return setError("Give the entry a title, for example `AWS root` or `Staging DB`.");
    const input: CredentialInput = {
      title,
      username,
      url,
      tags: tags.split(",").map((t) => t.trim()).filter(Boolean),
      projectId,
      password: credential && password === "" ? null : password,
      notes,
    };
    setBusy(true);
    setError(null);
    try {
      if (credential) await credentialUpdate(credential.id, input);
      else await credentialCreate(input);
      setPassword("");
      setNotes(null);
      toast(credential ? "Saved" : "Password added", "success");
      await onSaved();
    } catch (err) {
      setError(errMessage(err));
    } finally {
      setBusy(false);
    }
  }

  function close() {
    setPassword("");
    setNotes(null);
    onClose();
  }

  return (
    <Modal
      title={credential ? `Edit ${credential.title}` : "Add password"}
      onClose={close}
      width={560}
      footer={
        <>
          <button className="btn" onClick={close} disabled={busy}>
            Cancel
          </button>
          <button className="btn btn-primary" type="submit" form="cred-form" disabled={busy}>
            {credential ? "Save" : "Add password"}
          </button>
        </>
      }
    >
      <form id="cred-form" onSubmit={submit} className="flex flex-col gap-3" autoComplete="off">
        <div>
          <label className="label" htmlFor="c-title">
            Title
          </label>
          <input id="c-title" className="input" value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Production database console" />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="label" htmlFor="c-user">
              Username
            </label>
            <input id="c-user" className="input mono" value={username} onChange={(e) => setUsername(e.target.value)} />
          </div>
          <div>
            <label className="label" htmlFor="c-url">
              URL or host
            </label>
            <input id="c-url" className="input mono" value={url} onChange={(e) => setUrl(e.target.value)} />
          </div>
        </div>
        <div>
          <PasswordField label={credential ? "New password (leave empty to keep the current one)" : "Password"} value={password} onChange={setPassword} showStrength={password.length > 0} autoComplete="new-password" />
          <div className="mt-2 flex flex-wrap items-center gap-3 text-[12px]">
            <button type="button" className="btn btn-sm" onClick={generate}>
              Generate
            </button>
            <label className="flex items-center gap-2">
              Length
              <input type="range" min={12} max={64} value={genLength} onChange={(e) => setGenLength(Number(e.target.value))} aria-label="Generated password length" />
              <span className="w-6 tabular-nums">{genLength}</span>
            </label>
            <label className="flex items-center gap-1.5">
              <input type="checkbox" checked={genSymbols} onChange={(e) => setGenSymbols(e.target.checked)} /> Symbols
            </label>
          </div>
        </div>
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="label" htmlFor="c-proj">
              Project (optional)
            </label>
            <select id="c-proj" className="input" value={projectId ?? ""} onChange={(e) => setProjectId(e.target.value ? Number(e.target.value) : null)}>
              <option value="">None</option>
              {projects.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label className="label" htmlFor="c-tags">
              Tags (comma separated)
            </label>
            <input id="c-tags" className="input" value={tags} onChange={(e) => setTags(e.target.value)} placeholder="database, prod" />
          </div>
        </div>
        <div>
          <label className="label" htmlFor="c-notes">
            Notes (encrypted like the password: recovery codes, security answers)
          </label>
          {notes === null ? (
            <button type="button" className="btn btn-sm" onClick={credential?.hasNotes ? loadNotes : () => setNotes("")}>
              {credential?.hasNotes ? "Reveal notes to edit" : "Add notes"}
            </button>
          ) : (
            <textarea id="c-notes" className="input h-24 resize-y" value={notes} onChange={(e) => setNotes(e.target.value)} />
          )}
        </div>
        {error && (
          <p className="text-[13px] text-prod" role="alert">
            {error}
          </p>
        )}
      </form>
    </Modal>
  );
}
