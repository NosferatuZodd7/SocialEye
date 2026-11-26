# Contributing to SocialEye

Thanks for helping! Bug reports, docs fixes, translations and new official-API integrations are all welcome.

## Ground rules
- **Official APIs only.** No scraping, unofficial/private APIs, password handling, cookie/session extraction, CAPTCHA bypass or hidden automation. PRs that add any of these will be closed.
- **Never fabricate data.** If an API doesn't provide something, return `AppError::Unsupported` or record it as unavailable — never a placeholder value or simulated success.
- **No secrets in the repository**: no API keys, OAuth secrets, tokens, real exports or personal data in code, fixtures or screenshots.
- Significant actions (publish, delete, restore) must keep requiring explicit confirmation.

## Setup
See the *Develop* section of the README. Run before pushing:

```bash
npm run lint && npm run typecheck && npm test
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## Where things live
- `src-tauri/src/connectors/<platform>.rs` — one file per platform; parsers are pure functions with unit tests using anonymised API responses.
- `src-tauri/src/commands.rs` — the IPC surface; validate input here.
- `src/lib/i18n.tsx` — every UI string, in English and Portuguese (TypeScript fails if a key is missing).
- `docs/ARCHITECTURE.md` — design and database.

## Translations
Add a dictionary to `src/lib/i18n.tsx` typed as `Record<Key, string>` and register it in `dictionaries`. The i18n test checks that placeholders match.

## Commits and PRs
Small, focused PRs with a clear description and test notes. Update `CHANGELOG.md` for user-visible changes. By contributing you agree your work is licensed under the MIT License.
