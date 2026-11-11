// Targets: one deployed thing = branch + stage. Add, import, re-import, archive, delete.
import { useState, type FormEvent } from "react";
import { Confirm } from "../components/Confirm";
import { ImportDialog } from "../components/ImportDialog";
import { KeyNotes } from "../components/KeyNotes";
import { IconFolder, IconPlus } from "../components/icons";
import { Modal } from "../components/Modal";
import { ScanDialog } from "../components/ScanDialog";
import { ShareDialog } from "../components/ShareDialog";
import { StageBadge } from "../components/StageBadge";
import { useToast } from "../components/Toast";
import { formatDate, plural } from "../lib/format";
import { errMessage, projectDelete, snapshotReimport, targetArchive, targetCreate, targetDelete, targetUpdate } from "../lib/ipc";
import { STAGES, type Stage, type Target } from "../types";
import { ProjectForm } from "./Projects";
import type { PageProps } from "./Workspace";

export default function ProjectDetail({ project, targets, reloadTargets, reloadProjects, onDeleted, goTo }: PageProps) {
  const toast = useToast();
  const [editProject, setEditProject] = useState(false);
  const [deleteProject, setDeleteProject] = useState(false);
  const [targetForm, setTargetForm] = useState<{ target?: Target } | null>(null);
  const [importing, setImporting] = useState<Target | null>(null);
  const [deleting, setDeleting] = useState<Target | null>(null);
  const [scanning, setScanning] = useState(false);
  const [sharing, setSharing] = useState<Target | null>(null);

  const active = targets.filter((t) => !t.isArchived);
  const archived = targets.filter((t) => t.isArchived);

  async function reimport(t: Target) {
    try {
      const r = await snapshotReimport(t.id);
      if (r.unchanged) toast(`${t.branch} / ${t.stage}: the file has not changed since the last import, so nothing was added.`, "info");
      else toast(`Re-imported version ${r.snapshot?.version}`, "success");
      await reloadTargets();
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }

  async function archive(t: Target, value: boolean) {
    try {
      await targetArchive(t.id, value);
      await reloadTargets();
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }

  return (
    <div className="mx-auto max-w-[1000px] p-5">
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <button className="btn btn-primary" onClick={() => setTargetForm({})}>
          <IconPlus /> Add target
        </button>
        <button className="btn" onClick={() => setScanning(true)}>
          <IconFolder /> Scan folder
        </button>
        <span className="ml-auto flex gap-2">
          <button className="btn" onClick={() => setEditProject(true)}>
            Edit project
          </button>
          <button className="btn btn-danger" onClick={() => setDeleteProject(true)}>
            Delete project
          </button>
        </span>
      </div>

      {targets.length === 0 ? (
        <div className="panel p-8 text-center">
          <h2 className="mb-1 text-[15px] font-semibold">No targets yet</h2>
          <p className="mx-auto mb-4 max-w-[440px] text-[13px] text-muted">
            A target is one deployed thing: a branch plus a stage, like main on production. Add targets, then import each one's .env file. Scan folder can do both in one step.
          </p>
          <div className="flex justify-center gap-2">
            <button className="btn btn-primary" onClick={() => setTargetForm({})}>
              Add target
            </button>
            <button className="btn" onClick={() => setScanning(true)}>
              Scan folder
            </button>
          </div>
        </div>
      ) : (
        <>
          <TargetTable
            targets={active}
            onImport={setImporting}
            onReimport={reimport}
            onEdit={(t) => setTargetForm({ target: t })}
            onArchive={(t) => archive(t, true)}
            onDelete={setDeleting}
            onHistory={() => goTo("history")}
            onShare={setSharing}
          />
          {archived.length > 0 && (
            <details className="mt-4">
              <summary className="cursor-pointer text-[13px] text-muted">Archived targets ({archived.length})</summary>
              <div className="mt-2">
                <TargetTable
                  targets={archived}
                  onImport={setImporting}
                  onReimport={reimport}
                  onEdit={(t) => setTargetForm({ target: t })}
                  onArchive={(t) => archive(t, false)}
                  archivedView
                  onDelete={setDeleting}
                  onHistory={() => goTo("history")}
                  onShare={setSharing}
                />
              </div>
            </details>
          )}
        </>
      )}

      <KeyNotes project={project} />

      {sharing && sharing.currentSnapshotId !== null && (
        <ShareDialog preselect={{ snapshotId: sharing.currentSnapshotId, label: `${sharing.branch} / ${sharing.stage} v${sharing.currentVersion}` }} onClose={() => setSharing(null)} />
      )}
      {targetForm && (
        <TargetForm
          projectId={project.id}
          target={targetForm.target}
          onClose={() => setTargetForm(null)}
          onSaved={async () => {
            setTargetForm(null);
            await Promise.all([reloadTargets(), reloadProjects()]);
          }}
        />
      )}
      {importing && (
        <ImportDialog
          target={importing}
          onClose={() => setImporting(null)}
          onDone={async () => {
            setImporting(null);
            await reloadTargets();
          }}
        />
      )}
      {scanning && (
        <ScanDialog
          project={project}
          targets={targets}
          onClose={() => setScanning(false)}
          onDone={async () => {
            await Promise.all([reloadTargets(), reloadProjects()]);
          }}
        />
      )}
      {editProject && (
        <ProjectForm
          project={project}
          onClose={() => setEditProject(false)}
          onSaved={async () => {
            setEditProject(false);
            await reloadProjects();
          }}
        />
      )}
      {deleteProject && (
        <Confirm
          title="Delete project"
          danger
          confirmLabel="Delete project"
          onClose={() => setDeleteProject(false)}
          onConfirm={async () => {
            try {
              await projectDelete(project.id);
              toast(`Deleted ${project.name}`, "success");
              onDeleted();
            } catch (e) {
              toast(errMessage(e), "error");
            }
          }}
        >
          <p>
            This permanently deletes <strong>{project.name}</strong>, its {plural(project.targetCount, "target")} and every snapshot. It cannot be undone. Your .env files on disk are not touched.
          </p>
        </Confirm>
      )}
      {deleting && (
        <Confirm
          title="Delete target"
          danger
          confirmLabel="Delete target"
          onClose={() => setDeleting(null)}
          onConfirm={async () => {
            try {
              await targetDelete(deleting.id);
              toast(`Deleted ${deleting.branch} / ${deleting.stage}`, "success");
              setDeleting(null);
              await Promise.all([reloadTargets(), reloadProjects()]);
            } catch (e) {
              toast(errMessage(e), "error");
            }
          }}
        >
          <p>
            This permanently deletes <strong>{deleting.branch} / {deleting.stage}</strong> and its {plural(deleting.snapshotCount, "snapshot")}. It cannot be undone. Archive it instead to hide it from compare and push checks.
          </p>
        </Confirm>
      )}
    </div>
  );
}

interface TableProps {
  targets: Target[];
  archivedView?: boolean;
  onImport: (t: Target) => void;
  onReimport: (t: Target) => void;
  onEdit: (t: Target) => void;
  onArchive: (t: Target) => void;
  onDelete: (t: Target) => void;
  onHistory: () => void;
  onShare: (t: Target) => void;
}

function TargetTable({ targets, archivedView, onImport, onReimport, onEdit, onArchive, onDelete, onHistory, onShare }: TableProps) {
  return (
    <div className="panel overflow-x-auto">
      <table className="data-table">
        <thead>
          <tr>
            <th>Stage</th>
            <th>Branch</th>
            <th>Version</th>
            <th>Keys</th>
            <th>Last imported</th>
            <th>
              <span className="sr-only">Actions</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {targets.map((t) => (
            <tr key={t.id}>
              <td>
                <StageBadge stage={t.stage} />
              </td>
              <td>
                <span className="mono">{t.branch}</span>
                {t.label && <span className="ml-2 text-[12px] text-muted">{t.label}</span>}
              </td>
              <td>
                {t.currentVersion !== null ? (
                  <button className="btn btn-quiet btn-sm" onClick={onHistory} title="Open history">
                    v{t.currentVersion}
                  </button>
                ) : (
                  <span className="text-muted">none</span>
                )}
              </td>
              <td className="text-right tabular-nums">{t.keyCount}</td>
              <td className="text-muted">{formatDate(t.lastImported)}</td>
              <td>
                <div className="flex flex-wrap justify-end gap-1">
                  <button className="btn btn-sm btn-primary" onClick={() => onImport(t)}>
                    Import .env
                  </button>
                  {t.snapshotCount > 0 && (
                    <button className="btn btn-sm" onClick={() => onReimport(t)}>
                      Re-import
                    </button>
                  )}
                  {t.currentSnapshotId !== null && (
                    <button className="btn btn-sm" onClick={() => onShare(t)}>
                      Share
                    </button>
                  )}
                  <button className="btn btn-sm" onClick={() => onEdit(t)}>
                    Edit
                  </button>
                  <button className="btn btn-sm" onClick={() => onArchive(t)}>
                    {archivedView ? "Unarchive" : "Archive"}
                  </button>
                  <button className="btn btn-sm btn-danger" onClick={() => onDelete(t)}>
                    Delete
                  </button>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function TargetForm({ projectId, target, onClose, onSaved }: { projectId: number; target?: Target; onClose: () => void; onSaved: () => void | Promise<void> }) {
  const toast = useToast();
  const [branch, setBranch] = useState(target?.branch ?? "main");
  const [stage, setStage] = useState<Stage>(target?.stage ?? "production");
  const [label, setLabel] = useState(target?.label ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!branch.trim()) return setError("Give the target a branch name, for example `main`.");
    setBusy(true);
    setError(null);
    try {
      if (target) await targetUpdate(target.id, branch, stage, label || null, target.color);
      else await targetCreate(projectId, branch, stage, label || null);
      toast(target ? "Target saved" : "Target added. Import its .env file next.", "success");
      await onSaved();
    } catch (err) {
      setError(errMessage(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      title={target ? "Edit target" : "Add target"}
      onClose={onClose}
      width={460}
      footer={
        <>
          <button className="btn" onClick={onClose} disabled={busy}>
            Cancel
          </button>
          <button className="btn btn-primary" type="submit" form="target-form" disabled={busy}>
            {target ? "Save target" : "Add target"}
          </button>
        </>
      }
    >
      <form id="target-form" onSubmit={submit} className="flex flex-col gap-3">
        <div>
          <label className="label" htmlFor="t-branch">
            Branch
          </label>
          <input id="t-branch" className="input mono" value={branch} onChange={(e) => setBranch(e.target.value)} placeholder="main" />
        </div>
        <div>
          <label className="label" htmlFor="t-stage">
            Stage
          </label>
          <select id="t-stage" className="input" value={stage} onChange={(e) => setStage(e.target.value as Stage)}>
            {STAGES.map((s) => (
              <option key={s}>{s}</option>
            ))}
          </select>
        </div>
        <div>
          <label className="label" htmlFor="t-label">
            Label (optional)
          </label>
          <input id="t-label" className="input" value={label} onChange={(e) => setLabel(e.target.value)} placeholder="Heroku prod" />
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
