# Contributing to LOKI

Thanks for wanting to make LOKI better. Fork the repository, work on a branch in your fork, and
open a pull request. Every change is reviewed and merged by the maintainer,
[@saibelagavi](https://github.com/saibelagavi), who decides what goes in.

## Before you start

- **Bugs:** open an issue with steps to reproduce (use fake values, never real secrets), or send a
  pull request with the fix and a test.
- **New features:** please open a feature request first and wait for a reply before writing code.
  LOKI is deliberately small and offline, so some good ideas will not fit; agreeing on the scope
  first saves your time.
- **Security problems:** do not open an issue. Follow [SECURITY.md](SECURITY.md).

## The rules every change must keep

These come from [CLAUDE.md](CLAUDE.md), the project's design document. A pull request that breaks
one of them will not be merged.

1. **No network, ever.** No crate, npm package, plugin, font, CDN link, analytics, updater or
   telemetry that opens a socket. `npm run check:offline` must pass.
2. **Secret values never leave the Rust core** unless the user explicitly asks (reveal, copy,
   export, share). DTOs carry key names, statuses and fingerprints only.
3. **Never log, print or snapshot a secret value.** Tests use obvious fake values such as
   `fake_value_1`. Never commit a real `.env` file.
4. **Nothing is written outside `%LOCALAPPDATA%\LOKI`** except files the user saves through a save
   dialog.
5. **New dependencies need a discussion first** (in the issue). Small and auditable beats clever.
6. **Database changes are additive migrations.** Users' vaults must upgrade without losing data.

## Development setup

Requirements: Windows 10 or 11, Node.js 20+, Rust (stable), and the Visual Studio C++ Build Tools.

```powershell
npm install
$env:LOKI_DATA_DIR = "$env:TEMP\loki-dev"   # never develop against your real vault
npm run tauri dev
```

## Before you open the pull request

Run everything CI runs. All of it must pass.

```powershell
npm run lint
npm test                 # run from the repository folder with an UPPERCASE drive letter (E:\, not e:\)
npm run build
npm run size
npm run check:offline
cd src-tauri
cargo clippy --all-targets -- -D warnings
cargo test --lib
```

Also:

- Add or update tests for what you changed (Rust in the module's `tests`, frontend in
  `src/lib/*.test.ts`).
- Check the change in both the dark and the light theme, and with the keyboard only.
- Keep the pull request focused on one thing, and explain the why in the description.

## Style

- Rust: `thiserror` errors whose messages may name keys but never values; no `unwrap()` outside
  tests; every database write in a transaction; `commands/` stays thin, logic lives in `core/`.
- Frontend: function components and hooks; every backend call goes through `src/lib/ipc.ts`; no
  secret value in React state longer than the reveal timeout.
- Copy: sentence case, buttons say what happens ("Run push check"), errors say how to fix it.

## License

By contributing, you agree that your contribution is licensed under the
[Apache License 2.0](LICENSE), the same license as the project.
