# Release process

1. Update the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`.
2. Move the *Unreleased* entries in `CHANGELOG.md` under the new version.
3. Make sure CI is green on `main` (frontend, Rust and the Windows installer build).
4. Tag and push: `git tag v0.2.0 && git push origin v0.2.0`.
5. The **Release** workflow builds the NSIS (`.exe`) and MSI installers on `windows-latest` and attaches them to a **draft** GitHub release.
6. Install the draft's installer on a clean Windows 10 and Windows 11 machine, connect at least one account, run a sync, then publish the release.

## Code signing (recommended)

Unsigned installers trigger SmartScreen warnings. To sign, add a certificate to the repository secrets and configure `bundle.windows.certificateThumbprint` / `signCommand` in `tauri.conf.json` as described in the [Tauri Windows signing guide](https://tauri.app/distribute/sign/windows/). Never commit certificates or passwords.

## Database changes

Add a new string to the end of `MIGRATIONS` in `src-tauri/src/db.rs`. Never edit a migration that has shipped. Add a test that opens a database at the previous version and upgrades it.
