// The unlocked app: rail (tools + projects), the selected view, and lazily loaded pages.
import { lazy, Suspense, useCallback, useEffect, useState } from "react";
import { OpenShareDialog } from "../components/OpenShareDialog";
import { ShareDialog } from "../components/ShareDialog";
import { IconPlus } from "../components/icons";
import { useToast } from "../components/Toast";
import { errMessage, projectList, settingsGet, targetList, vaultLock } from "../lib/ipc";
import { SettingsContext, DEFAULT_SETTINGS } from "../lib/settingsContext";
import type { Project, Settings, Target } from "../types";
import { ProjectForm, ProjectRail, type RailView } from "./Projects";

const ProjectDetail = lazy(() => import("./ProjectDetail"));
const Compare = lazy(() => import("./Compare"));
const History = lazy(() => import("./History"));
const PushCheck = lazy(() => import("./PushCheck"));
const SettingsPage = lazy(() => import("./SettingsPage"));
const SearchDialog = lazy(() => import("./SearchDialog"));
const Overview = lazy(() => import("./Overview"));
const DiffChecker = lazy(() => import("./DiffChecker"));
const Passwords = lazy(() => import("./Passwords"));

export type Tab = "targets" | "compare" | "history" | "push";
const TABS: { id: Tab; label: string }[] = [
  { id: "compare", label: "Compare" },
  { id: "history", label: "History" },
  { id: "push", label: "Push check" },
  { id: "targets", label: "Targets" },
];

/** Ask the compare matrix to scroll to a key and highlight it (from search). */
export interface CompareFocus {
  key: string;
  targetId: number | null;
  nonce: number;
}

export interface PageProps {
  project: Project;
  targets: Target[];
  reloadTargets: () => Promise<void>;
  reloadProjects: () => Promise<void>;
  onDeleted: () => void;
  goTo: (tab: Tab) => void;
  focus: CompareFocus | null;
}

interface Props {
  onLocked: () => void;
}

const Loading = () => <p className="p-6 text-[13px] text-muted">Loading…</p>;

export default function Workspace({ onLocked }: Props) {
  const toast = useToast();
  const [projects, setProjects] = useState<Project[] | null>(null);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [targets, setTargets] = useState<Target[]>([]);
  const [tab, setTab] = useState<Tab>("compare");
  const [view, setView] = useState<RailView>("overview");
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [showSearch, setShowSearch] = useState(false);
  const [showNew, setShowNew] = useState(false);
  const [showOpenShare, setShowOpenShare] = useState(false);
  const [showShare, setShowShare] = useState(false);
  const [focus, setFocus] = useState<CompareFocus | null>(null);
  const [passwordFocus, setPasswordFocus] = useState<number | null>(null);

  const reloadProjects = useCallback(async () => {
    try {
      const list = await projectList();
      setProjects(list);
      setSelectedId((cur) => (cur !== null && list.some((p) => p.id === cur) ? cur : (list[0]?.id ?? null)));
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }, [toast]);

  const reloadTargets = useCallback(async () => {
    if (selectedId === null) return setTargets([]);
    try {
      setTargets(await targetList(selectedId));
    } catch (e) {
      toast(errMessage(e), "error");
    }
  }, [selectedId, toast]);

  useEffect(() => {
    void reloadProjects();
    settingsGet()
      .then(setSettings)
      .catch(() => undefined);
  }, [reloadProjects]);

  useEffect(() => {
    void reloadTargets();
  }, [reloadTargets]);

  const lock = useCallback(async () => {
    try {
      await vaultLock();
    } finally {
      onLocked();
    }
  }, [onLocked]);

  // Ctrl+K search, Ctrl+L lock.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
      const k = e.key.toLowerCase();
      if (k === "k") {
        e.preventDefault();
        setShowSearch(true);
      } else if (k === "l") {
        e.preventDefault();
        void lock();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [lock]);

  const project = projects?.find((p) => p.id === selectedId) ?? null;

  function openProject(id: number, t: Tab = tab) {
    if (id !== selectedId) setTargets([]);
    setSelectedId(id);
    setTab(t);
    setView("project");
  }

  const page: PageProps | null = project
    ? {
        project,
        targets,
        reloadTargets,
        reloadProjects,
        onDeleted: () => {
          setSelectedId(null);
          setView("overview");
          void reloadProjects();
        },
        goTo: setTab,
        focus,
      }
    : null;

  function renderMain() {
    if (view === "settings") return <SettingsPage onClose={() => setView("overview")} />;
    if (view === "overview")
      return (
        <Overview
          onOpenProject={(id, t) => openProject(id, t)}
          onOpenPassword={(id) => {
            setPasswordFocus(id);
            setView("passwords");
          }}
        />
      );
    if (view === "diff") return <DiffChecker />;
    if (view === "passwords") return <Passwords focusId={passwordFocus} />;
    if (projects === null) return <Loading />;
    if (page === null)
      return (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
          <h1 className="display text-[18px]">No projects yet</h1>
          <p className="max-w-[420px] text-[13px] text-muted">Add a project, then import its .env files. LOKI will show what differs between staging and production, and what is missing.</p>
          <button className="btn btn-primary" onClick={() => setShowNew(true)}>
            <IconPlus /> Add project
          </button>
        </div>
      );
    return (
      <>
        <header className="flex flex-wrap items-center gap-x-6 gap-y-2 border-b border-line px-5 pt-3">
          <div className="min-w-0 pb-2">
            <h1 className="display truncate text-[18px]">{page.project.name}</h1>
            {page.project.rootPath && <p className="mono truncate text-[12px] text-muted">{page.project.rootPath}</p>}
          </div>
          <div role="tablist" aria-label="Project views" className="ml-auto flex gap-1">
            {TABS.map((t) => (
              <button
                key={t.id}
                role="tab"
                aria-selected={tab === t.id}
                className={`-mb-px border-b-2 px-3 py-2 text-[13px] ${tab === t.id ? "border-focus font-medium" : "border-transparent text-muted hover:text-chalk"}`}
                onClick={() => setTab(t.id)}
              >
                {t.label}
              </button>
            ))}
          </div>
        </header>
        <div key={page.project.id} className="min-h-0 flex-1 overflow-auto" role="tabpanel">
          {tab === "compare" && <Compare {...page} />}
          {tab === "history" && <History {...page} />}
          {tab === "push" && <PushCheck {...page} />}
          {tab === "targets" && <ProjectDetail {...page} />}
        </div>
      </>
    );
  }

  return (
    <SettingsContext.Provider value={{ settings, setSettings }}>
      <div className="flex h-full">
        <ProjectRail
          projects={projects ?? []}
          selectedId={selectedId}
          view={view}
          onSelect={(id) => openProject(id)}
          onView={(v) => {
            if (v !== "passwords") setPasswordFocus(null);
            setView(v);
          }}
          onAdd={() => setShowNew(true)}
          onSearch={() => setShowSearch(true)}
          onShare={() => setShowShare(true)}
          onOpenShare={() => setShowOpenShare(true)}
          onLock={lock}
        />

        <main className={`flex min-w-0 flex-1 flex-col ${view === "project" ? "" : "overflow-auto"}`}>
          <Suspense fallback={<Loading />}>{renderMain()}</Suspense>
        </main>
      </div>

      {showNew && (
        <ProjectForm
          onClose={() => setShowNew(false)}
          onSaved={(p) => {
            setShowNew(false);
            void reloadProjects();
            openProject(p.id, "targets");
          }}
        />
      )}
      {showSearch && (
        <Suspense fallback={null}>
          <SearchDialog
            onClose={() => setShowSearch(false)}
            onOpenHit={(projectId, key, targetId) => {
              setShowSearch(false);
              openProject(projectId, "compare");
              setFocus({ key, targetId, nonce: Date.now() });
            }}
          />
        </Suspense>
      )}
      {showShare && <ShareDialog onClose={() => setShowShare(false)} />}
      {showOpenShare && (
        <OpenShareDialog
          onClose={() => setShowOpenShare(false)}
          onImported={(projectId) => {
            setShowOpenShare(false);
            void reloadProjects();
            if (projectId !== null) openProject(projectId, "compare");
            else setView("overview");
          }}
        />
      )}
    </SettingsContext.Provider>
  );
}
