## What and why

<!-- What does this change, and what problem does it solve? Link the issue: "Closes #123". -->

## How it was tested

<!-- Commands you ran, and what you checked by hand (both themes, keyboard only). -->

## Checklist

- [ ] For a new feature: it was agreed in an issue first
- [ ] `npm run lint`, `npm test`, `npm run build`, `npm run size` pass
- [ ] `npm run check:offline` passes (no networking added)
- [ ] `cargo clippy --all-targets -- -D warnings` and `cargo test --lib` pass
- [ ] Tests added or updated; only fake values (`fake_value_1`), no real `.env` files
- [ ] No secret value is logged, printed, or sent to the UI without an explicit user action
- [ ] Database changes (if any) are additive migrations
- [ ] Works in dark and light themes, and with the keyboard only
