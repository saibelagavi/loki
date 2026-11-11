// Per-project notes about a key name: what it is, which stages require it, whether it is secret.
import { useCallback, useEffect, useMemo, useState } from "react";
import { errMessage, keyNoteList, keyNoteSet, projectKeys } from "../lib/ipc";
import { STAGES, type KeyNote, type Project, type Stage } from "../types";
import { useToast } from "./Toast";

export function KeyNotes({ project }: { project: Project }) {
  const toast = useToast();
  const [keys, setKeys] = useState<string[]>([]);
  const [notes, setNotes] = useState<Map<string, KeyNote>>(new Map());
  const [editing, setEditing] = useState<string | null>(null);
  const [filter, setFilter] = useState("");

  const load = useCallback(async () => {
    try {
      const [k, n] = await Promise.all([projectKeys(project.id), keyNoteList(project.id)]);
      setKeys(k);
      setNotes(new Map(n.map((x) => [x.key, x])));
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }, [project.id, toast]);

  useEffect(() => {
    void load();
  }, [load]);

  // Notes can exist for keys that are not in any current snapshot; keep them visible.
  const all = useMemo(() => [...new Set([...keys, ...notes.keys()])].sort(), [keys, notes]);
  const shown = all.filter((k) => k.toLowerCase().includes(filter.trim().toLowerCase()));

  return (
    <section aria-labelledby="notes-h" className="mt-8">
      <div className="mb-2 flex items-end justify-between gap-3">
        <div>
          <h2 id="notes-h" className="text-[15px] font-semibold">
            Key notes
          </h2>
          <p className="text-[12px] text-muted">Mark a key as required for a stage and the push check will flag it when it is missing or empty.</p>
        </div>
        <input className="input max-w-[220px]" placeholder="Filter keys" value={filter} onChange={(e) => setFilter(e.target.value)} aria-label="Filter keys" />
      </div>

      {all.length === 0 ? (
        <p className="text-[13px] text-muted">Import a .env file first. Its keys will appear here.</p>
      ) : (
        <ul className="panel divide-y divide-line">
          {shown.map((k) => (
            <li key={k} className="px-3 py-2">
              <div className="flex items-center gap-3">
                <span className="mono text-[13px]">{k}</span>
                {notes.get(k)?.requiredStages.map((s) => (
                  <span key={s} className="chip">
                    required: {s}
                  </span>
                ))}
                {notes.get(k) && !notes.get(k)?.isSecret && <span className="chip">not secret</span>}
                {notes.get(k)?.description && <span className="truncate text-[12px] text-muted">{notes.get(k)?.description}</span>}
                <button className="btn btn-sm ml-auto" onClick={() => setEditing(editing === k ? null : k)} aria-expanded={editing === k}>
                  {notes.has(k) ? "Edit note" : "Add note"}
                </button>
              </div>
              {editing === k && (
                <NoteForm
                  project={project}
                  keyName={k}
                  note={notes.get(k)}
                  onSaved={() => {
                    setEditing(null);
                    void load();
                  }}
                />
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function NoteForm({ project, keyName, note, onSaved }: { project: Project; keyName: string; note?: KeyNote; onSaved: () => void }) {
  const toast = useToast();
  const [description, setDescription] = useState(note?.description ?? "");
  const [stages, setStages] = useState<Stage[]>(note?.requiredStages ?? []);
  const [secret, setSecret] = useState(note?.isSecret ?? true);
  const [rotated, setRotated] = useState(note?.lastRotated ?? "");
  const [busy, setBusy] = useState(false);

  async function save() {
    setBusy(true);
    try {
      await keyNoteSet(project.id, keyName, description.trim() || null, stages, secret, rotated || null);
      toast(`Note saved for ${keyName}`, "success");
      onSaved();
    } catch (e) {
      toast(errMessage(e), "error");
    } finally {
      setBusy(false);
    }
  }

  const toggle = (s: Stage) => setStages((cur) => (cur.includes(s) ? cur.filter((x) => x !== s) : [...cur, s]));

  return (
    <div className="mt-3 grid gap-3 rounded-md bg-ink p-3">
      <div>
        <label className="label" htmlFor={`d-${keyName}`}>
          What is this key?
        </label>
        <input id={`d-${keyName}`} className="input" value={description} onChange={(e) => setDescription(e.target.value)} />
      </div>
      <fieldset>
        <legend className="label">Required in these stages</legend>
        <div className="flex flex-wrap gap-3">
          {STAGES.map((s) => (
            <label key={s} className="flex items-center gap-1.5 text-[13px]">
              <input type="checkbox" checked={stages.includes(s)} onChange={() => toggle(s)} /> {s}
            </label>
          ))}
        </div>
      </fieldset>
      <div className="flex flex-wrap items-end gap-4">
        <label className="flex items-center gap-1.5 text-[13px]">
          <input type="checkbox" checked={secret} onChange={(e) => setSecret(e.target.checked)} /> This is a secret
        </label>
        <div>
          <label className="label" htmlFor={`r-${keyName}`}>
            Last rotated
          </label>
          <input id={`r-${keyName}`} type="date" className="input" value={rotated} onChange={(e) => setRotated(e.target.value)} />
        </div>
        <button className="btn btn-primary ml-auto" onClick={save} disabled={busy}>
          Save note
        </button>
      </div>
    </div>
  );
}
