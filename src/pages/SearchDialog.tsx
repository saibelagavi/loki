// Ctrl+K: where is REDIS_URL used, and is it consistent across projects and stages?
// Pick a hit (click, or arrows + Enter) to jump to that exact cell in the compare matrix.
import { useEffect, useMemo, useRef, useState } from "react";
import { Modal } from "../components/Modal";
import { StageBadge } from "../components/StageBadge";
import { plural } from "../lib/format";
import { errMessage, searchKeys } from "../lib/ipc";
import type { SearchGroup, SearchHit } from "../types";

interface Props {
  onClose: () => void;
  onOpenHit: (projectId: number, key: string, targetId: number) => void;
}

function Highlight({ text, query }: { text: string; query: string }) {
  const q = query.trim();
  const i = q ? text.toLowerCase().indexOf(q.toLowerCase()) : -1;
  if (i < 0) return <>{text}</>;
  return (
    <>
      {text.slice(0, i)}
      <mark className="hit">{text.slice(i, i + q.length)}</mark>
      {text.slice(i + q.length)}
    </>
  );
}

export default function SearchDialog({ onClose, onOpenHit }: Props) {
  const [query, setQuery] = useState("");
  const [groups, setGroups] = useState<SearchGroup[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [searched, setSearched] = useState(false);
  const [active, setActive] = useState(0);
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    const q = query.trim();
    if (!q) {
      setGroups([]);
      setSearched(false);
      return;
    }
    let cancelled = false;
    const t = window.setTimeout(() => {
      searchKeys(q)
        .then((g) => {
          if (cancelled) return;
          setGroups(g);
          setSearched(true);
          setError(null);
          setActive(0);
        })
        .catch((e) => !cancelled && setError(errMessage(e)));
    }, 120);
    return () => {
      cancelled = true;
      window.clearTimeout(t);
    };
  }, [query]);

  // Every hit in display order, for keyboard navigation.
  const flat = useMemo(() => groups.flatMap((g) => g.hits.map((h) => ({ key: g.key, hit: h }))), [groups]);

  useEffect(() => {
    listRef.current?.querySelector(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [active]);

  function open(key: string, h: SearchHit) {
    onOpenHit(h.projectId, key, h.targetId);
  }

  function onKeyDown(e: React.KeyboardEvent) {
    if (flat.length === 0) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => Math.min(flat.length - 1, a + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(0, a - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const f = flat[active];
      if (f) open(f.key, f.hit);
    }
  }

  let index = -1;
  return (
    <Modal title="Search keys across all projects" onClose={onClose} width={760}>
      <input
        className="input mb-3"
        autoFocus
        placeholder="Type a key name, for example REDIS_URL"
        aria-label="Key name"
        aria-controls="search-results"
        aria-activedescendant={flat.length ? `hit-${active}` : undefined}
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={onKeyDown}
      />
      {error && (
        <p className="text-[13px] text-prod" role="alert">
          {error}
        </p>
      )}
      {!query.trim() && <p className="text-[13px] text-muted">Searches key names in every project's current versions. Use the arrow keys and Enter to jump to the key in its compare matrix. Values are never shown here.</p>}
      {searched && groups.length === 0 && <p className="text-[13px] text-muted">No key matches “{query.trim()}”.</p>}

      <ul id="search-results" ref={listRef} role="listbox" aria-label="Matching keys" className="flex flex-col gap-3" aria-live="polite">
        {groups.map((g) => (
          <li key={g.key} className="rounded-md border border-line" role="presentation">
            <div className="flex flex-wrap items-center gap-2 border-b border-line px-3 py-2">
              <span className="mono text-[13px] font-medium">
                <Highlight text={g.key} query={query} />
              </span>
              <span className="text-[12px] text-muted">{plural(g.hits.length, "place")}</span>
              {g.hits.length > 1 &&
                (g.consistent ? (
                  <span className="chip tone-muted">= same value everywhere</span>
                ) : (
                  <span className="chip tone-staging">≠ {g.distinctValues > 1 ? `${g.distinctValues} different values` : "some are empty"}</span>
                ))}
            </div>
            <ul className="divide-y divide-line" role="presentation">
              {g.hits.map((h) => {
                index += 1;
                const i = index;
                const on = i === active;
                return (
                  <li key={h.variableId} role="presentation">
                    <button
                      id={`hit-${i}`}
                      data-index={i}
                      role="option"
                      aria-selected={on}
                      className={`flex w-full items-center gap-3 px-3 py-1.5 text-left text-[13px] hover:bg-ink ${on ? "bg-ink outline outline-2 -outline-offset-2 outline-focus" : ""}`}
                      onMouseEnter={() => setActive(i)}
                      onClick={() => open(g.key, h)}
                    >
                      <span className="w-[170px] truncate">{h.projectName}</span>
                      <span className="mono w-[130px] truncate text-[12px]">{h.branch}</span>
                      <StageBadge stage={h.stage} />
                      <span className="mono ml-auto text-[12px] text-muted">{h.isEmpty ? "∅ empty" : `#${h.fingerprint}`}</span>
                      <span className="text-[12px] text-muted" aria-hidden>
                        {on ? "Enter ↵" : ""}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          </li>
        ))}
      </ul>
    </Modal>
  );
}
