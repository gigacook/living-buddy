//! Input validation helpers that turn bad input into field-level errors.

use crate::error::{AppError, AppResult};
use chrono::{NaiveDate, NaiveTime};
use chrono_tz::Tz;
use tendly_core::model::clean_text;
use tendly_core::recurrence::RRule;

pub fn title(field: &str, raw: &str, max: usize) -> AppResult<String> {
    let t = clean_text(raw, max, false);
    if t.is_empty() {
        return Err(AppError::field(field, "Please add a title."));
    }
    Ok(t)
}

pub fn opt_text(raw: Option<&str>, max: usize, multiline: bool) -> Option<String> {
    raw.map(|s| clean_text(s, max, multiline)).filter(|s| !s.is_empty())
}

pub fn date(field: &str, raw: Option<&str>) -> AppResult<Option<String>> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map(|d| Some(d.format("%Y-%m-%d").to_string()))
            .map_err(|_| AppError::field(field, "Use a date like 2026-10-31.")),
    }
}

pub fn time(field: &str, raw: Option<&str>) -> AppResult<Option<String>> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => NaiveTime::parse_from_str(s, "%H:%M")
            .map(|t| Some(t.format("%H:%M").to_string()))
            .map_err(|_| AppError::field(field, "Use a 24-hour time like 18:30.")),
    }
}

pub fn timezone(raw: Option<&str>, default: &str) -> AppResult<String> {
    let tz = raw.map(str::trim).filter(|s| !s.is_empty()).unwrap_or(default);
    tz.parse::<Tz>().map(|t| t.name().to_string()).map_err(|_| AppError::field("timezone", "Unknown time zone."))
}

pub fn recurrence(raw: Option<&str>) -> AppResult<Option<String>> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => RRule::parse(s)
            .map(|r| Some(r.to_string()))
            .map_err(|e| AppError::field("recurrence", format!("Repeat rule: {e}."))),
    }
}

pub fn tags(raw: &[String]) -> AppResult<Vec<String>> {
    let mut out: Vec<String> = raw
        .iter()
        .map(|t| clean_text(t, 32, false).to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    out.sort();
    out.dedup();
    if out.len() > 20 {
        return Err(AppError::field("tags", "Up to 20 tags."));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validators() {
        assert!(title("title", "   ", 10).is_err());
        assert_eq!(title("title", " Dishes ", 100).unwrap(), "Dishes");
        assert!(date("d", Some("2026-02-30")).is_err());
        assert_eq!(date("d", Some("")).unwrap(), None);
        assert!(time("t", Some("25:00")).is_err());
        assert_eq!(timezone(None, "Europe/Stockholm").unwrap(), "Europe/Stockholm");
        assert!(timezone(Some("Nowhere/City"), "UTC").is_err());
        assert_eq!(recurrence(Some("RRULE:FREQ=WEEKLY;BYDAY=MO")).unwrap().unwrap(), "FREQ=WEEKLY;BYDAY=MO");
        assert!(recurrence(Some("FREQ=SOMETIMES")).is_err());
        assert_eq!(tags(&["Home".into(), "home".into(), " x ".into()]).unwrap(), vec!["home", "x"]);
    }
}
