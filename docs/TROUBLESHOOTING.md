# Troubleshooting

| Symptom | Cause and fix |
|---|---|
| "add your developer app's Client ID in Settings" | No app configured for that provider. See [API_SETUP.md](API_SETUP.md). |
| Browser shows `redirect_uri_mismatch` / "URL blocked" | The redirect URI registered with the provider must be exactly `http://127.0.0.1:<port>/callback` with the port shown in Settings. Spotify does not accept `localhost`. |
| "cannot listen on 127.0.0.1:53682" | Another program uses the port. Pick a different redirect port in Settings and register the new URI. |
| "timed out waiting for browser authorization" | Sign-in was not completed within 5 minutes. Try again. |
| Account shows **Needs reconnection** | The token expired or was revoked (password change, app removed, Google testing-mode 7-day limit). Click **Reconnect**. |
| "no Facebook Pages found" | You need a Page where you have a task, and must select it in the Meta login dialog. Personal profiles are not supported. |
| "no Instagram Professional account found" | Convert to Creator/Business and link the account to a Page, then reconnect. |
| Metrics listed under "Not provided by the platform" | The API didn't return them (retired metric, insufficient followers, missing permission). They are shown as unavailable rather than estimated. |
| "API quota exhausted" (YouTube) | Daily quota used up; it resets at midnight Pacific Time. Lower the sync frequency in Settings. |
| "Rate limited by the platform" | SocialEye retries automatically; if it persists, wait the indicated time. |
| Instagram publishing fails at "could not process the media" | The public URL must be directly downloadable by Meta (HTTPS, correct content type, JPEG for images, MP4/H.264 for reels). |
| YouTube video uploaded as private although public was chosen | Unaudited Google API projects can only upload private videos. |
| Scheduled Instagram post marked failed "missed schedule" | SocialEye was closed at the scheduled time. Open the draft and publish it manually. |
| Instagram import says "no followers or following data" | Select `followers_1.json` (and `followers_2.json`… if present) and `following.json` from `connections/followers_and_following/`. |
| Windows SmartScreen warns about the installer | Release builds are unsigned unless a signing certificate is configured. Choose "More info → Run anyway" if you trust the source. |

Data location: `%APPDATA%\io.github.socialeye\`. Before restoring a backup SocialEye saves the current database as `pre-restore.db` there.
