//! Import metrics from CSV exports (e.g. Spotify for Creators analytics downloads,
//! which have no public API). Columns are detected from headers; nothing is estimated.
use crate::error::{AppError, AppResult};
use chrono::NaiveDate;
use serde::Serialize;

pub const MAX_BYTES: usize = 20 * 1024 * 1024;

#[derive(Debug, Serialize, PartialEq)]
pub struct CsvRow {
    pub day: Option<String>,
    pub episode: Option<String>,
    pub metric: String,
    pub value: f64,
}

#[derive(Debug, Serialize, Default)]
pub struct CsvParsed {
    pub rows: Vec<CsvRow>,
    pub metrics: Vec<String>,
    pub ignored_columns: Vec<String>,
    pub skipped_rows: usize,
}

fn norm(h: &str) -> String {
    h.trim().trim_start_matches('\u{feff}').to_lowercase()
}

/// Known header names (EN/PT) → canonical metric.
fn metric_for(h: &str) -> Option<String> {
    let m = match h {
        "plays" | "streams" | "reproduções" | "reproducoes" => "plays",
        "starts" | "inícios" => "starts",
        "listeners" | "ouvintes" => "listeners",
        "followers" | "seguidores" => "followers",
        "audience size" | "audience" => "audience_size",
        "consumption hours" | "hours" | "horas" => "consumption_hours",
        "impressions" => "impressions",
        _ => return None,
    };
    Some(m.into())
}

pub fn parse_date(s: &str) -> Option<String> {
    let s = s.trim();
    for fmt in ["%Y-%m-%d", "%Y/%m/%d", "%d.%m.%Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(s, fmt) {
            return Some(d.to_string());
        }
    }
    if let Some(day) = s.get(..10).and_then(|p| NaiveDate::parse_from_str(p, "%Y-%m-%d").ok()) {
        return Some(day.to_string()); // timestamps like 2026-01-01T00:00:00Z
    }
    // Slash dates: unambiguous when the first part > 12, otherwise US order (Spotify's export format).
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() == 3 {
        let a: u32 = parts[0].parse().ok()?;
        let fmt = if a > 12 { "%d/%m/%Y" } else { "%m/%d/%Y" };
        return NaiveDate::parse_from_str(s, fmt).ok().map(|d| d.to_string());
    }
    None
}

pub fn parse(content: &str) -> AppResult<CsvParsed> {
    if content.len() > MAX_BYTES {
        return Err(AppError::validation("CSV larger than 20 MB"));
    }
    let delim = if content.lines().next().is_some_and(|l| l.matches(';').count() > l.matches(',').count()) { b';' } else { b',' };
    let mut rdr = csv::ReaderBuilder::new().delimiter(delim).flexible(true).from_reader(content.as_bytes());
    let headers: Vec<String> = rdr.headers().map_err(|e| AppError::validation(format!("invalid CSV: {e}")))?.iter().map(norm).collect();
    let date_col = headers.iter().position(|h| ["date", "day", "data", "dia", "release date"].contains(&h.as_str()));
    let ep_col = headers.iter().position(|h| ["episode", "episode name", "episode title", "title", "episódio", "episodio", "name"].contains(&h.as_str()));
    let metric_cols: Vec<(usize, String)> = headers.iter().enumerate().filter_map(|(i, h)| metric_for(h).map(|m| (i, m))).collect();
    if metric_cols.is_empty() {
        return Err(AppError::validation(format!(
            "no metric columns recognised. Expected headers such as Plays, Streams, Starts, Listeners, Followers. Found: {}",
            headers.join(", ")
        )));
    }
    if date_col.is_none() && ep_col.is_none() {
        return Err(AppError::validation("CSV needs a Date column or an Episode column"));
    }
    let mut out = CsvParsed {
        metrics: metric_cols.iter().map(|(_, m)| m.clone()).collect(),
        ignored_columns: headers
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != date_col && Some(*i) != ep_col && !metric_cols.iter().any(|(c, _)| c == i))
            .map(|(_, h)| h.clone())
            .collect(),
        ..Default::default()
    };
    for rec in rdr.records() {
        let Ok(rec) = rec else {
            out.skipped_rows += 1;
            continue;
        };
        let day = match date_col {
            Some(c) => match rec.get(c).and_then(parse_date) {
                Some(d) => Some(d),
                None => {
                    out.skipped_rows += 1;
                    continue;
                }
            },
            None => None,
        };
        let episode = ep_col.and_then(|c| rec.get(c)).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        for (c, m) in &metric_cols {
            let raw = rec.get(*c).unwrap_or("").trim().replace([' ', '\u{a0}'], "");
            // "1,234" thousands separators are common in exports.
            let cleaned = raw.replace(',', "");
            if let Ok(v) = cleaned.parse::<f64>() {
                out.rows.push(CsvRow { day: day.clone(), episode: episode.clone(), metric: m.clone(), value: v });
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_plays_export() {
        let p = parse("\u{feff}Date,Plays,Listeners,Notes\n2026-01-01,1,200,x\n01/02/2026,\"1,500\",300,\nbad,1,1,\n").unwrap();
        assert_eq!(p.metrics, ["plays", "listeners"]);
        assert_eq!(p.ignored_columns, ["notes"]);
        assert_eq!(p.skipped_rows, 1);
        assert_eq!(p.rows[2], CsvRow { day: Some("2026-01-02".into()), episode: None, metric: "plays".into(), value: 1500.0 });
    }

    #[test]
    fn episode_totals_semicolon() {
        let p = parse("Episode;Streams\nEp 1;10\nEp 2;20\n").unwrap();
        assert_eq!(p.rows.len(), 2);
        assert_eq!(p.rows[1].episode.as_deref(), Some("Ep 2"));
        assert!(p.rows[1].day.is_none());
    }

    #[test]
    fn rejects_unknown_columns() {
        assert!(parse("foo,bar\n1,2\n").unwrap_err().to_string().contains("Found: foo, bar"));
    }

    #[test]
    fn dates() {
        assert_eq!(parse_date("25/12/2026").as_deref(), Some("2026-12-25"));
        assert_eq!(parse_date("2026-03-01T10:00:00Z").as_deref(), Some("2026-03-01"));
        assert_eq!(parse_date("13/13/2026"), None);
    }
}
