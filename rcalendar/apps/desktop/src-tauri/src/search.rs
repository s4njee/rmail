//! Full-text search and natural-language date parsing for ⌘K search (S2.3, S6.2).

use chrono::{Datelike, Days, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

use calendar_core::model::{Event, Task};

/// Result payload for the `search` command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResults {
    /// Matched events by text or date.
    pub events: Vec<Event>,
    /// Matched tasks by text.
    pub tasks: Vec<Task>,
    /// Natural-language date if one was parsed from the query.
    pub matched_date: Option<NaiveDate>,
}

/// Attempts to parse a date from natural-language queries like "today", "tomorrow",
/// "next tuesday", "aug 13", "2026-08-13".
pub fn parse_date_query(query: &str, reference_date: NaiveDate) -> Option<NaiveDate> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return None;
    }

    match q.as_str() {
        "today" => return Some(reference_date),
        "tomorrow" => return reference_date.checked_add_days(Days::new(1)),
        "yesterday" => return reference_date.checked_sub_days(Days::new(1)),
        _ => {}
    }

    // "next monday", "this friday", etc.
    let (prefix, rest) = if let Some(stripped) = q.strip_prefix("next ") {
        ("next", stripped.trim())
    } else if let Some(stripped) = q.strip_prefix("this ") {
        ("this", stripped.trim())
    } else {
        ("", q.as_str())
    };

    if let Some(target_weekday) = parse_weekday(rest) {
        let current_weekday = reference_date.weekday();
        let days_until = (target_weekday.num_days_from_monday() + 7
            - current_weekday.num_days_from_monday())
            % 7;
        let add = if prefix == "next" {
            if days_until == 0 {
                7
            } else {
                days_until + 7
            }
        } else if days_until == 0 {
            0
        } else {
            days_until
        };
        return reference_date.checked_add_days(Days::new(add as u64));
    }

    // Try ISO format: YYYY-MM-DD
    if let Ok(d) = NaiveDate::parse_from_str(&q, "%Y-%m-%d") {
        return Some(d);
    }
    if let Ok(d) = NaiveDate::parse_from_str(&q, "%Y/%m/%d") {
        return Some(d);
    }
    if let Ok(d) = NaiveDate::parse_from_str(&q, "%m/%d/%Y") {
        return Some(d);
    }
    if let Ok(d) = NaiveDate::parse_from_str(&q, "%d/%m/%Y") {
        return Some(d);
    }

    // Month + Day, e.g. "aug 13", "august 15", "13 aug", "august 13 2026"
    parse_month_day(&q, reference_date.year())
}

fn parse_weekday(s: &str) -> Option<Weekday> {
    match s {
        "monday" | "mon" | "mo" => Some(Weekday::Mon),
        "tuesday" | "tue" | "tues" | "tu" => Some(Weekday::Tue),
        "wednesday" | "wed" | "we" => Some(Weekday::Wed),
        "thursday" | "thu" | "thur" | "thurs" | "th" => Some(Weekday::Thu),
        "friday" | "fri" | "fr" => Some(Weekday::Fri),
        "saturday" | "sat" | "sa" => Some(Weekday::Sat),
        "sunday" | "sun" | "su" => Some(Weekday::Sun),
        _ => None,
    }
}

fn parse_month_name(s: &str) -> Option<u32> {
    match s {
        "jan" | "january" => Some(1),
        "feb" | "february" => Some(2),
        "mar" | "march" => Some(3),
        "apr" | "april" => Some(4),
        "may" => Some(5),
        "jun" | "june" => Some(6),
        "jul" | "july" => Some(7),
        "aug" | "august" => Some(8),
        "sep" | "september" => Some(9),
        "oct" | "october" => Some(10),
        "nov" | "november" => Some(11),
        "dec" | "december" => Some(12),
        _ => None,
    }
}

fn parse_month_day(s: &str, default_year: i32) -> Option<NaiveDate> {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.len() == 2 {
        // "aug 13" or "13 aug"
        if let (Some(m), Ok(d)) = (parse_month_name(tokens[0]), tokens[1].parse::<u32>()) {
            return NaiveDate::from_ymd_opt(default_year, m, d);
        }
        if let (Ok(d), Some(m)) = (tokens[0].parse::<u32>(), parse_month_name(tokens[1])) {
            return NaiveDate::from_ymd_opt(default_year, m, d);
        }
    } else if tokens.len() == 3 {
        // "august 13 2026" or "13 august 2026"
        if let (Some(m), Ok(d), Ok(y)) = (
            parse_month_name(tokens[0]),
            tokens[1].parse::<u32>(),
            tokens[2].parse::<i32>(),
        ) {
            return NaiveDate::from_ymd_opt(y, m, d);
        }
        if let (Ok(d), Some(m), Ok(y)) = (
            tokens[0].parse::<u32>(),
            parse_month_name(tokens[1]),
            tokens[2].parse::<i32>(),
        ) {
            return NaiveDate::from_ymd_opt(y, m, d);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_relative_and_named_dates() {
        let ref_date = NaiveDate::from_ymd_opt(2026, 8, 13).unwrap(); // Thursday

        assert_eq!(parse_date_query("today", ref_date), Some(ref_date));
        assert_eq!(
            parse_date_query("tomorrow", ref_date),
            Some(NaiveDate::from_ymd_opt(2026, 8, 14).unwrap())
        );
        assert_eq!(
            parse_date_query("yesterday", ref_date),
            Some(NaiveDate::from_ymd_opt(2026, 8, 12).unwrap())
        );
        assert_eq!(
            parse_date_query("friday", ref_date),
            Some(NaiveDate::from_ymd_opt(2026, 8, 14).unwrap())
        );
        assert_eq!(
            parse_date_query("next monday", ref_date),
            Some(NaiveDate::from_ymd_opt(2026, 8, 24).unwrap()) // next monday after coming monday
        );
        assert_eq!(
            parse_date_query("aug 15", ref_date),
            Some(NaiveDate::from_ymd_opt(2026, 8, 15).unwrap())
        );
        assert_eq!(
            parse_date_query("2026-08-20", ref_date),
            Some(NaiveDate::from_ymd_opt(2026, 8, 20).unwrap())
        );
        assert_eq!(parse_date_query("Dentist appointment", ref_date), None);
    }
}
