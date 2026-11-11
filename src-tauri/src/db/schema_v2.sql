-- v2: password store. Additive only: no existing table or row is changed.
--
-- `uid` is a random 16-byte id used in the AEAD associated data, so a ciphertext cannot be moved
-- to another row. `meta_*` holds title, username, URL and tags (shown in the list). `secret_*`
-- holds the password and notes (only decrypted on Reveal or Copy). `secret_fp` is an HMAC
-- fingerprint of the password, used to spot reused passwords without decrypting.
CREATE TABLE credentials (
  id INTEGER PRIMARY KEY,
  uid BLOB NOT NULL UNIQUE,
  project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL,
  meta_nonce BLOB NOT NULL, meta_ct BLOB NOT NULL,
  secret_nonce BLOB NOT NULL, secret_ct BLOB NOT NULL,
  secret_fp BLOB NOT NULL,
  is_weak INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  password_changed_at TEXT NOT NULL
);

CREATE INDEX idx_credentials_project ON credentials(project_id);
CREATE INDEX idx_credentials_fp ON credentials(secret_fp);
