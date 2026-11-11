// Mirrors the Rust DTOs. DTOs never carry secret values (only reveal_value returns one).

export type Stage = "local" | "dev" | "staging" | "preview" | "production" | "other";
export const STAGES: Stage[] = ["production", "staging", "preview", "dev", "local", "other"];

export interface AppInfo {
  name: string;
  version: string;
}

export interface VaultStatus {
  exists: boolean;
  unlocked: boolean;
}

export interface Project {
  id: number;
  name: string;
  rootPath: string | null;
  description: string | null;
  targetCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface Target {
  id: number;
  projectId: number;
  branch: string;
  stage: Stage;
  label: string | null;
  color: string | null;
  isArchived: boolean;
  currentSnapshotId: number | null;
  currentVersion: number | null;
  keyCount: number;
  snapshotCount: number;
  lastImported: string | null;
}

export interface Snapshot {
  id: number;
  targetId: number;
  version: number;
  note: string | null;
  sourcePath: string | null;
  createdAt: string;
  keyCount: number;
}

export interface KeyInfo {
  variableId: number;
  key: string;
  isEmpty: boolean;
  hasComment: boolean;
  /** First 8 hex characters of the fingerprint. Equal fingerprint means equal value. */
  fingerprint: string;
}

export interface KeyNote {
  key: string;
  description: string | null;
  requiredStages: Stage[];
  isSecret: boolean;
  lastRotated: string | null;
}

export interface AuditEntry {
  id: number;
  ts: string;
  action: string;
  entity: string;
  detail: string | null;
}

export interface SearchHit {
  projectId: number;
  projectName: string;
  targetId: number;
  branch: string;
  stage: Stage;
  isEmpty: boolean;
  fingerprint: string;
  variableId: number;
}

export interface SearchGroup {
  key: string;
  hits: SearchHit[];
  distinctValues: number;
  consistent: boolean;
}

export interface ScanHit {
  path: string;
  fileName: string;
  guessedStage: Stage;
  guessedBranch: string | null;
}

export interface PreviewKey {
  key: string;
  isEmpty: boolean;
  hasComment: boolean;
  line: number;
}

export interface Preview {
  keys: PreviewKey[];
  warnings: { line: number; message: string }[];
  keyCount: number;
  emptyCount: number;
  duplicateCount: number;
  sourcePath: string | null;
}

export interface ReimportResult {
  unchanged: boolean;
  sourcePath: string;
  snapshot: Snapshot | null;
}

export type CellStatus = "base" | "same" | "different" | "missing" | "empty" | "extra" | "none";

export interface MatrixColumn {
  targetId: number;
  branch: string;
  stage: Stage;
  label: string | null;
  color: string | null;
  snapshotId: number | null;
  version: number | null;
  keyCount: number;
}

export interface MatrixCell {
  status: CellStatus;
  variableId: number | null;
}

export interface MatrixRow {
  key: string;
  cells: MatrixCell[];
}

export interface CompareResult {
  columns: MatrixColumn[];
  baselineIndex: number;
  rows: MatrixRow[];
}

export interface DiffEntry {
  key: string;
  change: "added" | "removed" | "changed";
  aVariableId: number | null;
  bVariableId: number | null;
}

export interface DiffResult {
  entries: DiffEntry[];
  added: number;
  removed: number;
  changed: number;
}

export type Severity = "error" | "warning" | "info";

export interface Finding {
  severity: Severity;
  ruleId: string;
  key: string | null;
  message: string;
}

export interface PushResult {
  findings: Finding[];
  fromLabel: string;
  toLabel: string;
  toStage: Stage;
}

export interface RuleInfo {
  id: string;
  severity: Severity;
  title: string;
  description: string;
}

export interface CustomRule {
  id: string;
  keyPattern: string;
  op: "contains" | "starts_with";
  text: string;
  severity: Severity;
  productionOnly: boolean;
}

export interface Settings {
  diffRevealSeconds: number;
  rotationDays: number;
  autoLockMinutes: number;
  lockOnMinimize: boolean;
  clipboardClearSeconds: number;
  revealSeconds: number;
  excludeFromCapture: boolean;
  disabledRules: string[];
  customRules: CustomRule[];
}

export interface ImportSummary {
  projects: number;
  targets: number;
  snapshots: number;
  renamed: number;
  credentials: number;
}

export type ExportMode = "full" | "example" | "missing_only";

// ---- v2: catalog, diff checker, passwords, share, overview

export interface CatalogEntry {
  project: Project;
  targets: Target[];
}

export type DiffSource =
  | { kind: "snapshot"; snapshotId: number }
  | { kind: "target"; targetId: number }
  | { kind: "file"; path: string }
  | { kind: "text"; text: string };

export type ValueStatus = "same" | "changed" | "added" | "removed";

export interface ValueRow {
  key: string;
  status: ValueStatus;
  leftEmpty: boolean | null;
  rightEmpty: boolean | null;
  /** Value-free explanation for a change, e.g. "only whitespace differs". */
  hint: string | null;
  /** Present only for keys the user asked to reveal. */
  leftValue: string | null;
  rightValue: string | null;
}

export interface ValueDiffResult {
  leftLabel: string;
  rightLabel: string;
  leftWarnings: number;
  rightWarnings: number;
  rows: ValueRow[];
  counts: { same: number; changed: number; added: number; removed: number };
  revealSeconds: number;
}

export interface Credential {
  id: number;
  title: string;
  username: string;
  url: string;
  tags: string[];
  projectId: number | null;
  projectName: string | null;
  hasNotes: boolean;
  isWeak: boolean;
  isReused: boolean;
  createdAt: string;
  updatedAt: string;
  passwordChangedAt: string;
  ageDays: number;
}

export interface CredentialInput {
  title: string;
  username: string;
  url: string;
  tags: string[];
  projectId: number | null;
  /** null = keep the stored password (on update). */
  password: string | null;
  /** null = keep the stored notes (on update). */
  notes: string | null;
}

export interface ShareCreated {
  path: string;
  passKey: string;
  keyCount: number;
  expiresAt: string | null;
}

export interface SharePreview {
  token: string;
  keys: { key: string; isEmpty: boolean; hasComment: boolean }[];
  createdAt: string;
  expiresAt: string | null;
  expired: boolean;
  source: string;
  note: string | null;
}

export interface HealthCheck {
  fromTargetId: number;
  fromLabel: string;
  toTargetId: number;
  toLabel: string;
  errors: number;
  warnings: number;
  info: number;
}

export interface ProjectHealth {
  id: number;
  name: string;
  targetCount: number;
  stages: Stage[];
  lastImported: string | null;
  daysSinceImport: number | null;
  check: HealthCheck | null;
  checkError: string | null;
}

export interface RotationItem {
  kind: "key" | "password";
  projectId: number | null;
  projectName: string | null;
  name: string;
  lastRotated: string;
  ageDays: number;
  credentialId: number | null;
}

export interface Overview {
  projects: ProjectHealth[];
  rotation: RotationItem[];
  rotationDays: number;
  totals: { projects: number; targets: number; keys: number; passwords: number; weakPasswords: number; reusedPasswords: number };
}

export interface DriftRow {
  projectId: number;
  projectName: string;
  targetId: number;
  branch: string;
  stage: Stage;
  path: string;
  status: "unchanged" | "changed" | "missing" | "unreadable";
  lastImported: string | null;
}
