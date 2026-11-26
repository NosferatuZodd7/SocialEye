# Release process

Releases are produced by `.github/workflows/ci.yml`; see [UPDATES.md](UPDATES.md) for channels, signing and the one-time key setup.

## Testing builds (automatic)

Every push to the development branch (`DEV_BRANCH` in `ci.yml`) or `main` that passes CI is published as a GitHub **pre-release** `vX.Y.Z-dev.N` with a signed NSIS installer and `latest.json`. Users on the Testing channel are offered it in the app.

## Stable release

1. Update the version in `package.json` (the source of truth for CI), `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`.
2. Move the *Unreleased* entries in `CHANGELOG.md` under the new version.
3. Make sure CI is green on the commit to release.
4. Tag and push: `git tag v0.2.0 && git push origin v0.2.0`. CI checks that the tag equals `package.json`, builds the NSIS and MSI installers and publishes a normal (non-pre) release with `latest.json`.
5. Install the release on a clean Windows 10 and Windows 11 machine, connect an account, run a sync, and check that an older copy on the Stable channel offers and installs the update.

After a stable `vX.Y.Z`, test builds automatically continue as `vX.Y.(Z+1)-dev.N`.

## Code signing (recommended)

Unsigned installers trigger SmartScreen warnings on manual installs. To sign, configure `bundle.windows.signCommand` (or a certificate thumbprint) as described in the [Tauri Windows signing guide](https://tauri.app/distribute/sign/windows/), reading credentials only from Actions secrets. Never commit certificates or passwords.

## Database changes

Add a new string to the end of `MIGRATIONS` in `src-tauri/src/db.rs`. Never edit a migration that has shipped: users update in place and their database is migrated on start.
