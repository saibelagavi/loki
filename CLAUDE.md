# CLAUDE.md — LOKI (Local Only Key Inventory)

## What we are building

LOKI is a Windows desktop app that keeps an encrypted, offline inventory of `.env` files across
projects, branches and deployment stages, and answers one question fast on push day:
**"What is different between staging and production, and what is missing?"**

The developer pain it solves: on release day you open five `.env` files side by side and hunt
manually for missing keys, stale keys, and values that point at the wrong place (a test API key
in production, `localhost` in a prod URL). LOKI replaces that hunt with a compare matrix, version
history, and a pre-push checklist.

## Non-negotiable rules (read before every change)

1. **No network, ever.** The app must work with the network cable pulled. Do not add any crate,
   npm package, Tauri plugin, font, CDN link, analytics, crash reporter, auto-updater or telemetry
   that opens a socket. If a dependency pulls in `reqwest`, `hyper`, `ureq`, `tokio` net features
   or similar, reject it and find another way. Ask the user before adding ANY new dependency.
2. **Secret values never leave the Rust core in plaintext unless the user explicitly asks**
   (Reveal, Copy, Export). The frontend receives key names, statuses and fingerprints only.
3. **Never log, print, `dbg!`, panic-message, or snapshot-test a secret value.** Tests use obvious
   fake values (`fake_value_1`). No real `.env` files are ever committed to this repo.
4. **Nothing is written outside the app data folder** except files the user explicitly exports to
   a path they chose in a save dialog.
5. **The master password is never stored.** Only a salt, KDF parameters and a verifier.
6. Keep it boring and auditable: small dependency tree, explicit code, no clever magic.

## Tech stack

| Layer | Choice | Why |
|---|---|---|
| Shell | Tauri 2 | Small native Windows binary, no bundled browser runtime, strict CSP |
| Core | Rust (stable) | Crypto, parsing, diffing, storage all live here |
| Storage | SQLite via `rusqlite` (feature `bundled`) | Single local file, no server |
| Crypto | `argon2` (Argon2id), `chacha20poly1305` (XChaCha20-Poly1305), `hmac` + `sha2`, `zeroize`, `rand` (OsRng) | Well-known RustCrypto crates |
| UI | React + TypeScript + Vite + Tailwind | Bundled locally; system fonts only |
| Tests | `cargo test`, Vitest | |

Allowed Tauri plugins: `dialog` (open/save file pickers), `clipboard-manager`. Nothing else
without asking. Do NOT use `tauri-plugin-http`, `tauri-plugin-updater`, `tauri-plugin-shell`.

## Commands

```powershell
npm install
npm run tauri dev        # run app in dev
npm run tauri build      # produce Windows installer (.msi / .exe) in src-tauri/target/release/bundle
cd src-tauri; cargo test; cargo clippy -- -D warnings; cd ..
npm run test             # Vitest
npm run lint
npm run check:offline    # custom script, see "Offline guard"
npm run build:msix       # Microsoft Store package (unsigned; the Store signs it)
```

## Folder structure

```
loki/
├─ CLAUDE.md
├─ src/                          # React frontend (no secret logic here)
│  ├─ pages/  (Unlock, Projects, ProjectDetail, Compare, History, PushCheck, Settings)
│  ├─ components/
│  ├─ lib/ipc.ts                 # typed wrappers around invoke(); the ONLY place invoke() is called
│  └─ types.ts                   # mirrors Rust DTOs
├─ src-tauri/
│  ├─ tauri.conf.json            # CSP + capabilities locked down
│  ├─ capabilities/default.json
│  └─ src/
│     ├─ main.rs
│     ├─ commands/               # #[tauri::command] fns, thin; call into core
│     ├─ core/
│     │  ├─ vault.rs             # unlock, lock, key derivation, auto-lock timer
│     │  ├─ crypto.rs            # encrypt/decrypt/fingerprint, zeroize
│     │  ├─ parser.rs            # .env parsing + serializing
│     │  ├─ diff.rs              # snapshot diff + compare matrix
│     │  ├─ rules.rs             # push-day checks
│     │  ├─ scan.rs              # find .env* files in a chosen folder
│     │  └─ backup.rs            # encrypted .loki export/import
│     ├─ db/ (schema.sql, migrations.rs, repo.rs)
│     └─ error.rs                # errors that NEVER contain values
└─ scripts/check-offline.mjs
```

## Domain model

```
Project            e.g. "QSeverse API"
 └─ Target         one deployed thing = branch + stage
    │              e.g. (main, production), (staging, staging), (feature/x, preview)
    └─ Snapshot    one imported version of that target's .env (v1, v2, v3 …), immutable
       └─ Variable KEY + encrypted value + fingerprint + comment + position
KeyNote            per-project notes about a key name (what it is, required in which stages)
```

Snapshots are immutable. Editing or re-importing creates a new snapshot; the newest is "current".

### Schema (SQLite)

```sql
CREATE TABLE vault_meta (id INTEGER PRIMARY KEY CHECK (id = 1),
  kdf_salt BLOB NOT NULL, kdf_m_cost INTEGER, kdf_t_cost INTEGER, kdf_p_cost INTEGER,
  verifier_nonce BLOB NOT NULL, verifier_ct BLOB NOT NULL, created_at TEXT NOT NULL);

CREATE TABLE projects (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE,
  root_path TEXT, description TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);

CREATE TABLE targets (id INTEGER PRIMARY KEY,
  project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  branch TEXT NOT NULL,
  stage TEXT NOT NULL CHECK (stage IN ('local','dev','staging','preview','production','other')),
  label TEXT, color TEXT, is_archived INTEGER NOT NULL DEFAULT 0,
  UNIQUE (project_id, branch, stage));

CREATE TABLE snapshots (id INTEGER PRIMARY KEY,
  target_id INTEGER NOT NULL REFERENCES targets(id) ON DELETE CASCADE,
  version INTEGER NOT NULL, note TEXT,
  source_path TEXT, source_sha256 BLOB,      -- hash of the file bytes, to detect "file changed"
  created_at TEXT NOT NULL, UNIQUE (target_id, version));

CREATE TABLE variables (id INTEGER PRIMARY KEY,
  snapshot_id INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
  key TEXT NOT NULL, position INTEGER NOT NULL,
  value_nonce BLOB NOT NULL, value_ct BLOB NOT NULL,   -- XChaCha20-Poly1305
  fingerprint BLOB NOT NULL,                           -- HMAC-SHA256(fp_key, value)
  is_empty INTEGER NOT NULL, comment TEXT,
  UNIQUE (snapshot_id, key));

CREATE TABLE key_notes (project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  key TEXT NOT NULL, description TEXT, required_stages TEXT,  -- JSON array
  is_secret INTEGER NOT NULL DEFAULT 1, last_rotated TEXT,
  PRIMARY KEY (project_id, key));

CREATE TABLE audit_log (id INTEGER PRIMARY KEY, ts TEXT NOT NULL,
  action TEXT NOT NULL, entity TEXT NOT NULL, detail TEXT);   -- NEVER contains values
```

Key names are stored in plaintext so search and diff are fast; values and comments-with-values
are not. (Decision recorded; revisit only if the user asks.)

## Crypto design

- On first run the user sets a master password. Derive 64 bytes with **Argon2id**
  (m = 64 MiB, t = 3, p = 1, random 16-byte salt) and split into:
  `enc_key` (32 B) for XChaCha20-Poly1305 and `fp_key` (32 B) for HMAC fingerprints.
- Verifier: encrypt a fixed constant with `enc_key`; unlocking succeeds if it decrypts.
- Each value: fresh random 24-byte nonce, AAD = `"{snapshot_id}:{key}"` so ciphertexts can't be
  swapped between rows.
- **Fingerprints let us diff without decrypting**: equal fingerprint ⇒ equal value.
- Keys live only in memory inside a `Mutex<Option<VaultKeys>>`; `VaultKeys` implements
  `ZeroizeOnDrop`. Locking sets it to `None`.
- Auto-lock after N minutes idle (default 5) and optionally on window minimize.
- Changing the master password re-encrypts every row inside one transaction.
- DB file location: `%LOCALAPPDATA%\LOKI\loki.db`.

## IPC surface (the frontend can only call these)

```
vault_status, vault_create(password), vault_unlock(password), vault_lock, vault_change_password
project_list/create/update/delete
target_list(project_id), target_create/update/archive/delete
scan_folder(path) -> [{path, guessed_stage, guessed_branch}]
import_preview(path | pasted_text) -> {keys, warnings}      // values NOT returned
import_commit(target_id, source, note) -> snapshot
snapshot_list(target_id), snapshot_diff(a_id, b_id)
compare_matrix(target_ids[], baseline_id) -> rows of {key, cells[status]}
push_check(from_target, to_target) -> findings[]
reveal_value(variable_id) -> string          // audited, UI hides again after 10 s
copy_value(variable_id)                       // copies, clears clipboard after 30 s
export_env(snapshot_id, path, mode: full | example | missing_only)
search_keys(query) -> hits across all projects
backup_export(path, backup_password), backup_import(path, backup_password)
settings_get/set
```

DTOs never include `value` except the return of `reveal_value`.

## Feature map

### MVP (build in this order)

**M0 — Scaffold + offline guard.** Tauri 2 + React app starts on Windows. CSP set to
`default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; connect-src ipc: http://ipc.localhost`.
`scripts/check-offline.mjs` fails if `Cargo.lock` or `package-lock.json` contains forbidden
networking crates/packages (in the production build), or if any `http(s)://` URL appears in
`src/` outside comments. Wire it into `npm run check:offline`.

**M1 — Vault.** Create / unlock / lock screens, auto-lock timer, password strength hint.
Wrong password shows a generic error and adds a 1 s delay.

**M2 — Projects and targets.** Create a project (optionally with its local folder path), add
targets as (branch, stage). Stage gets a color: production red, staging amber, dev blue.

**M3 — Import.** Pick file, drag and drop, or paste text. "Scan folder" finds `.env`,
`.env.*` (skip `node_modules`, `.git`, `target`, `dist`, `venv`) and guesses the stage from the
file name (`.env.production` → production). If the folder is a git repo, read `.git/HEAD`
directly (no git binary, no network) to guess the branch. Preview shows keys, counts and parser
warnings before committing.

Parser must handle: `KEY=value`, `export KEY=value`, single and double quotes, `\n` escapes
inside double quotes, multi-line double-quoted values, `# comments` (full-line and after an
unquoted value), empty values, spaces around `=`, CRLF, UTF-8 BOM, duplicate keys (last wins,
plus a warning), invalid lines (warning, not crash). Serializer round-trips what it parses.

**M4 — Compare matrix (the core screen).** Rows = keys, columns = chosen targets, one column is
the baseline. Cell states: `same` as baseline, `different`, `missing`, `empty`, `extra`
(present here, absent in baseline). Filters: only problems, only missing, search by key.
Values stay masked; a cell can be revealed or copied individually. Sticky header, keyboard nav.

**M5 — Versions.** Each target lists its snapshots with version, date, note and key count.
Diff any two snapshots: added, removed, changed (changed shown via fingerprint, value only on
reveal). "Re-import" on a target re-reads its last `source_path`; if the file's SHA-256 is
unchanged, say so and skip. Restore = create a new snapshot copied from an old one.

**M6 — Push-day check.** Pick "from" (e.g. staging) and "to" (e.g. production) targets and get a
findings list, grouped by severity:
- Error: key in `from` missing in `to`; key marked required for that stage but missing or empty.
- Warning: value in production contains `localhost`, `127.0.0.1`, `0.0.0.0`, `example.com`,
  `changeme`, `xxx`, `todo`, `test`; production value starts with a known test-key prefix
  (`sk_test_`, `pk_test_`, `rk_test_`); a secret has the SAME fingerprint in production and a
  non-production target (shared secret across stages); `DEBUG=true`/`NODE_ENV=development` in
  production.
- Info: key only in `to` (possibly stale).
Rules live in `rules.rs` as data, user can toggle each rule in Settings and add simple custom
rules (key pattern + value contains / starts with). Export the findings as a Markdown checklist
(keys only, never values).

**M7 — Export.** Write a snapshot as a full `.env`, a `.env.example` (keys + comments, values
blank), or a "missing only" block to paste into the deploy platform. Always via save dialog.

### v1 polish

Global key search across all projects ("where is `REDIS_URL` used and is it consistent?"),
per-key notes (description, required stages, last rotated), encrypted backup file `.loki`
(own password, Argon2id + XChaCha20-Poly1305) with import, local audit log viewer,
dark/light theme, settings for auto-lock and clipboard timeout, Windows option to exclude the
window from screen capture (`SetWindowDisplayAffinity` with `WDA_EXCLUDEFROMCAPTURE`).

### v2 (built, requested by the user)

- **Overview (landing page).** Release readiness for every project (the push check from staging,
  or the closest stage, to production, as counts), drift (re-hash each target's source file on
  disk, on demand only, with one-click re-import) and rotation reminders (secret keys with a
  last-rotated date and saved passwords older than `rotationDays`, default 90).
- **Diff checker.** Compare real values between any two sources: a target's current version, a
  specific version, a file on disk, or pasted text, across projects. Equality and value-free hints
  ("only whitespace differs", "only a trailing slash differs") come from the core. Values are
  returned only for keys the user reveals (or "all"), audited as `diff_reveal`, shown with a
  character-level diff, and dropped after `diffRevealSeconds` (default 60).
- **Passwords.** Credentials that are not in a .env (consoles, registrars, CI). Table
  `credentials` (schema v2): `meta` blob (title, username, URL, tags) and `secret` blob
  (password, notes), both XChaCha20-Poly1305 with AAD `cred:{uid}:meta|secret`. `secret_fp`
  flags reused passwords without decrypting; weak passwords are flagged. Generator uses OsRng and
  skips look-alike characters. Included in `.loki` backups and in change-password re-encryption.
- **Search jump.** A search hit opens the project's compare matrix, adds the target column if
  needed, centres the row, moves the cursor to that cell and flashes it. Matches are highlighted;
  the search list works with arrow keys and Enter.
- **Secure share (`.lokishare`).** A three-step wizard (choose, pass key, send) bundles whole
  projects, single `.env` files, or some keys of a file into one file (payload format 2: a list of
  items, each with project, branch and stage; format 1 single-file shares still open). The pass key
  is generated by LOKI (`share_generate_key`, 25 Crockford base32 characters, 125 bits), shown
  before the file is created, and `share_create` refuses anything that is not in that format.
  Creating the file requires confirming the key was copied. Same file format as backups with
  magic `LOKISHR1`; optional expiry inside the payload, enforced by LOKI on import. The recipient
  maps each `.env` to a target (default: the matching project and target, created if missing);
  `share_import` validates every mapping before writing and adds new versions only.
- **Data safety on upgrade.** Before migrating an existing vault to a newer schema, the core
  writes a full consistent copy with `VACUUM INTO` to `%LOCALAPPDATA%\LOKI\backups\`. Migrations
  are additive and transactional. The unlock screen reports the backup file.

IPC added: `target_catalog`, `clipboard_copy_text`, `value_diff(left, right, reveal_all,
reveal_keys)`, `credential_list/create/update/delete/reveal/copy`, `password_generate`,
`share_create/open/import/save_env/discard`, `overview`, `drift_check`. `vault_unlock` now
returns the backup path when it upgraded the schema.

### v2 ideas (do not build unless asked)

Folder watch that flags "your local .env changed since last import", a `loki` CLI
(`loki diff staging production --project api`) reading the same vault, Windows Hello unlock via
DPAPI-wrapped key, key-rotation reminders, compare against a `.env.example` as the contract.

## Performance budget (LOKI must feel instant)

Targets on a mid-range Windows laptop, measured on a release build:

| Metric | Budget |
|---|---|
| Double-click to window visible (cold start) | under 1 s |
| Unlock (Argon2id) | about 0.5 s, with a progress state, never a frozen window |
| Installer size (without bundled WebView2) | under 15 MB |
| Frontend JS, gzipped | under 250 KB total, initial route under 120 KB |
| Compare matrix with 500 keys × 6 targets | scrolls at 60 fps |

How to hit it:
- `Cargo.toml` release profile: `opt-level = "s"`, `lto = true`, `codegen-units = 1`,
  `strip = true`, `panic = "abort"`.
- Do almost nothing before the first paint. On startup only read `vault_meta`; open the full DB,
  run migrations and derive keys only after unlock. No folder scans or file hashing at startup.
- Create the window with `visible: false` and the app's background color, then show it from the
  frontend after the first render, so there is no white flash.
- No UI component kit, no icon pack, no chart library, no animation library, no web fonts
  loaded from the network (Exo 2 is a bundled 40 KB asset).
  Icons are a handful of inline SVGs in `src/components/icons.tsx`.
- Lazy-load every page except Unlock with `React.lazy`.
- Virtualize the compare matrix rows (write a small windowing hook; ask before adding a library).
- Heavy work (scan, import, diff, re-encrypt) runs in Rust on a background thread; the UI shows
  progress and never blocks.
- Add `npm run size` that fails if the built JS exceeds the budget, and log cold-start time in
  dev builds only.

## Visual design

The look is calm and precise: a quiet workspace where the only loud colors are the stage colors,
because stage is the information that matters on push day. Spend boldness on the compare matrix;
keep everything else disciplined. Do not use Marvel names, characters, imagery or styling
anywhere in the app, icon, README or store listing; LOKI's identity is its own.

### Tokens

| Token | Dark | Light | Use |
|---|---|---|---|
| `--ink` (background) | `#16132F` | `#F6F5FB` | app background |
| `--surface` | `#211D45` | `#FFFFFF` | panels, matrix |
| `--line` | `#34305E` | `#E1DFEE` | borders, grid lines |
| `--chalk` (text) | `#ECEAF6` | `#1C1A33` | primary text |
| `--muted` | `#A39FC4` | `#6B6790` | secondary text, "same" cells |
| `--prod` | `#F25F5C` | `#CE3835` | production |
| `--staging` | `#F2A541` | `#9D630D` | staging |
| `--dev` | `#4EA8DE` | `#1D75AF` | dev and local |
| `--focus` | `#B9B2FF` | `#5B4FD6` | keyboard focus ring |

Default theme follows Windows (`prefers-color-scheme`), with a manual override in Settings.
Check every text/background pair for WCAG AA contrast.

### Type

Brand and headings: **Exo 2** SemiBold / Bold (`.wordmark`, `.display`), bundled with the app as
`src/assets/fonts/exo2-latin-600-700.woff2` (40 KB variable font, SIL OFL, license next to it).
It is never fetched from the network. Interface text stays on system fonts:
`"Segoe UI Variable", "Segoe UI", system-ui, sans-serif`; `"Cascadia Mono", Consolas, monospace`
for key names, values and paths, since those are the content developers scan.
Scale: 12 / 13 / 15 / 18 / 24 px. Sentence case everywhere; no all-caps labels.

### Layout

```
┌──────────┬────────────────────────────────────────────────────────┐
│ Projects │ QSeverse API          [Compare] [History] [Push check] │
│  ● api   ├─────────────────┬────────────┬────────────┬────────────┤
│  ○ web   │ Key             │ main/prod  │ staging    │ dev        │
│  ○ admin │ DATABASE_URL    │ ●  base    │ ≠ differs  │ ≠ differs  │
│          │ REDIS_URL       │ ●  base    │ = same     │ ○ missing  │
│ + Add    │ STRIPE_KEY      │ ●  base    │ ≠ differs  │ ∅ empty    │
└──────────┴─────────────────┴────────────┴────────────┴────────────┘
```

Left rail lists projects. The main area is the compare matrix with a sticky monospace key column
and sticky stage headers, each header carrying its stage color as a thin top bar. Cell states use
a symbol and a color together (never color alone): same `=` in muted, differs `≠` in staging
amber, missing as a hollow ring in production red, empty `∅` dashed, extra `+`. Text is
left-aligned; numbers right-aligned.

### Interaction and copy

- Motion only answers actions: a revealed value fades back to dots with a visible 10 s countdown;
  copy shows "Copied, clears in 30 s". Respect `prefers-reduced-motion`.
- Full keyboard use: `Ctrl+K` key search, arrow keys in the matrix, `Enter` reveal, `Ctrl+C` copy,
  `Ctrl+L` lock. Visible focus ring on everything.
- Buttons say what happens: "Import .env", "Run push check", "Export .env.example", "Lock".
- Empty states direct the next step ("Add a project, then import its .env files").
- Errors say what happened and how to fix it ("Line 14: missing `=` after `API_KEY`").

## Brand assets

Provided in `/assets` (not `/brand`): `loki-icon.svg` (master), `loki-icon-1024.png`, `loki.ico`.
The mark is a key whose bit is a branch graph: the shaft is the main line and the teeth fork off
it, ending in the three stage colors (production red, staging amber, dev blue).
Generate all Tauri icons with `npm run tauri icon ./assets/loki-icon-1024.png`, then delete the
generated `icons/android` and `icons/ios` folders (Windows only). Never redraw the
icon in code; use the SVG file. Wordmark is simply "LOKI" in Segoe UI Variable Semibold with the
tagline "Local Only Key Inventory".

## Packaging and release (Windows)

`tauri.conf.json`:
- `productName`: "LOKI"; `identifier`: `com.<owner>.loki` (ask the user for the owner part).
- `bundle.targets`: `["msi", "nsis"]`; `bundle.icon` from the generated icons.
- `bundle.windows.webviewInstallMode`: `{ "type": "offlineInstaller" }` for the public installer
  (no download during install) or `{ "type": "skip" }` for a Windows 11-only build.
- Main window: `visible: false`, `backgroundColor` = `--ink`, sensible `minWidth`/`minHeight`.
- No updater plugin. Updates arrive through winget, the Microsoft Store, or a new installer.

Release flow (only when the user asks to go public): MIT or Apache-2.0 license, README with
screenshots, `SECURITY.md` describing the crypto and offline design, a GitHub Actions workflow
that builds and attaches the installer to a GitHub Release, then a winget manifest PR to
`microsoft/winget-pkgs`. Code signing and Microsoft Store submission are separate steps the user
handles; do not add signing secrets to the repo.

## Coding conventions

- Rust: `thiserror` for errors; error messages may name keys, never values. No `unwrap()` outside
  tests. Every DB write path in a transaction. Keep `commands/` thin; logic in `core/`.
- Frontend: function components + hooks, no global state library unless needed; all IPC through
  `src/lib/ipc.ts`; no secret value ever stored in React state longer than the reveal timeout.
- Windows-first: use `std::path`, handle backslashes and spaces in paths, test with paths like
  `C:\Users\Name\My Projects\app\.env.production`.
- Small PR-sized steps. After each milestone: run all checks, update the checklist below.

## Tests that must exist

- Parser: every case listed in M3, plus round-trip property test.
- Crypto: encrypt/decrypt round trip, wrong key fails, swapped AAD fails, fingerprints equal for
  equal values and differ across vaults.
- Diff and compare matrix: all five cell states.
- Rules: each push-day rule with a positive and negative case.
- Leak tests: serialize every DTO except `reveal_value` and assert the fake value string does not
  appear; run a full import with logging at TRACE and assert the value is absent from logs.
- Offline: `npm run check:offline` passes.

## Definition of done (per feature)

Works offline on Windows 10/11, no plaintext value in logs or DTOs, tests added and passing,
`cargo clippy` clean, `check:offline` green, `npm run size` within budget, works in dark and
light themes with keyboard only, audit log entry for reveal/copy/export/delete.

## Milestone checklist

- [x] M0 Scaffold + offline guard
- [x] M1 Vault
- [x] M2 Projects and targets
- [x] M3 Import + scan + parser
- [x] M4 Compare matrix
- [x] M5 Versions + diff
- [x] M6 Push-day check
- [x] M7 Export
- [ ] M8 Icon, installer and performance budget verified on a release build
      Done: icon, release `loki.exe` 4.8 MB, cold start about 390 ms, scroll about 70 fps on a 500x6
      matrix, JS 100 KB gzipped. Not done: the `.msi`/`.exe` installers were not produced because
      the bundler could not download WiX and NSIS in the build sandbox. Run `npm run tauri build`
      on a normal connection (build-time only; the app itself never touches the network).
- [x] v1 polish: global key search, key notes, encrypted `.loki` backup, audit log viewer,
      dark/light theme, settings, exclude-from-capture

## Decisions and deviations from this spec (recorded while building)

These were made without asking because they are small, reversible, or required by the rules above.

- **Comments are encrypted.** `variables.comment` holds `nonce(24) || ciphertext` (AAD
  `"{snapshot_id}:{key}:comment"`), because comments often contain values. Key names stay
  plaintext. The frontend only learns `hasComment`, never the comment text.
- **`settings` table added** (JSON document under key `settings`). Theme is the one exception: it
  lives in `localStorage` so it applies before unlock.
- **Save and open dialogs for exports, push-check checklists and backups are opened from Rust.**
  The frontend never supplies a write path (rule 4). Capabilities are `core:default`,
  `core:window:allow-show` and `dialog:allow-open` only.
- **Copy goes through `arboard` in Rust, not the clipboard plugin.** It is already a transitive
  dependency, adds no new code to the tree, and lets the copy be excluded from Windows clipboard
  history and cloud sync. The clipboard-manager plugin and npm package were removed. The 30 s
  timer only clears the clipboard if it still holds LOKI's value.
- **The main window is created in Rust** (`create: false` in the config) so its WebView2 profile
  lives in `%LOCALAPPDATA%\LOKI\webview`, not in `%LOCALAPPDATA%\<identifier>`.
- **`LOKI_DATA_DIR`** overrides the data folder (use a scratch folder to try the app safely).
- **Light-theme tokens were darkened to pass WCAG AA** (checked by `src/lib/contrast.test.ts`):
  `--prod` `#D93B38` to `#CE3835`, `--staging` `#B9740F` to `#9D630D`, `--dev` `#1F7BB8` to
  `#1D75AF`. Dark tokens are unchanged.
- **Parser warnings never echo a bare line** unless it looks like an `UPPER_SNAKE` env name,
  because a line with no `=` might be a secret pasted without its key.
- **`KEY=# note`** (hash followed by a space at the start of a value) is a comment; **`KEY=#fff`**
  keeps the `#`. A `#` inside an unquoted value needs whitespace before it to start a comment.
- **Shared-secret rule** treats a key as secret if its key note says so, or, with no note, if the
  name contains KEY, SECRET, TOKEN, PASSWORD, PASSWD, PWD, CREDENTIAL or PRIVATE.
- **The `test` placeholder word** is matched as a substring (so it also hits `latest`). It sits
  in the separate `placeholder_word` rule so it can be switched off alone.
- **Backup import adds, never overwrites.** A project whose name already exists is imported as
  `Name (imported)`, `Name (imported 2)` and so on.
- **Restore and re-import:** a restored snapshot keeps no source path or hash, so the next
  re-import of the file always creates a new version instead of wrongly saying "unchanged".
- **No logging crate on purpose.** `leak_tests.rs` scans the Rust source for print and log
  macros, so "no value in logs" holds by construction. The only `eprintln!` calls are the
  startup failure message and a dev-build-only timing line in `lib.rs`.
- **`npm run check:offline`** asks `cargo tree` for the Windows production dependency graph
  instead of grepping `Cargo.lock`: the lockfile lists `reqwest` and `hyper` for platforms that
  never build on Windows, and a grep would false-positive on them.
- **Editing a single value is not built.** The IPC surface has no edit command; change a value by
  re-importing a file or restoring a version (both create a new snapshot).
- **Exo 2 is bundled, not "system fonts only".** The user asked for Exo 2 typography. It is a
  local asset (no network), used for the wordmark and headings; body text keeps system fonts for
  density and speed.
- **`rust-version` is 1.85** (was 1.77) so `Option::is_none_or` can be used.
- **Tables use the `data-table` class.** `grid` collided with Tailwind's `display: grid` utility.
- **Run Vitest from `E:\loki`, not `e:\loki`.** On Windows a lowercase drive letter makes Vitest
  load two copies of its runner and every suite fails with "Cannot read properties of undefined
  (reading 'config')".
- **Python edits on Windows write CRLF.** Source files are LF; normalize after scripted edits.
- **End-to-end test hooks (debug builds only).** `LOKI_TEST_SAVE_DIR` makes save dialogs save into
  that folder, `LOKI_TEST_OPEN_FILE` answers the backup open dialog, and in the dev server
  `window.__lokiTestPickFile` answers the frontend file picker. They are compiled out of release
  builds (`#[cfg(debug_assertions)]`, `import.meta.env.DEV`). Never automate native dialogs with
  simulated keystrokes: on a real desktop they can land in another app.
- **Installers install per machine (`C:\Program Files\LOKI`).** Tauri's NSIS default is a
  per-user install into `%LOCALAPPDATA%\LOKI`, which is the vault's data folder. `nsis.installMode`
  is `perMachine` so an install or uninstall can never touch `loki.db`. Never change this back.
- **Day-to-day copy for the developer lives in `%LOCALAPPDATA%\Programs\LOKI\loki.exe`** (the
  desktop shortcut points there), so `npm run tauri build` never collides with a running LOKI.
  After a build, copy `src-tauri\target\release\loki.exe` there with LOKI closed.
- **Microsoft Store build is an MSIX** (`npm run build:msix`, output in `src-tauri\target\msix`),
  left unsigned because the Store signs it; the .msi/.exe stay for GitHub Releases. The manifest
  is `src-tauri/msix/AppxManifest.xml`; its identity (`sainathbelagavi.LOKI-LocalOnlyKeyInventory`,
  `CN=5460F997-…`, publisher display name) must match Partner Center exactly. Tile images come
  from `scripts/msix-assets.ps1`.
- **The MSIX must not virtualize AppData.** `FileSystemWriteVirtualization` is disabled with the
  `unvirtualizedResources` capability, so the Store build uses the real `%LOCALAPPDATA%\LOKI` (the
  same vault as the .msi/.exe build). With virtualization on, Windows deletes the vault on
  uninstall. `build-msix.mjs` refuses to pack without these lines. Never remove them. Test the
  installed package only under a spare Windows account: it opens the real vault folder.
- **Releases come from tags.** Bump the version in `package.json`, `Cargo.toml` and
  `tauri.conf.json`, add `docs/releases/vX.Y.Z.md`, push tag `vX.Y.Z`: `release.yml` builds the
  .msi/.exe with `SHA256SUMS.txt` into a DRAFT release (the maintainer publishes it) and keeps the
  MSIX as a workflow artifact for Partner Center. Store listing text and screenshots live in
  `docs/store/` (fake demo data only).
- **Settings → Your data** shows the data folder (from `app_info`, a path, never a value) and says
  uninstalling keeps it and how to remove it. An MSIX uninstall cannot show any app UI, so this
  and the README/Store text are the only places to say it.
- **Startup failures show a native message box** (`MessageBoxW` via a direct `user32` import, no
  crate), including a missing WebView2 Runtime, which the MSIX cannot bundle.
