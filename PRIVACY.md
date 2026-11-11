# LOKI privacy policy

Last updated: 12 November 2026

LOKI (Local Only Key Inventory) is a desktop app published by Sainath Belagavi.

## The short version

LOKI does not collect, transmit, sell or share any information. It has no network code, no
analytics, no telemetry, no crash reporting and no accounts. Everything stays on your computer.

## What LOKI stores, and where

Only on your own device, in `%LOCALAPPDATA%\LOKI\`:

- **Your vault** (`loki.db`): the `.env` values, comments and passwords you import or enter,
  encrypted with a key derived from your master password; and, in plaintext, the names of your
  projects, targets (branch and stage) and keys, notes you write about keys, settings, and a local
  audit log of actions such as reveal, copy and export (it records key names, never values).
- **Safety copies** (`backups\`): an automatic copy of the vault made before an upgrade.
- **App preferences** (`webview\`): your theme choice and the embedded browser's cache.

Your master password is never stored anywhere.

## What leaves your device

Nothing, unless you do it yourself: files you export, encrypted backups or shares you create, and
values you copy to the clipboard. LOKI writes exported files only to locations you choose.

## Children

LOKI is a developer tool and does not knowingly process any information about children.

## Deleting your data

Delete the folder `%LOCALAPPDATA%\LOKI\`. Uninstalling LOKI does not delete it, so that a
reinstall does not lose your vault. LOKI cannot recover a forgotten master password.

## Changes

Changes to this policy will be published in this file in the project repository, with a new date.

## Contact

belagavisainath1@gmail.com
