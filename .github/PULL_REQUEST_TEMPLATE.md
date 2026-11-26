## What this changes

<!-- One or two sentences. Link the issue: Fixes #123 -->

## How it was tested

- [ ] `npm run lint && npm run typecheck && npm test`
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` (in `src-tauri`)
- [ ] Tried in the app (`npm run tauri dev`) — describe what you clicked

## Checklist

- [ ] No real API keys, tokens, client secrets or personal data in code, tests, fixtures or screenshots
- [ ] Platform features use official APIs only (no scraping, private APIs, cookie or password handling)
- [ ] Unsupported operations return a structured "unsupported" error — no simulated success
- [ ] New UI text added to both English and Portuguese in `src/lib/i18n.tsx`
- [ ] Database changes are a **new** migration (shipped migrations are never edited)
- [ ] CHANGELOG.md updated for user-visible changes
