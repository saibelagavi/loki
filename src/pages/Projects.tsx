// Left rail (project list) and the create / edit project form.
import { useState, type FormEvent } from "react";
import { IconFolder, IconLock, IconPlus, IconSearch, IconSettings } from "../components/icons";
import { Modal } from "../components/Modal";
import { useToast } from "../components/Toast";
import { errMessage, pickFolder, projectCreate, projectUpdate } from "../lib/ipc";
import type { Project } from "../types";

export type RailView = "overview" | "diff" | "passwords" | "settings" | "project";

interface RailProps {
  projects: Project[];
  selectedId: number | null;
  view: RailView;
  onSelect: (id: number) => void;
  onView: (v: Exclude<RailView, "project">) => void;
  onAdd: () => void;
  onSearch: () => void;
  onOpenShare: () => void;
  onLock: () => void;
}

const TOOLS: { id: "overview" | "diff" | "passwords"; label: string; icon: string }[] = [
  { id: "overview", label: "Overview", icon: "◆" },
  { id: "diff", label: "Diff checker", icon: "≠" },
  { id: "passwords", label: "Passwords", icon: "✱" },
];

export function ProjectRail({ projects, selectedId, view, onSelect, onView, onAdd, onSearch, onOpenShare, onLock }: RailProps) {
  return (
    <nav className="flex h-full w-[220px] shrink-0 flex-col border-r border-line bg-surface" aria-label="Main">
      <div className="px-4 pb-2 pt-4">
        <p className="wordmark text-[24px] leading-none">LOKI</p>
        <p className="mt-1 text-[12px] text-muted">Local Only Key Inventory</p>
      </div>
      <ul className="px-2 pt-2">
        {TOOLS.map((t) => (
          <li key={t.id}>
            <button
              className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[13px] hover:bg-ink ${view === t.id ? "bg-ink font-medium" : ""}`}
              aria-current={view === t.id ? "page" : undefined}
              onClick={() => onView(t.id)}
            >
              <span aria-hidden className="w-4 text-center text-[12px]" style={{ color: view === t.id ? "var(--focus)" : "var(--muted)" }}>
                {t.icon}
              </span>
              {t.label}
            </button>
          </li>
        ))}
      </ul>
      <p className="px-4 pb-1 pt-3 text-[12px] text-muted">Projects</p>
      <ul className="min-h-0 flex-1 overflow-auto px-2">
        {projects.map((p) => {
          const active = p.id === selectedId && view === "project";
          return (
            <li key={p.id}>
              <button
                className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[13px] hover:bg-ink ${active ? "bg-ink font-medium" : ""}`}
                onClick={() => onSelect(p.id)}
                aria-current={active ? "page" : undefined}
              >
                <span aria-hidden className="text-[12px]" style={{ color: active ? "var(--focus)" : "var(--muted)" }}>
                  {active ? "●" : "○"}
                </span>
                <span className="truncate">{p.name}</span>
              </button>
            </li>
          );
        })}
      </ul>
      <div className="flex flex-col gap-1 border-t border-line p-2">
        <button className="btn btn-quiet justify-start" onClick={onAdd}>
          <IconPlus /> Add project
        </button>
        <button className="btn btn-quiet justify-start" onClick={onSearch}>
          <IconSearch /> Search keys <kbd className="ml-auto text-[12px] text-muted">Ctrl+K</kbd>
        </button>
        <button className="btn btn-quiet justify-start" onClick={onOpenShare}>
          <IconFolder /> Open share
        </button>
        <button className={`btn btn-quiet justify-start ${view === "settings" ? "bg-ink" : ""}`} onClick={() => onView("settings")}>
          <IconSettings /> Settings
        </button>
        <button className="btn btn-quiet justify-start" onClick={onLock}>
          <IconLock /> Lock <kbd className="ml-auto text-[12px] text-muted">Ctrl+L</kbd>
        </button>
      </div>
    </nav>
  );
}

interface FormProps {
  project?: Project;
  onClose: () => void;
  onSaved: (p: Project) => void;
}

export function ProjectForm({ project, onClose, onSaved }: FormProps) {
  const toast = useToast();
  const [name, setName] = useState(project?.name ?? "");
  const [rootPath, setRootPath] = useState(project?.rootPath ?? "");
  const [description, setDescription] = useState(project?.description ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function browse() {
    const p = await pickFolder();
    if (p) setRootPath(p);
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!name.trim()) return setError("Give the project a name, for example `QSeverse API`.");
    setBusy(true);
    setError(null);
    try {
      const saved = project
        ? await projectUpdate(project.id, name, rootPath || null, description || null)
        : await projectCreate(name, rootPath || null, description || null);
      toast(project ? "Project saved" : "Project added. Now add a target and import its .env file.", "success");
      onSaved(saved);
    } catch (err) {
      setError(errMessage(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      title={project ? "Edit project" : "Add project"}
      onClose={onClose}
      width={480}
      footer={
        <>
          <button className="btn" onClick={onClose} disabled={busy}>
            Cancel
          </button>
          <button className="btn btn-primary" form="project-form" type="submit" disabled={busy}>
            {project ? "Save project" : "Add project"}
          </button>
        </>
      }
    >
      <form id="project-form" onSubmit={submit} className="flex flex-col gap-3">
        <div>
          <label className="label" htmlFor="p-name">
            Name
          </label>
          <input id="p-name" className="input" value={name} onChange={(e) => setName(e.target.value)} placeholder="QSeverse API" />
        </div>
        <div>
          <label className="label" htmlFor="p-root">
            Project folder (optional)
          </label>
          <div className="flex gap-2">
            <input id="p-root" className="input mono" value={rootPath} onChange={(e) => setRootPath(e.target.value)} placeholder="C:\Users\Name\My Projects\api" />
            <button type="button" className="btn" onClick={browse}>
              <IconFolder /> Browse
            </button>
          </div>
          <p className="mt-1 text-[12px] text-muted">Used by Scan folder to find .env files. LOKI only reads it when you ask.</p>
        </div>
        <div>
          <label className="label" htmlFor="p-desc">
            Description (optional)
          </label>
          <input id="p-desc" className="input" value={description} onChange={(e) => setDescription(e.target.value)} />
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
