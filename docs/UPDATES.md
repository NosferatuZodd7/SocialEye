# Updates

SocialEye updates itself from the GitHub Releases of this repository, the same model as OrbisLink and bitBybit. There is no SocialEye server, no account and no telemetry.

## Channels

| Channel | What it gets | Where it comes from |
|---|---|---|
| **Testing (branch builds)** — default while SocialEye is 0.x | every build of the development branch | a GitHub **pre-release** `vX.Y.Z-dev.N`, published by CI for every push to `DEV_BRANCH` (set at the top of `.github/workflows/ci.yml`) or `main`; N is the workflow run number |
| **Stable** | final versions only | a GitHub release `vX.Y.Z`, published by CI when the owner pushes the tag `vX.Y.Z` (it must equal the `version` in `package.json`) |

Semantic-version ordering: `0.1.0-dev.9 < 0.1.0-dev.10 < 0.1.0 < 0.1.1-dev.1`. If `vX.Y.Z` is already released, test builds move to the next patch automatically. A testing user receives a stable release when it is newer; a stable user is never moved to a test build. Older or equal versions are never installed.

The user picks the channel in **Settings → Updates**. SocialEye checks at startup and every 6 hours (this can be turned off) and shows a notification once per new version. **Nothing is installed without the user clicking "Install and restart" and confirming.**

## Trust chain (fail closed)

1. CI builds the NSIS installer with the version stamped into `tauri.conf.json`, the update public key (`SOCIALEYE_UPDATER_PUBKEY`) and the repository (`SOCIALEYE_UPDATE_REPOSITORY`) compiled into the app.
2. The Tauri CLI signs the installer with the private key from the `TAURI_SIGNING_PRIVATE_KEY` secret (minisign/Ed25519). The signature's trusted comment includes the version. `tauri-action` uploads the installer, its `.sig` and `latest.json` to the release. **Without the secret and the public-key variable nothing is published**: the job prints a warning and only uploads unsigned build artifacts.
3. The app lists `api.github.com/repos/<repo>/releases` and picks the newest non-draft release of its channel that is newer than itself and has a `latest.json` served from `https://github.com/<repo>/releases/download/…` (`src-tauri/src/updates.rs`, unit-tested).
4. The Tauri updater downloads the installer named in that `latest.json`, verifies the signature with the embedded public key, and with `requireSignedVersion` also checks that the signed version equals the announced one (this blocks replaying an old signed build as a "new" version). Any failure refuses the update.
5. The NSIS installer runs in passive mode (progress bar, no questions) over the existing installation; data in `%APPDATA%\io.github.socialeye` is kept. SocialEye restarts on the new version.

A build compiled without `SOCIALEYE_UPDATER_PUBKEY` (for example a local `npm run tauri build`) has no updater: Settings shows that updates are disabled, and that copy must be updated by installing a new version manually once.

## One-time setup (repository owner)

1. Generate the key pair on your own computer (never in the repository):
   ```bash
   npx tauri signer generate -w ~/.tauri/socialeye.key
   ```
   It asks for a password and writes `socialeye.key` (private) and `socialeye.key.pub` (public).
2. GitHub → repository **Settings → Secrets and variables → Actions**:
   - **Secrets**: `TAURI_SIGNING_PRIVATE_KEY` = the full contents of `socialeye.key`; `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = its password.
   - **Variables**: `SOCIALEYE_UPDATER_PUBKEY` = the full contents of `socialeye.key.pub`.
3. Push to the development branch. The `Windows installers (and update release)` job publishes `vX.Y.Z-dev.N` as a pre-release.
4. Install that build once manually (download the `-setup.exe` from the release). From then on it updates itself.

Keep a backup of the private key and password: if they are lost, installed copies cannot verify new releases and every user has to reinstall manually once.

### Rotating the key

Installed copies trust only the public key they were built with. To rotate: generate a new pair, publish one last release signed with the **old** key whose embedded public key is the **new** one (set the new variable, keep the old secret for that run), then switch the secret to the new private key.

## Publishing a stable release

See [RELEASE.md](RELEASE.md): bump the version, tag `vX.Y.Z`, push the tag.

## Code signing (Authenticode) — not configured

The updater signature above is configured by the two settings in step 2. Windows Authenticode signing of the installer is a separate thing: without it, SmartScreen shows "Unknown publisher" on a manual download. It needs an OV/EV certificate or Azure Trusted Signing; see the Tauri Windows signing guide. No certificate may be committed.
