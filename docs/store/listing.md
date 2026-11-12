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
Your vault lives in %LOCALAPPDATA%\LOKI (Settings > Your data shows the folder). Uninstalling or reinstalling LOKI keeps it. To remove everything, uninstall LOKI and then delete that folder. LOKI cannot recover a forgotten master password.

LOKI is open source under the Apache License 2.0: github.com/saibelagavi/loki

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

## Search terms (up to 7)

env
dotenv
environment variables
secrets manager
developer tools
config diff
api keys

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
- Minimum system requirements: Windows 10 version 1903 or later, x64

## Properties

- Category: Developer tools; secondary category: Security
- Personal information: answer **Yes** with the privacy policy URL. LOKI transmits nothing, but it
  stores credentials and secrets locally, and certification expects a policy for that.
- Support info: website and email only; no phone or postal address (they are shown publicly).
- Product declarations: OneDrive backups OFF (nothing is synced), record and broadcast OFF (games
  only, and LOKI shows secrets), alternate drives on. Leave "tested to meet accessibility
  guidelines" unchecked until a screen-reader pass is done.
- System requirements: leave all unspecified (the package sets Windows 10 1903+).

## Age ratings questionnaire

No user interaction or chat, no user-generated content shared with others, no location sharing,
no purchases, no ads, no network access. Answer no to each of those.

## Submission options

Restricted capabilities justification:

runFullTrust: LOKI is a desktop app (Tauri, Rust) that needs full trust to run its own process and its WebView2 window.
unvirtualizedResources: LOKI keeps the user's encrypted vault in %LOCALAPPDATA%\LOKI\loki.db. It must not be virtualized, because Windows would delete the user's only copy of their stored secrets on uninstall or reinstall, and the same vault is also used by LOKI's standalone installer. LOKI writes nothing else outside its own folder and never uses the network.

Notes for certification:

LOKI works fully offline and needs no account. On first launch, choose any master password (for example: test-password-2026). To try it: Add project, add a target, then Import .env > Paste and paste:
API_URL=https://api.example.test
STRIPE_KEY=fake_value_1
DEBUG=false
Then open the compare matrix, Push check and Diff checker. All values in this note are fake.
