//! Report export to CSV and Excel. PDF is produced by the frontend's print view
//! (the system "Save as PDF" printer), which keeps charts vector-sharp.
use crate::error::{AppError, AppResult};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;

#[derive(Deserialize)]
pub struct Sheet {
    pub name: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

#[derive(Deserialize)]
pub struct Report {
    pub title: String,
    pub sheets: Vec<Sheet>,
}

/// Neutralise spreadsheet formula injection (CWE-1236) in text cells.
pub fn safe_text(s: &str) -> String {
    if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{s}")
    } else {
        s.to_string()
    }
}

fn cell_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => safe_text(s),
        other => other.to_string(),
    }
}

pub fn to_csv(r: &Report) -> AppResult<String> {
    let mut w = csv::Writer::from_writer(vec![]);
    for (i, s) in r.sheets.iter().enumerate() {
        if r.sheets.len() > 1 {
            if i > 0 {
                w.write_record([""]).map_err(csv_err)?;
            }
            w.write_record([safe_text(&s.name)]).map_err(csv_err)?;
        }
        w.write_record(s.columns.iter().map(|c| safe_text(c))).map_err(csv_err)?;
        for row in &s.rows {
            w.write_record(row.iter().map(cell_text)).map_err(csv_err)?;
        }
    }
    String::from_utf8(w.into_inner().map_err(|e| AppError::Io { message: e.to_string() })?).map_err(|e| AppError::Io { message: e.to_string() })
}

fn csv_err(e: csv::Error) -> AppError {
    AppError::Io { message: e.to_string() }
}

pub fn write_xlsx(r: &Report, path: &Path) -> AppResult<()> {
    use rust_xlsxwriter::{Format, Workbook};
    let mut wb = Workbook::new();
    let bold = Format::new().set_bold();
    let x = |e: rust_xlsxwriter::XlsxError| AppError::Io { message: e.to_string() };
    for s in &r.sheets {
        let ws = wb.add_worksheet();
        // Excel sheet names: ≤31 chars, no []:*?/\
        let name: String = s.name.chars().filter(|c| !"[]:*?/\\".contains(*c)).take(31).collect();
        ws.set_name(if name.is_empty() { "Sheet" } else { &name }).map_err(x)?;
        for (c, h) in s.columns.iter().enumerate() {
            ws.write_string_with_format(0, c as u16, h, &bold).map_err(x)?;
        }
        for (ri, row) in s.rows.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                let (r, c) = (ri as u32 + 1, c as u16);
                match v {
                    Value::Number(n) => ws.write_number(r, c, n.as_f64().unwrap_or_default()).map(|_| ()),
                    Value::Bool(b) => ws.write_boolean(r, c, *b).map(|_| ()),
                    Value::Null => Ok(()),
                    // write_string never evaluates formulas, so no escaping is needed here.
                    Value::String(s) => ws.write_string(r, c, s).map(|_| ()),
                    other => ws.write_string(r, c, other.to_string()).map(|_| ()),
                }
                .map_err(x)?;
            }
        }
        ws.autofit();
    }
    wb.save(path).map_err(x)
}

pub fn export(r: &Report, path: &Path, format: &str) -> AppResult<()> {
    if r.sheets.is_empty() {
        return Err(AppError::validation("report has no data"));
    }
    match format {
        "csv" => Ok(std::fs::write(path, to_csv(r)?)?),
        "xlsx" => write_xlsx(r, path),
        _ => Err(AppError::validation("format must be csv or xlsx")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn report() -> Report {
        serde_json::from_value(
            json!({"title":"T","sheets":[{"name":"Overview","columns":["Day","Views"],"rows":[["2026-01-01",10],["=HYPERLINK(\"x\")",null]]}]}),
        )
        .unwrap()
    }

    #[test]
    fn csv_escapes_formulas() {
        let s = to_csv(&report()).unwrap();
        assert!(s.starts_with("Day,Views\n2026-01-01,10\n"));
        assert!(s.contains("'=HYPERLINK"));
    }

    #[test]
    fn xlsx_is_written() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("r.xlsx");
        export(&report(), &p, "xlsx").unwrap();
        assert!(std::fs::metadata(&p).unwrap().len() > 1000);
        assert!(export(&report(), &p, "exe").is_err());
    }
}
