CREATE TABLE vault_meta (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  kdf_salt BLOB NOT NULL,
  kdf_m_cost INTEGER, kdf_t_cost INTEGER, kdf_p_cost INTEGER,
  verifier_nonce BLOB NOT NULL, verifier_ct BLOB NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE projects (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  root_path TEXT, description TEXT,
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);

CREATE TABLE targets (
  id INTEGER PRIMARY KEY,
  project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  branch TEXT NOT NULL,
  stage TEXT NOT NULL CHECK (stage IN ('local','dev','staging','preview','production','other')),
  label TEXT, color TEXT,
  is_archived INTEGER NOT NULL DEFAULT 0,
  UNIQUE (project_id, branch, stage)
);

CREATE TABLE snapshots (
  id INTEGER PRIMARY KEY,
  target_id INTEGER NOT NULL REFERENCES targets(id) ON DELETE CASCADE,
  version INTEGER NOT NULL, note TEXT,
  source_path TEXT, source_sha256 BLOB,
  created_at TEXT NOT NULL,
  UNIQUE (target_id, version)
);

-- value_* and comment hold XChaCha20-Poly1305 ciphertext. comment is nonce(24) || ciphertext,
-- or NULL. Comments are encrypted because they sometimes contain values.
CREATE TABLE variables (
  id INTEGER PRIMARY KEY,
  snapshot_id INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
  key TEXT NOT NULL, position INTEGER NOT NULL,
  value_nonce BLOB NOT NULL, value_ct BLOB NOT NULL,
  fingerprint BLOB NOT NULL,
  is_empty INTEGER NOT NULL,
  comment BLOB,
  UNIQUE (snapshot_id, key)
);

CREATE TABLE key_notes (
  project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  key TEXT NOT NULL, description TEXT,
  required_stages TEXT,
  is_secret INTEGER NOT NULL DEFAULT 1,
  last_rotated TEXT,
  PRIMARY KEY (project_id, key)
);

CREATE TABLE audit_log (
  id INTEGER PRIMARY KEY,
  ts TEXT NOT NULL,
  action TEXT NOT NULL, entity TEXT NOT NULL, detail TEXT
);

-- JSON settings (auto-lock, clipboard timeout, rule toggles, custom rules). Never holds values.
CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE INDEX idx_variables_snapshot ON variables(snapshot_id);
CREATE INDEX idx_variables_key ON variables(key);
CREATE INDEX idx_snapshots_target ON snapshots(target_id, version);
CREATE INDEX idx_targets_project ON targets(project_id);
