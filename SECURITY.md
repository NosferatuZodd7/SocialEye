# Security policy

## Reporting a vulnerability
Please report privately via **GitHub → Security → Report a vulnerability** on this repository. Do not open public issues for security problems. Include steps to reproduce and the affected version. We aim to acknowledge within 7 days.

## Supported versions
Only the latest release receives security fixes.

## Security model
- OAuth 2.0 authorization code + PKCE with a loopback redirect and `state` validation; SocialEye never asks for or stores social network passwords.
- Access/refresh tokens and client secrets are stored in the OS credential store (Windows Credential Manager), never in SQLite, logs, exports or the WebView.
- Tokens are sent only in `Authorization` headers; error messages are stripped of request URLs.
- All IPC inputs are validated in Rust; SQL uses bound parameters only; LIKE wildcards are escaped.
- Imports (Instagram exports, CSV) are size-limited, parsed in memory and validated (usernames must match Instagram's character set).
- Spreadsheet exports neutralise formula injection.
- A restrictive Content Security Policy; local media is exposed to the WebView only per file through the asset-protocol scope.
- External links are opened only for `https://` URLs.
- Destructive or public actions require explicit confirmation, enforced in the backend.
- Updates come only from this repository's GitHub Releases, are verified against a public key compiled into the app (minisign signature over the installer, including the version), are never older than the installed version, and are installed only after the user confirms. Builds without the key have no updater. The private key exists only as an Actions secret.
- CI runs `npm audit` and `cargo audit`; Dependabot keeps dependencies current.
