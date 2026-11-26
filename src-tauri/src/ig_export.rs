//! Instagram "Download your information" export parsing (JSON and HTML) and
//! follower/following comparison. This is the only official way to get follower lists.
use crate::error::{AppError, AppResult};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

pub const MAX_FILE_BYTES: usize = 50 * 1024 * 1024;
pub const MAX_FILES: usize = 60;

#[derive(Deserialize)]
pub struct ImportFile {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Default, Serialize)]
pub struct ParsedExport {
    /// username → follow timestamp (unix secs) if present
    pub followers: BTreeMap<String, Option<i64>>,
    pub following: BTreeMap<String, Option<i64>>,
    pub used_files: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, PartialEq)]
enum Kind {
    Followers,
    Following,
    Ignored,
    Unknown,
}

fn classify(name: &str) -> Kind {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name).to_lowercase();
    let stem = base.trim_end_matches(".json").trim_end_matches(".html");
    let re = followers_re();
    if re.is_match(stem) {
        Kind::Followers
    } else if stem == "following" {
        Kind::Following
    } else if [
        "close_friends",
        "pending_follow_requests",
        "recently_unfollowed_profiles",
        "blocked_profiles",
        "restricted_profiles",
        "removed_suggestions",
        "follow_requests_you've_received",
        "hide_story_from",
        "following_hashtags",
    ]
    .contains(&stem)
    {
        Kind::Ignored
    } else {
        Kind::Unknown
    }
}

fn followers_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^followers(_\d+)?$").unwrap())
}

fn href_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"href="https?://(?:www\.)?instagram\.com/(?:_u/)?([A-Za-z0-9._]{1,30})/?""#).unwrap())
}

pub fn valid_username(u: &str) -> bool {
    !u.is_empty() && u.len() <= 30 && u.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
}

fn username_from(entry: &Value, item: &Value) -> Option<String> {
    let from_href = || entry["href"].as_str().and_then(|h| h.trim_end_matches('/').rsplit('/').next().map(String::from));
    let u = entry["value"].as_str().map(String::from).or_else(|| item["title"].as_str().filter(|t| !t.is_empty()).map(String::from)).or_else(from_href)?;
    let u = u.trim().to_lowercase();
    valid_username(&u).then_some(u)
}

/// Parse the JSON shape used for both files: an array of items (followers) or
/// `{relationships_following: [...]}` (following).
fn parse_json(v: &Value) -> Option<Vec<(String, Option<i64>)>> {
    let items = v.as_array().or_else(|| v.as_object().and_then(|o| o.values().find_map(Value::as_array)))?;
    let mut out = vec![];
    for item in items {
        for e in item["string_list_data"].as_array().into_iter().flatten() {
            if let Some(u) = username_from(e, item) {
                out.push((u, e["timestamp"].as_i64()));
            }
        }
    }
    Some(out)
}

fn parse_html(s: &str) -> Vec<(String, Option<i64>)> {
    href_re().captures_iter(s).map(|c| (c[1].to_lowercase(), None)).filter(|(u, _)| valid_username(u)).collect()
}

pub fn parse(files: &[ImportFile]) -> AppResult<ParsedExport> {
    if files.is_empty() {
        return Err(AppError::validation("no files selected"));
    }
    if files.len() > MAX_FILES {
        return Err(AppError::validation(format!("too many files (max {MAX_FILES})")));
    }
    let mut out = ParsedExport::default();
    for f in files {
        if f.content.len() > MAX_FILE_BYTES {
            out.warnings.push(format!("{}: larger than 50 MB, skipped", f.name));
            continue;
        }
        let kind = classify(&f.name);
        match kind {
            Kind::Ignored => {
                out.warnings.push(format!("{}: recognised, not needed for follower comparison", f.name));
                continue;
            }
            Kind::Unknown => {
                out.warnings.push(format!("{}: not a followers/following file, skipped", f.name));
                continue;
            }
            _ => {}
        }
        let entries = if f.name.to_lowercase().ends_with(".html") {
            parse_html(&f.content)
        } else {
            match serde_json::from_str::<Value>(&f.content).ok().as_ref().and_then(parse_json) {
                Some(e) => e,
                None => {
                    out.warnings.push(format!("{}: unrecognised JSON structure, skipped", f.name));
                    continue;
                }
            }
        };
        if entries.is_empty() {
            out.warnings.push(format!("{}: contained no accounts", f.name));
        }
        let target = if kind == Kind::Followers { &mut out.followers } else { &mut out.following };
        for (u, ts) in entries {
            target.entry(u).or_insert(ts);
        }
        out.used_files.push(f.name.clone());
    }
    if out.followers.is_empty() && out.following.is_empty() {
        return Err(AppError::validation(
            "no followers or following data found. In Instagram open Accounts Centre → Your information and permissions → Download your information, choose 'Followers and following', format JSON (or HTML), then select followers_1.json and following.json from the 'connections/followers_and_following' folder.",
        ));
    }
    if !out.used_files.iter().any(|n| classify(n) == Kind::Followers) {
        out.warnings.push("no followers file (followers_1.json) — 'not following back' cannot be computed".into());
    }
    if !out.used_files.iter().any(|n| classify(n) == Kind::Following) {
        out.warnings.push("no following file (following.json) — 'not following back' cannot be computed".into());
    }
    Ok(out)
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Comparison {
    pub not_following_back: Vec<String>,
    pub fans: Vec<String>,
    pub mutual: Vec<String>,
    pub new_followers: Option<Vec<String>>,
    pub lost_followers: Option<Vec<String>>,
    pub new_following: Option<Vec<String>>,
}

pub fn compare(
    followers: &BTreeSet<String>,
    following: &BTreeSet<String>,
    previous_followers: Option<&BTreeSet<String>>,
    previous_following: Option<&BTreeSet<String>>,
) -> Comparison {
    let v = |s: std::collections::btree_set::Difference<'_, String>| s.cloned().collect::<Vec<_>>();
    Comparison {
        not_following_back: v(following.difference(followers)),
        fans: v(followers.difference(following)),
        mutual: followers.intersection(following).cloned().collect(),
        new_followers: previous_followers.map(|p| v(followers.difference(p))),
        lost_followers: previous_followers.map(|p| v(p.difference(followers))),
        new_following: previous_following.map(|p| v(following.difference(p))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(name: &str, content: &str) -> ImportFile {
        ImportFile { name: name.into(), content: content.into() }
    }

    const FOLLOWERS: &str = r#"[{"title":"","media_list_data":[],"string_list_data":[{"href":"https://www.instagram.com/alice","value":"alice","timestamp":1700000000}]},
        {"title":"","string_list_data":[{"href":"https://www.instagram.com/Bob","value":"Bob","timestamp":1700000001}]}]"#;
    // 2025 format: username in "title", value omitted, /_u/ links.
    const FOLLOWING: &str = r#"{"relationships_following":[{"title":"alice","string_list_data":[{"href":"https://www.instagram.com/_u/alice","timestamp":1}]},
        {"title":"carol","string_list_data":[{"href":"https://www.instagram.com/_u/carol","timestamp":2}]}]}"#;

    #[test]
    fn parses_both_json_formats_and_compares() {
        let p = parse(&[f("connections/followers_and_following/followers_1.json", FOLLOWERS), f("following.json", FOLLOWING)]).unwrap();
        assert_eq!(p.followers.keys().collect::<Vec<_>>(), ["alice", "bob"]);
        assert_eq!(p.followers["alice"], Some(1_700_000_000));
        assert_eq!(p.following.len(), 2);
        let fs: BTreeSet<_> = p.followers.keys().cloned().collect();
        let fg: BTreeSet<_> = p.following.keys().cloned().collect();
        let c = compare(&fs, &fg, None, None);
        assert_eq!(c.not_following_back, ["carol"]);
        assert_eq!(c.fans, ["bob"]);
        assert_eq!(c.mutual, ["alice"]);
        assert!(c.new_followers.is_none());
    }

    #[test]
    fn multipart_followers_are_merged() {
        let p = parse(&[f("followers_1.json", FOLLOWERS), f("followers_2.json", r#"[{"string_list_data":[{"value":"dave"}]}]"#)]).unwrap();
        assert_eq!(p.followers.len(), 3);
        assert!(p.warnings.iter().any(|w| w.contains("following.json")));
    }

    #[test]
    fn parses_html_export() {
        let html = r#"<div><a target="_blank" href="https://www.instagram.com/eve.x_1">eve.x_1</a></div><a href="https://www.instagram.com/accounts/../evil">"#;
        let p = parse(&[f("followers_1.html", html)]).unwrap();
        assert_eq!(p.followers.keys().collect::<Vec<_>>(), ["eve.x_1"]);
    }

    #[test]
    fn rejects_unrelated_files_with_guidance() {
        let e = parse(&[f("liked_posts.json", "[]")]).unwrap_err();
        assert!(e.to_string().contains("Download your information"));
        assert!(parse(&[]).is_err());
    }

    #[test]
    fn invalid_usernames_are_dropped() {
        let p = parse(&[f("followers_1.json", r#"[{"string_list_data":[{"value":"<script>"}, {"value":"ok_user"}]}]"#)]).unwrap();
        assert_eq!(p.followers.keys().collect::<Vec<_>>(), ["ok_user"]);
    }

    #[test]
    fn history_diff() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<BTreeSet<_>>();
        let c = compare(&s(&["a", "c"]), &s(&[]), Some(&s(&["a", "b"])), Some(&s(&[])));
        assert_eq!(c.new_followers.unwrap(), ["c"]);
        assert_eq!(c.lost_followers.unwrap(), ["b"]);
    }
}
