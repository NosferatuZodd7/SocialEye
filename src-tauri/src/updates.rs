//! Self-update from this repository's GitHub Releases (same model as OrbisLink and bitBybit):
//! no SocialEye server, no account, no telemetry.
//!
//! Channels
//! - `testing`: every CI build of the development branch, published as a pre-release `vX.Y.Z-dev.N`.
//! - `stable`: releases `vX.Y.Z` published from a tag pushed by the owner.
//!
//! Trust chain (fail closed)
//! - CI signs the installer with the private key held only in the `TAURI_SIGNING_PRIVATE_KEY`
//!   Actions secret; without it nothing is published.
//! - The public key is compiled into the app (`SOCIALEYE_UPDATER_PUBKEY` at build time). A build
//!   without a key refuses to update.
//! - The app picks the newest release of its channel that is newer than itself, downloads only from
//!   `https://github.com/<repo>/releases/download/…`, and the Tauri updater verifies the minisign
//!   signature (including the signed version) before running the installer. Older or equal versions
//!   are never installed.
use crate::error::{AppError, AppResult};
use serde::Serialize;
use serde_json::Value;

/// Updater public key (minisign, base64), baked in by CI. `None` = updates disabled in this build.
pub const PUBKEY: Option<&str> = option_env!("SOCIALEYE_UPDATER_PUBKEY");
/// `owner/repo` the app updates from.
pub const REPOSITORY: &str = match option_env!("SOCIALEYE_UPDATE_REPOSITORY") {
    Some(r) => r,
    None => "NosferatuZodd7/SocialEye",
};
/// Channel used when the user has not chosen one. Testing while SocialEye is a 0.x preview.
pub const DEFAULT_CHANNEL: &str = "testing";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Available {
    pub version: String,
    pub tag: String,
    pub prerelease: bool,
    pub notes: String,
    pub published_at: Option<String>,
    pub html_url: String,
    /// Signed update manifest of that release (Tauri `latest.json`).
    pub manifest_url: String,
}

pub fn enabled() -> bool {
    PUBKEY.is_some_and(|k| !k.trim().is_empty())
}

pub fn parse_tag(tag: &str) -> Option<semver::Version> {
    semver::Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()
}

/// From the GitHub releases list, the newest release of `channel` that is newer than `current`
/// and carries a signed update manifest. Pure, so it is unit-tested.
pub fn pick(releases: &Value, channel: &str, current: &semver::Version, repo: &str) -> Option<Available> {
    let prefix = format!("https://github.com/{repo}/releases/download/");
    releases
        .as_array()?
        .iter()
        .filter(|r| !r["draft"].as_bool().unwrap_or(true))
        .filter(|r| channel == "testing" || !r["prerelease"].as_bool().unwrap_or(true))
        .filter_map(|r| {
            let tag = r["tag_name"].as_str()?;
            let v = parse_tag(tag)?;
            // A stable user never gets a pre-release even if it was published as a normal release.
            if channel != "testing" && !v.pre.is_empty() {
                return None;
            }
            let manifest = r["assets"].as_array()?.iter().find(|a| a["name"].as_str() == Some("latest.json"))?;
            let url = manifest["browser_download_url"].as_str()?;
            if !url.starts_with(&prefix) {
                return None;
            }
            (v > *current).then(|| {
                (
                    v.clone(),
                    Available {
                        version: v.to_string(),
                        tag: tag.into(),
                        prerelease: r["prerelease"].as_bool().unwrap_or(false),
                        notes: r["body"].as_str().unwrap_or_default().chars().take(4000).collect(),
                        published_at: r["published_at"].as_str().map(Into::into),
                        html_url: r["html_url"].as_str().unwrap_or_default().into(),
                        manifest_url: url.into(),
                    },
                )
            })
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, a)| a)
}

pub async fn check(http: &reqwest::Client, channel: &str, current: &semver::Version) -> AppResult<Option<Available>> {
    if !enabled() {
        return Err(AppError::unsupported(
            "updates",
            "this build has no update signing key, so it cannot verify updates",
            Some("Install the latest version from the project's GitHub Releases page."),
        ));
    }
    if !matches!(channel, "testing" | "stable") {
        return Err(AppError::validation("channel must be testing or stable"));
    }
    let v = crate::http::send(
        http.get(format!("https://api.github.com/repos/{REPOSITORY}/releases")).query(&[("per_page", "40")]).header("Accept", "application/vnd.github+json"),
    )
    .await?;
    Ok(pick(&v, channel, current, REPOSITORY))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const REPO: &str = "o/r";

    fn rel(tag: &str, pre: bool, draft: bool, with_manifest: bool) -> Value {
        let assets = if with_manifest {
            json!([{"name": "latest.json", "browser_download_url": format!("https://github.com/o/r/releases/download/{tag}/latest.json")}])
        } else {
            json!([{"name": "SocialEye_setup.exe", "browser_download_url": "x"}])
        };
        json!({"tag_name": tag, "prerelease": pre, "draft": draft, "assets": assets, "body": "notes", "html_url": "h"})
    }

    fn v(s: &str) -> semver::Version {
        semver::Version::parse(s).unwrap()
    }

    #[test]
    fn testing_gets_newest_build_including_stable() {
        let list = json!([
            rel("v0.1.0-dev.12", true, false, true),
            rel("v0.1.0-dev.9", true, false, true),
            rel("v0.1.0", false, false, true),
            rel("v0.1.1-dev.3", true, false, true)
        ]);
        assert_eq!(pick(&list, "testing", &v("0.1.0-dev.12"), REPO).unwrap().version, "0.1.1-dev.3");
        assert_eq!(pick(&list, "stable", &v("0.1.0-dev.12"), REPO).unwrap().version, "0.1.0");
    }

    #[test]
    fn semver_orders_dev_builds_numerically() {
        let list = json!([rel("v0.1.0-dev.9", true, false, true), rel("v0.1.0-dev.10", true, false, true)]);
        assert_eq!(pick(&list, "testing", &v("0.1.0-dev.2"), REPO).unwrap().tag, "v0.1.0-dev.10");
    }

    #[test]
    fn never_downgrades_or_offers_equal() {
        let list = json!([rel("v0.1.0", false, false, true), rel("v0.0.9", false, false, true)]);
        assert!(pick(&list, "stable", &v("0.1.0"), REPO).is_none());
        assert!(pick(&list, "testing", &v("0.2.0-dev.1"), REPO).is_none());
    }

    #[test]
    fn skips_drafts_unsigned_and_foreign_urls() {
        let mut foreign = rel("v9.0.0", false, false, true);
        foreign["assets"][0]["browser_download_url"] = json!("https://evil.example/latest.json");
        let list = json!([rel("v2.0.0", false, true, true), rel("v3.0.0", false, false, false), foreign, rel("not-a-version", false, false, true)]);
        assert!(pick(&list, "testing", &v("0.1.0"), REPO).is_none());
    }

    #[test]
    fn stable_ignores_prerelease_tags_even_if_not_flagged() {
        let list = json!([rel("v0.2.0-dev.1", false, false, true)]);
        assert!(pick(&list, "stable", &v("0.1.0"), REPO).is_none());
    }
}
