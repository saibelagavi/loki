// The ONLY place invoke() is called. Every backend command gets a typed wrapper here.
// Argument keys are camelCase; Tauri maps them to the snake_case Rust parameters.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  CatalogEntry,
  Credential,
  CredentialInput,
  DiffSource,
  DriftRow,
  Overview,
  ShareCreated,
  ShareMapping,
  SharePreview,
  ShareSelection,
  ValueDiffResult,
  AppInfo,
  AuditEntry,
  CompareResult,
  CustomRule,
  DiffResult,
  ExportMode,
  ImportSummary,
  KeyInfo,
  KeyNote,
  Preview,
  Project,
  PushResult,
  ReimportResult,
  RuleInfo,
  ScanHit,
  SearchGroup,
  Settings,
  Snapshot,
  Stage,
  Target,
  VaultStatus,
} from "../types";

const call = <T>(cmd: string, args?: Record<string, unknown>) => invoke<T>(cmd, args);

/** Backend errors arrive as plain strings that never contain secret values. */
export function errMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Something went wrong.";
}

export const inTauri = () => "__TAURI_INTERNALS__" in window;

// ---- app & vault
export const appInfo = () => call<AppInfo>("app_info");
export const vaultStatus = () => call<VaultStatus>("vault_status");
export const vaultCreate = (password: string) => call<void>("vault_create", { password });
/** Resolves to the path of a safety copy when unlocking upgraded the vault, else null. */
export const vaultUnlock = (password: string) => call<string | null>("vault_unlock", { password });
export const vaultLock = () => call<void>("vault_lock");
export const vaultChangePassword = (oldPassword: string, newPassword: string) =>
  call<void>("vault_change_password", { oldPassword, newPassword });
export const vaultTouch = () => call<void>("vault_touch");

// ---- projects & targets
export const projectList = () => call<Project[]>("project_list");
export const projectCreate = (name: string, rootPath: string | null, description: string | null) =>
  call<Project>("project_create", { name, rootPath, description });
export const projectUpdate = (id: number, name: string, rootPath: string | null, description: string | null) =>
  call<Project>("project_update", { id, name, rootPath, description });
export const projectDelete = (id: number) => call<void>("project_delete", { id });

export const targetList = (projectId: number) => call<Target[]>("target_list", { projectId });
export const targetCreate = (projectId: number, branch: string, stage: Stage, label: string | null, color: string | null = null) =>
  call<Target>("target_create", { projectId, branch, stage, label, color });
export const targetUpdate = (id: number, branch: string, stage: Stage, label: string | null, color: string | null = null) =>
  call<Target>("target_update", { id, branch, stage, label, color });
export const targetArchive = (id: number, archived: boolean) => call<Target>("target_archive", { id, archived });
export const targetDelete = (id: number) => call<void>("target_delete", { id });

// ---- scan & import (values are never returned by preview)
export const scanFolder = (path: string) => call<ScanHit[]>("scan_folder", { path });
export const importPreview = (source: { path?: string; text?: string }) =>
  call<Preview>("import_preview", { path: source.path ?? null, text: source.text ?? null });
export const importCommit = (targetId: number, source: { path?: string; text?: string }, note: string | null) =>
  call<Snapshot>("import_commit", { targetId, path: source.path ?? null, text: source.text ?? null, note });
export const snapshotReimport = (targetId: number) => call<ReimportResult>("snapshot_reimport", { targetId });

// ---- snapshots
export const snapshotList = (targetId: number) => call<Snapshot[]>("snapshot_list", { targetId });
export const snapshotKeys = (snapshotId: number) => call<KeyInfo[]>("snapshot_keys", { snapshotId });
export const snapshotDiff = (aId: number, bId: number) => call<DiffResult>("snapshot_diff", { aId, bId });
export const snapshotRestore = (snapshotId: number, note: string | null) =>
  call<Snapshot>("snapshot_restore", { snapshotId, note });

// ---- compare & push check
export const compareMatrix = (targetIds: number[], baselineId: number) =>
  call<CompareResult>("compare_matrix", { targetIds, baselineId });
export const pushCheck = (fromTarget: number, toTarget: number) => call<PushResult>("push_check", { fromTarget, toTarget });
export const pushCheckExport = (fromTarget: number, toTarget: number) =>
  call<string | null>("push_check_export", { fromTarget, toTarget });

// ---- reveal, copy, export (audited in the core)
export const revealValue = (variableId: number) => call<string>("reveal_value", { variableId });
/** Resolves to the number of seconds until the clipboard is cleared. */
export const copyValue = (variableId: number) => call<number>("copy_value", { variableId });
/** Resolves to the chosen path, or null if the save dialog was cancelled. */
export const exportEnv = (snapshotId: number, mode: ExportMode, compareSnapshotId: number | null = null) =>
  call<string | null>("export_env", { snapshotId, mode, compareSnapshotId });

// ---- search, notes, audit, settings, backup
export const searchKeys = (query: string) => call<SearchGroup[]>("search_keys", { query });
export const projectKeys = (projectId: number) => call<string[]>("project_keys", { projectId });
export const keyNoteList = (projectId: number) => call<KeyNote[]>("key_note_list", { projectId });
export const keyNoteSet = (
  projectId: number,
  key: string,
  description: string | null,
  requiredStages: Stage[],
  isSecret: boolean,
  lastRotated: string | null,
) => call<void>("key_note_set", { projectId, key, description, requiredStages, isSecret, lastRotated });
export const auditList = (limit = 200) => call<AuditEntry[]>("audit_list", { limit });
export const rulesCatalog = () => call<RuleInfo[]>("rules_catalog");
export const settingsGet = () => call<Settings>("settings_get");
export const settingsSet = (settings: Settings) => call<Settings>("settings_set", { settings });
export const backupExport = (backupPassword: string) => call<string | null>("backup_export", { backupPassword });
export const backupImport = (backupPassword: string) => call<ImportSummary | null>("backup_import", { backupPassword });

export type { CustomRule };

// ---- v2
export const targetCatalog = () => call<CatalogEntry[]>("target_catalog");
/** Copies generated text (a new password, a pass key) securely; resolves to the clear timeout. */
export const clipboardCopyText = (text: string) => call<number>("clipboard_copy_text", { text });

/** Statuses only, unless revealAll or revealKeys ask for values (audited in the core). */
export const valueDiff = (left: DiffSource, right: DiffSource, revealAll = false, revealKeys: string[] = []) =>
  call<ValueDiffResult>("value_diff", { left, right, revealAll, revealKeys });

export const credentialList = () => call<Credential[]>("credential_list");
export const credentialCreate = (input: CredentialInput) => call<Credential>("credential_create", { input });
export const credentialUpdate = (id: number, input: CredentialInput) => call<Credential>("credential_update", { id, input });
export const credentialDelete = (id: number) => call<void>("credential_delete", { id });
export const credentialReveal = (id: number, field: "password" | "notes") => call<string>("credential_reveal", { id, field });
export const credentialCopy = (id: number, field: "password" | "notes") => call<number>("credential_copy", { id, field });
export const passwordGenerate = (length: number, symbols: boolean) => call<string>("password_generate", { length, symbols });

/** A fresh LOKI-generated pass key (shown to the sender before the file is created). */
export const shareGenerateKey = () => call<string>("share_generate_key");
/** Resolves to null if the save dialog was cancelled. */
export const shareCreate = (selections: ShareSelection[], passKey: string, expiresDays: number | null, note: string | null) =>
  call<ShareCreated | null>("share_create", { selections, passKey, expiresDays, note });
export const shareOpen = (path: string, passKey: string) => call<SharePreview>("share_open", { path, passKey });
export const shareImport = (token: string, mappings: ShareMapping[], note: string | null) => call<Snapshot[]>("share_import", { token, mappings, note });
/** Saves one .env of the open share; resolves to null if the save dialog was cancelled. */
export const shareSaveEnv = (token: string, index: number) => call<string | null>("share_save_env", { token, index });
export const shareDiscard = () => call<void>("share_discard");

export const overview = () => call<Overview>("overview");
export const driftCheck = () => call<DriftRow[]>("drift_check");

// ---- native pickers and events (no secret values pass through these)
export async function pickFile(): Promise<string | null> {
  const r = await open({ multiple: false, directory: false, title: "Choose a .env file" });
  return typeof r === "string" ? r : null;
}

export async function pickFolder(): Promise<string | null> {
  const r = await open({ multiple: false, directory: true, title: "Choose a project folder" });
  return typeof r === "string" ? r : null;
}

/** Fires when the core locks the vault (idle timeout or window minimize). Returns an unsubscribe. */
export async function onVaultLocked(cb: (reason: string) => void): Promise<() => void> {
  return listen<string>("vault-locked", (e) => cb(e.payload));
}

/** Files dropped onto the window. Returns an unsubscribe. */
export async function onFilesDropped(cb: (paths: string[]) => void): Promise<() => void> {
  return getCurrentWebview().onDragDropEvent((e) => {
    if (e.payload.type === "drop") cb(e.payload.paths);
  });
}
