# Microsoft Store listing (English)

Paste these into Partner Center → your app → Store listings → English. Plain text: the Store does
not render Markdown. Keep this file in step with what is live.

## Product name

LOKI - Local Only Key Inventory

## Short description

An encrypted, offline home for your .env files and credentials. See what differs between staging and production, what is missing, and what would break on push day, without your secrets ever leaving your PC.

## Description

LOKI keeps an encrypted, offline inventory of your .env files across projects, branches and stages, and answers one question fast on release day: what is different between staging and production, and what is missing?

No more opening five .env files side by side and hunting by eye for the key production forgot, the test key that slipped into production, or the localhost URL nobody noticed.

COMPARE EVERY STAGE AT A GLANCE
Keys as rows, your targets as columns. Every cell says same, differs, missing, empty or extra, worked out from encrypted fingerprints without decrypting a single value.

PUSH-DAY CHECK
Pick staging and production and get a list of findings: keys missing in production, test keys (sk_test_) in production, localhost or example URLs, placeholder values, secrets shared between production and other stages, DEBUG=true. Turn rules off or add your own, then export a checklist with key names only.

AND MORE
• Push-day overview: release readiness for every project, .env files that changed on disk, secrets due for rotation
• Diff checker: compare real values between two targets, versions, files or pasted text, with character-level highlights
• Versions: every import is a version you can diff, restore or re-import
• Secure share: send whole projects, single .env files or some keys in one encrypted file that opens only with a one-time pass key
• Passwords: logins that are not in a .env, with weak and reused passwords flagged
• Search every project with Ctrl+K, key notes, encrypted backups, an audit log, auto-lock, dark and light themes, full keyboard use

PRIVATE BY DESIGN
• Offline: LOKI has no network code at all and works with the cable pulled
• Encrypted: Argon2id and XChaCha20-Poly1305; your master password is never stored
• Masked: values stay hidden until you reveal one, and hide again after a few seconds
• Copies stay out of Windows clipboard history and clear themselves

YOUR DATA
Your vault lives in %LOCALAPPDATA%\LOKI (Settings > Your data shows the folder). Uninstalling or reinstalling LOKI keeps it. To remove everything, use Settings > Your data > Delete all LOKI data, then uninstall LOKI. LOKI cannot recover a forgotten master password.

LOKI is open source under the Apache License 2.0: github.com/saibelagavi/loki

The Store version requires Windows 11. On Windows 10, use the installer from github.com/saibelagavi/loki/releases

## Product features (one per line, up to 20)

Compare every key across production, staging and dev in one matrix
Push-day check: missing keys, test keys, localhost URLs, shared secrets
Release readiness, file drift and rotation reminders for every project
Diff real values with character-level highlights
Version history with diff, restore and one-click re-import
Secure share: one encrypted file, opened only with a one-time pass key
Password store with weak and reused password flags
Search keys across all projects with Ctrl+K
Encrypted backups and a local audit log
Fully offline: no network code, no account, no telemetry
Argon2id and XChaCha20-Poly1305 encryption
Dark and light themes, full keyboard use

## Keywords (up to 7, 40 characters each, 21 words in all)

api keys
LOKI key inventory
.env file manager
offline .env inventory
secret management offline

## Screenshots and captions (upload in this order)

1. 01-compare-dark.png: See every key across production, staging and dev at a glance: same, differs, missing or extra. Values stay masked.
2. 02-push-check-dark.png: Before you push: keys missing in production, test keys, localhost URLs and secrets shared across stages.
3. 03-overview-light.png: Every project on one screen: what blocks a release, which .env files changed on disk, which secrets are due for rotation.
4. 04-diff-checker-dark.png: Compare real values between any two sources, with character-level highlights. Values hide again after 60 seconds.
5. 05-share-choose-dark.png: Share whole projects, single .env files or just some keys, in one encrypted file.
6. 06-share-passkey-dark.png: LOKI generates a strong one-time pass key. Send the file and the key separately; no server, no account.
7. 07-passwords-light.png: Keep console and dashboard logins next to your .env files. Weak and reused passwords are flagged.
8. 08-history-dark.png: Every import is a version. See what changed, restore an old one, or re-import the file in one click.
9. 09-search-light.png: Ctrl+K: where is REDIS_URL used, and is it the same everywhere? Jump straight to the cell.

The screenshots use fake demo data only.

## Other fields

- Copyright and trademark info: Copyright 2026 Sainath Belagavi
- Additional license terms: Apache License 2.0, https://github.com/saibelagavi/loki/blob/main/LICENSE
- Developed by: Sainath Belagavi
- Privacy policy: https://github.com/saibelagavi/loki/blob/main/PRIVACY.md
- Website: https://github.com/saibelagavi/loki
- Support contact: belagavisainath1@gmail.com
- Minimum system requirements: this listing has no "Additional system requirements" field, so the
  Windows 11 note is the last line of the Description instead. The package's MinVersion enforces it.

## Properties

- Category: Developer tools; secondary category: Security
- Personal information: answer **Yes** with the privacy policy URL. LOKI transmits nothing, but it
  stores credentials and secrets locally, and certification expects a policy for that.
- Support info: website and email only; no phone or postal address (they are shown publicly).
- Product declarations: OneDrive backups OFF (nothing is synced), record and broadcast OFF (games
  only, and LOKI shows secrets), alternate drives on. Leave "tested to meet accessibility
  guidelines" unchecked until a screen-reader pass is done.
- System requirements: leave all unspecified (the package sets Windows 11 as the minimum).

## Age ratings questionnaire

No user interaction or chat, no user-generated content shared with others, no location sharing,
no purchases, no ads, no network access. Answer no to each of those.

## Submission options

Restricted capabilities justification:

runFullTrust:

LOKI is a Win32 desktop application (a Rust core with a WebView2 user interface, built with Tauri) packaged as MSIX. runFullTrust is required to launch its desktop executable, loki.exe, as a full-trust process, which is how all packaged Win32 desktop apps run. LOKI installs no services or drivers, never requests administrator rights, and has no network access.

unvirtualizedResources (third submission, version 0.1.2, after the 2026-11-24 report; numbered to match the six points asked):

1. Exact files written to $(KnownFolder:LocalAppData)\LOKI, and nothing else:
- loki.db: the user's encrypted vault (SQLite). It holds key names, values encrypted with XChaCha20-Poly1305, settings and a local audit log. The master password is never stored.
- loki.db-journal: SQLite's rollback journal. It exists only while a write is in progress and is deleted when the write completes.
- backups\loki-before-schema-v<N>-<timestamp>.db: a full copy of loki.db written only when an app update upgrades the database format, so an update can never corrupt the only copy of the vault.
In this version the WebView2 browser profile moved out of this folder into the package's own LocalCache folder, which stays virtualized and is removed by Windows on uninstall.

2. How virtualization breaks the core function: LOKI is an offline vault for developer secrets (.env values and passwords) and is often the only place a user keeps them. Virtualized, the vault would live in the package's private storage, which Windows deletes on uninstall, on reinstall, and on Settings > Apps > Reset. A user reinstalling LOKI would silently and permanently lose every secret, with no way to recover it. Users who move from our standalone installer to the Store version would also get a split vault: virtualized reads still see the real folder, but the first write copies the database into package storage, after which the two versions show different data without any warning.

3. Why non-restricted storage is insufficient: ApplicationData.LocalFolder, LocalCacheFolder and other package-local storage have the same uninstall and reset deletion described in point 2. RoamingFolder is designed to sync and is also deleted on uninstall, which conflicts with LOKI's promise that nothing leaves the PC. Documents and other user folders are often redirected to OneDrive by Known Folder Move, which would upload the vault to the cloud. %LOCALAPPDATA% is the standard local, non-roaming, per-user location for this kind of data.

4. Data that remains after uninstall: only the files listed in point 1 (the encrypted vault and any pre-upgrade safety copies). Removal: Settings > Your data > Delete all LOKI data (new in this version). After the user re-enters the master password and types DELETE, it deletes those files and the folder, then closes LOKI. The user can also delete the folder in File Explorer; Settings > Your data shows its path with a Copy path button.

5. Users are informed: the create-vault screen, shown before any data is written, names the folder, says uninstalling or reinstalling keeps it, and says how to remove it. Settings > Your data repeats this, and so do the Store description ("YOUR DATA") and the project README.

6. The exclusion is limited to that one folder and to the minimum data: the manifest declares exactly one ExcludedDirectory, $(KnownFolder:LocalAppData)\LOKI. There is no RegistryWriteVirtualization or ExcludedKeys declaration, so registry virtualization stays enabled, and no other AppData directory is excluded. The folder holds only the files in point 1. LOKI has no network access.

Notes for certification (field on the separate Additional testing information page; no credentials there):

Third submission (version 0.1.2) after the 10.6.3 reports of 2026-11-18 and 2026-11-24. The unvirtualizedResources justification answers the six points asked, in order. New in this version for that review: Settings > Your data > Delete all LOKI data, a notice on the create-vault screen naming the data folder, and the WebView2 profile moved into package storage so %LOCALAPPDATA%\LOKI holds only the encrypted vault and its safety copies.

LOKI works fully offline and needs no account. On first launch, create any master password (LOKI has no accounts, so there is nothing to log in to). To try it: Add project, add a target, then Import .env > Paste and paste:
API_URL=https://api.example.test
STRIPE_KEY=fake_value_1
DEBUG=false
Then open the compare matrix, Push check and Diff checker. To check data removal: Settings > Your data > Delete all LOKI data. All values in this note are fake.
