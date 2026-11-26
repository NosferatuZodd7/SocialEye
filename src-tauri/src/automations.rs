//! Rule engine: triggers + conditions → non-destructive actions (in-app and desktop
//! notifications). Each firing is de-duplicated so a rule fires once per occurrence.
use crate::db::query_json;
use crate::error::{AppError, AppResult};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    /// Account metric (latest value) crosses a threshold, e.g. subscribers ≥ 10,000.
    MetricThreshold {
        metric: String,
        value: f64,
    },
    /// A single content item's stat crosses a threshold, e.g. video views ≥ 10,000.
    ContentThreshold {
        metric: String,
        value: f64,
    },
    /// Account metric grew by at least `value` over `days`.
    MetricGain {
        metric: String,
        value: f64,
        days: i64,
    },
    NewContent,
    NewComments,
    SyncFailed,
    ReconnectRequired,
    TokenExpiring {
        days: i64,
    },
    PublishFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rule {
    pub trigger: Trigger,
    /// Conditions: restrict to platforms and/or accounts (empty = all).
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub account_ids: Vec<i64>,
    /// Actions. Only notification actions exist by design: automations never
    /// publish, delete or modify anything on a platform.
    #[serde(default = "default_true")]
    pub notify_in_app: bool,
    #[serde(default)]
    pub notify_desktop: bool,
}

fn default_true() -> bool {
    true
}

/// Something that happened during a sync or publish.
#[derive(Debug, Clone)]
pub enum Event {
    SyncFailed { run_id: i64, error: String },
    ReconnectRequired { error: String },
    NewContent { content_id: i64, title: String },
    NewComments { content_id: i64, title: String, delta: i64, total: i64 },
    PublishFailed { variant_id: i64, error: String },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Fired {
    pub automation_id: i64,
    pub title: String,
    pub body: String,
    pub desktop: bool,
}

pub fn validate(rule: &Rule) -> AppResult<()> {
    match &rule.trigger {
        Trigger::MetricThreshold { metric, .. } | Trigger::ContentThreshold { metric, .. } if metric.trim().is_empty() => {
            Err(AppError::validation("metric is required"))
        }
        Trigger::MetricGain { days, .. } if !(1..=365).contains(days) => Err(AppError::validation("days must be between 1 and 365")),
        Trigger::TokenExpiring { days } if !(1..=60).contains(days) => Err(AppError::validation("days must be between 1 and 60")),
        _ if !rule.notify_in_app && !rule.notify_desktop => Err(AppError::validation("choose at least one notification action")),
        _ => Ok(()),
    }
}

pub fn defaults() -> Vec<(&'static str, Rule)> {
    let r = |trigger, desktop| Rule { trigger, platforms: vec![], account_ids: vec![], notify_in_app: true, notify_desktop: desktop };
    vec![
        ("Synchronization failed", r(Trigger::SyncFailed, false)),
        ("Account needs reconnection", r(Trigger::ReconnectRequired, true)),
        ("Authorization expiring within 7 days", r(Trigger::TokenExpiring { days: 7 }, true)),
        ("New content detected", r(Trigger::NewContent, false)),
        ("New comments", r(Trigger::NewComments, false)),
        ("Publishing failed", r(Trigger::PublishFailed, true)),
    ]
}

pub fn seed_defaults(conn: &Connection) -> AppResult<()> {
    if crate::db::get_setting(conn, "automations_seeded")?.is_some() {
        return Ok(());
    }
    for (name, rule) in defaults() {
        conn.execute("INSERT INTO automations(name, rule_json) VALUES (?1, ?2)", (name, serde_json::to_string(&rule)?))?;
    }
    crate::db::set_setting(conn, "automations_seeded", "1")
}

/// Record a firing unless this dedupe key already fired for the rule.
fn fire_once(conn: &Connection, automation_id: i64, key: &str, msg: &str) -> AppResult<bool> {
    let n = conn
        .execute("INSERT OR IGNORE INTO automation_runs(automation_id, status, message, dedupe_key) VALUES (?1, 'fired', ?2, ?3)", (automation_id, msg, key))?;
    if n > 0 {
        conn.execute("UPDATE automations SET last_fired_at = datetime('now') WHERE id = ?1", [automation_id])?;
    }
    Ok(n > 0)
}

fn latest_metric(conn: &Connection, account_id: i64, metric: &str, before_days: i64) -> AppResult<Option<f64>> {
    Ok(conn
        .query_row(
            "SELECT value FROM metric_snapshots WHERE account_id = ?1 AND content_id IS NULL AND metric = ?2
             AND day <= date('now', ?3) ORDER BY day DESC LIMIT 1",
            (account_id, metric, format!("-{before_days} days")),
            |r| r.get(0),
        )
        .optional()?)
}

/// Evaluate all enabled rules for one account after a sync/publish, given its events.
/// Returns the notifications to deliver; in-app ones are already inserted.
pub fn evaluate(conn: &Connection, account_id: i64, events: &[Event]) -> AppResult<Vec<Fired>> {
    let (platform, name, expires): (String, String, Option<i64>) =
        conn.query_row("SELECT platform, name, token_expires_at FROM accounts WHERE id = ?1", [account_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    let mut fired = vec![];
    for a in query_json(conn, "SELECT id, name, rule_json FROM automations WHERE enabled = 1", [])? {
        let id = a["id"].as_i64().unwrap_or_default();
        let Ok(rule) = serde_json::from_str::<Rule>(a["rule_json"].as_str().unwrap_or("")) else {
            conn.execute("INSERT INTO automation_runs(automation_id, status, message) VALUES (?1, 'error', 'invalid rule definition')", [id])?;
            continue;
        };
        if (!rule.platforms.is_empty() && !rule.platforms.contains(&platform)) || (!rule.account_ids.is_empty() && !rule.account_ids.contains(&account_id)) {
            continue;
        }
        let mut hits: Vec<(String, String, String)> = vec![]; // (dedupe, title, body)
        match &rule.trigger {
            Trigger::MetricThreshold { metric, value } => {
                if latest_metric(conn, account_id, metric, 0)?.is_some_and(|v| v >= *value) {
                    hits.push((format!("thr:{account_id}:{metric}:{value}"), format!("{name}: {metric} reached {value}"), String::new()));
                }
            }
            Trigger::MetricGain { metric, value, days } => {
                if let (Some(now), Some(then)) = (latest_metric(conn, account_id, metric, 0)?, latest_metric(conn, account_id, metric, *days)?) {
                    if now - then >= *value {
                        let week = chrono::Utc::now().format("%G-W%V");
                        hits.push((format!("gain:{account_id}:{metric}:{week}"), format!("{name}: +{} {metric} in {days} days", now - then), String::new()));
                    }
                }
            }
            Trigger::ContentThreshold { metric, value } => {
                let path = format!("$.{metric}");
                for c in query_json(
                    conn,
                    "SELECT id, title FROM content_items WHERE account_id = ?1 AND json_extract(stats_json, ?2) >= ?3",
                    (account_id, &path, value),
                )? {
                    let cid = c["id"].as_i64().unwrap_or_default();
                    let t = c["title"].as_str().unwrap_or("content").to_string();
                    hits.push((format!("cthr:{cid}:{metric}:{value}"), format!("“{t}” reached {value} {metric}"), name.clone()));
                }
            }
            Trigger::TokenExpiring { days } => {
                if let Some(exp) = expires {
                    let left = exp - chrono::Utc::now().timestamp();
                    if left > 0 && left < days * 86_400 {
                        hits.push((
                            format!("exp:{account_id}:{exp}"),
                            format!("{name}: authorization expires soon"),
                            "Reconnect the account in Accounts to keep syncing.".into(),
                        ));
                    }
                }
            }
            t => {
                for e in events {
                    let h = match (t, e) {
                        (Trigger::SyncFailed, Event::SyncFailed { run_id, error }) => {
                            (format!("fail:{run_id}"), format!("{name}: synchronization failed"), error.clone())
                        }
                        (Trigger::ReconnectRequired, Event::ReconnectRequired { error }) => {
                            (format!("reconn:{account_id}:{}", chrono::Utc::now().date_naive()), format!("{name} needs to be reconnected"), error.clone())
                        }
                        (Trigger::NewContent, Event::NewContent { content_id, title }) => {
                            (format!("new:{content_id}"), format!("New on {name}"), title.clone())
                        }
                        (Trigger::NewComments, Event::NewComments { content_id, title, delta, total }) => {
                            (format!("cmt:{content_id}:{total}"), format!("{delta} new comment(s) on “{title}”"), name.clone())
                        }
                        (Trigger::PublishFailed, Event::PublishFailed { variant_id, error }) => {
                            (format!("pub:{variant_id}:{}", chrono::Utc::now().timestamp()), format!("Publishing to {name} failed"), error.clone())
                        }
                        _ => continue,
                    };
                    hits.push(h);
                }
            }
        }
        for (key, title, body) in hits {
            if fire_once(conn, id, &key, &title)? {
                if rule.notify_in_app {
                    let kind = serde_json::to_value(&rule.trigger)?["type"].as_str().unwrap_or("automation").to_string();
                    conn.execute("INSERT INTO notifications(kind, title, body, account_id) VALUES (?1, ?2, ?3, ?4)", (&kind, &title, &body, account_id))?;
                }
                fired.push(Fired { automation_id: id, title, body, desktop: rule.notify_desktop });
            }
        }
    }
    Ok(fired)
}

pub fn parse_rule(v: &Value) -> AppResult<Rule> {
    let r: Rule = serde_json::from_value(v.clone())?;
    validate(&r)?;
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{put_metric, Db};

    fn setup() -> (Db, i64) {
        let db = Db::open_in_memory().unwrap();
        let id = {
            let c = db.conn();
            c.execute("INSERT INTO accounts(platform, account_type, external_id, name) VALUES ('youtube','channel','UC','Chan')", []).unwrap();
            c.last_insert_rowid()
        };
        (db, id)
    }

    fn add(c: &Connection, rule: Rule) -> i64 {
        c.execute("INSERT INTO automations(name, rule_json) VALUES ('r', ?1)", [serde_json::to_string(&rule).unwrap()]).unwrap();
        c.last_insert_rowid()
    }

    fn rule(trigger: Trigger) -> Rule {
        Rule { trigger, platforms: vec![], account_ids: vec![], notify_in_app: true, notify_desktop: false }
    }

    #[test]
    fn threshold_fires_once() {
        let (db, a) = setup();
        let c = db.conn();
        add(&c, rule(Trigger::MetricThreshold { metric: "subscribers".into(), value: 100.0 }));
        put_metric(&c, a, None, "subscribers", 99.0, &chrono::Utc::now().date_naive().to_string(), "t").unwrap();
        assert!(evaluate(&c, a, &[]).unwrap().is_empty());
        put_metric(&c, a, None, "subscribers", 120.0, &chrono::Utc::now().date_naive().to_string(), "t").unwrap();
        assert_eq!(evaluate(&c, a, &[]).unwrap().len(), 1);
        assert!(evaluate(&c, a, &[]).unwrap().is_empty(), "must not fire twice");
        let n: i64 = c.query_row("SELECT count(*) FROM notifications", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn content_threshold_and_platform_filter() {
        let (db, a) = setup();
        let c = db.conn();
        c.execute("INSERT INTO content_items(account_id, external_id, kind, title, stats_json) VALUES (?1, 'v', 'video', 'Vid', '{\"views\": 15000}')", [a])
            .unwrap();
        let mut r = rule(Trigger::ContentThreshold { metric: "views".into(), value: 10000.0 });
        r.platforms = vec!["spotify".into()];
        let id = add(&c, r.clone());
        assert!(evaluate(&c, a, &[]).unwrap().is_empty());
        r.platforms = vec!["youtube".into()];
        c.execute("UPDATE automations SET rule_json = ?1 WHERE id = ?2", (serde_json::to_string(&r).unwrap(), id)).unwrap();
        let f = evaluate(&c, a, &[]).unwrap();
        assert_eq!(f.len(), 1);
        assert!(f[0].title.contains("Vid"));
    }

    #[test]
    fn gain_over_window() {
        let (db, a) = setup();
        let c = db.conn();
        add(&c, rule(Trigger::MetricGain { metric: "subscribers".into(), value: 100.0, days: 7 }));
        let old = (chrono::Utc::now().date_naive() - chrono::Duration::days(8)).to_string();
        put_metric(&c, a, None, "subscribers", 1000.0, &old, "t").unwrap();
        put_metric(&c, a, None, "subscribers", 1150.0, &chrono::Utc::now().date_naive().to_string(), "t").unwrap();
        assert_eq!(evaluate(&c, a, &[]).unwrap().len(), 1);
    }

    #[test]
    fn event_rules_and_desktop_flag() {
        let (db, a) = setup();
        let c = db.conn();
        let mut r = rule(Trigger::SyncFailed);
        r.notify_desktop = true;
        add(&c, r);
        let f = evaluate(&c, a, &[Event::SyncFailed { run_id: 9, error: "boom".into() }, Event::NewContent { content_id: 1, title: "x".into() }]).unwrap();
        assert_eq!(f.len(), 1);
        assert!(f[0].desktop);
        assert_eq!(f[0].body, "boom");
    }

    #[test]
    fn disabled_rules_do_not_fire_and_validation() {
        let (db, a) = setup();
        let c = db.conn();
        let id = add(&c, rule(Trigger::NewContent));
        c.execute("UPDATE automations SET enabled = 0 WHERE id = ?1", [id]).unwrap();
        assert!(evaluate(&c, a, &[Event::NewContent { content_id: 1, title: "x".into() }]).unwrap().is_empty());
        let mut bad = rule(Trigger::NewContent);
        bad.notify_in_app = false;
        assert!(validate(&bad).is_err());
        assert!(validate(&rule(Trigger::TokenExpiring { days: 0 })).is_err());
    }

    #[test]
    fn seeds_defaults_once() {
        let (db, _) = setup();
        let c = db.conn();
        seed_defaults(&c).unwrap();
        seed_defaults(&c).unwrap();
        let n: i64 = c.query_row("SELECT count(*) FROM automations", [], |r| r.get(0)).unwrap();
        assert_eq!(n as usize, defaults().len());
    }
}
