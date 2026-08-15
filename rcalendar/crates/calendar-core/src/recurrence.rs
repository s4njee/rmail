//! Recurrence: RRULE parsing and expansion (S1.4), plus EXDATE handling and
//! scoped per-occurrence edits (S1.5).
//!
//! RRULEs are stored as raw RFC 5545 strings on [`Event`]; this module parses
//! them into a small typed [`Rrule`] for inspection/mutation and expands them
//! to [`Occurrence`]s using the battle-tested `rrule` crate. Exceptions are
//! modelled the iCal way: an event's `exdates` remove instances, and a
//! per-instance override is a separate non-recurring event (see
//! [`edit_occurrence`]).

use std::collections::HashSet;

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc, Weekday};
use rrule::{Frequency as CrateFrequency, NWeekday, RRule, RRuleSet, Tz};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::model::{Event, Occurrence, TimeRange};

/// The recurrence frequency, restricted to the calendar-relevant subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl Frequency {
    fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_uppercase().as_str() {
            "DAILY" => Ok(Self::Daily),
            "WEEKLY" => Ok(Self::Weekly),
            "MONTHLY" => Ok(Self::Monthly),
            "YEARLY" => Ok(Self::Yearly),
            other => Err(Error::InvalidRrule(format!(
                "unsupported FREQ {other:?} (supported: DAILY, WEEKLY, MONTHLY, YEARLY)"
            ))),
        }
    }

    fn to_crate(self) -> CrateFrequency {
        match self {
            Self::Daily => CrateFrequency::Daily,
            Self::Weekly => CrateFrequency::Weekly,
            Self::Monthly => CrateFrequency::Monthly,
            Self::Yearly => CrateFrequency::Yearly,
        }
    }
}

/// A parsed RRULE, supporting the S1.4 surface: `FREQ`, `INTERVAL`, `BYDAY`,
/// `BYMONTHDAY`, `UNTIL`, and `COUNT`. Unknown rule parts are ignored per
/// RFC 5545. `BYDAY` ordinals (e.g. `2MO`) are rejected as out of scope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rrule {
    pub freq: Frequency,
    pub interval: u32,
    /// Plain weekdays; ordinal forms are not supported.
    pub by_day: Vec<Weekday>,
    /// May be negative (e.g. `-1` = last day of the month).
    pub by_month_day: Vec<i8>,
    /// `UNTIL` as a UTC instant (date-only values are treated as inclusive
    /// through the end of that day).
    pub until: Option<DateTime<Utc>>,
    pub count: Option<u32>,
}

impl Rrule {
    /// Parses an RFC 5545 `RRULE` value (without the `RRULE:` prefix).
    pub fn parse(s: &str) -> Result<Self> {
        let mut freq = None;
        let mut interval = 1;
        let mut by_day = Vec::new();
        let mut by_month_day = Vec::new();
        let mut until = None;
        let mut count = None;

        for part in s.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let (key, value) = part.split_once('=').ok_or_else(|| {
                Error::InvalidRrule(format!("malformed rule part {part:?} (expected KEY=VALUE)"))
            })?;
            match key.trim().to_ascii_uppercase().as_str() {
                "FREQ" => freq = Some(Frequency::parse(value)?),
                "INTERVAL" => {
                    interval = value.trim().parse().map_err(|_| {
                        Error::InvalidRrule(format!("INTERVAL {value:?} is not a number"))
                    })?
                }
                "BYDAY" => by_day = parse_by_day(value)?,
                "BYMONTHDAY" => by_month_day = parse_by_month_day(value)?,
                "UNTIL" => until = Some(parse_until(value)?),
                "COUNT" => {
                    count = Some(value.trim().parse().map_err(|_| {
                        Error::InvalidRrule(format!("COUNT {value:?} is not a number"))
                    })?)
                }
                // Unknown rule parts are ignored (RFC 5545 §3.3.10).
                _ => {}
            }
        }

        let freq = freq.ok_or_else(|| Error::InvalidRrule("missing FREQ".into()))?;
        if interval == 0 {
            return Err(Error::InvalidRrule("INTERVAL must be at least 1".into()));
        }
        if count.is_some() && until.is_some() {
            return Err(Error::InvalidRrule(
                "RRULE cannot carry both COUNT and UNTIL".into(),
            ));
        }

        Ok(Self {
            freq,
            interval,
            by_day,
            by_month_day,
            until,
            count,
        })
    }
}

impl std::fmt::Display for Rrule {
    /// Serializes back to RFC 5545 `RRULE` syntax.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = vec![match self.freq {
            Frequency::Daily => "FREQ=DAILY",
            Frequency::Weekly => "FREQ=WEEKLY",
            Frequency::Monthly => "FREQ=MONTHLY",
            Frequency::Yearly => "FREQ=YEARLY",
        }
        .to_string()];

        if self.interval != 1 {
            parts.push(format!("INTERVAL={}", self.interval));
        }
        if !self.by_day.is_empty() {
            let days = self
                .by_day
                .iter()
                .map(|&day| weekday_name(day))
                .collect::<Vec<_>>()
                .join(",");
            parts.push(format!("BYDAY={days}"));
        }
        if !self.by_month_day.is_empty() {
            let days = self
                .by_month_day
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",");
            parts.push(format!("BYMONTHDAY={days}"));
        }
        if let Some(until) = self.until {
            parts.push(format!("UNTIL={}", until.format("%Y%m%dT%H%M%SZ")));
        }
        if let Some(count) = self.count {
            parts.push(format!("COUNT={count}"));
        }
        write!(f, "{}", parts.join(";"))
    }
}

fn parse_by_day(value: &str) -> Result<Vec<Weekday>> {
    value
        .split(',')
        .map(|token| {
            let token = token.trim();
            // Reject ordinal forms like "2MO" / "-1FR" — out of scope for v1.
            if token.chars().any(|c| c.is_ascii_digit()) {
                return Err(Error::InvalidRrule(format!(
                    "BYDAY ordinal {token:?} is not supported"
                )));
            }
            weekday_from_name(token)
                .ok_or_else(|| Error::InvalidRrule(format!("unknown weekday {token:?}")))
        })
        .collect()
}

fn parse_by_month_day(value: &str) -> Result<Vec<i8>> {
    value
        .split(',')
        .map(|token| {
            token.trim().parse::<i8>().map_err(|_| {
                Error::InvalidRrule(format!("BYMONTHDAY {token:?} is not a day number"))
            })
        })
        .collect()
}

fn parse_until(value: &str) -> Result<DateTime<Utc>> {
    if value.contains('T') {
        let naive = chrono::NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%SZ")
            .or_else(|_| chrono::NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S"))
            .map_err(|_| Error::InvalidRrule(format!("UNTIL {value:?} is not a DATE-TIME")))?;
        Ok(Utc.from_utc_datetime(&naive))
    } else {
        let date = chrono::NaiveDate::parse_from_str(value, "%Y%m%d")
            .map_err(|_| Error::InvalidRrule(format!("UNTIL {value:?} is not a DATE")))?;
        // Date-only UNTIL bounds the whole day inclusively → exclusive upper
        // bound is midnight of the next day.
        Ok(Utc.from_utc_datetime(&date.succ_opt().unwrap().and_hms_opt(0, 0, 0).unwrap()))
    }
}

fn weekday_from_name(name: &str) -> Option<Weekday> {
    match name.to_ascii_uppercase().as_str() {
        "MO" => Some(Weekday::Mon),
        "TU" => Some(Weekday::Tue),
        "WE" => Some(Weekday::Wed),
        "TH" => Some(Weekday::Thu),
        "FR" => Some(Weekday::Fri),
        "SA" => Some(Weekday::Sat),
        "SU" => Some(Weekday::Sun),
        _ => None,
    }
}

fn weekday_name(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "MO",
        Weekday::Tue => "TU",
        Weekday::Wed => "WE",
        Weekday::Thu => "TH",
        Weekday::Fri => "FR",
        Weekday::Sat => "SA",
        Weekday::Sun => "SU",
    }
}

/// Validates an RRULE string without expanding anything.
pub fn validate_rrule(s: &str) -> Result<()> {
    Rrule::parse(s).map(|_| ())
}

/// The timezone an event's wall-clock times live in, or UTC when floating.
fn event_tz(event: &Event) -> Result<rrule::Tz> {
    match &event.tz {
        Some(name) => name
            .parse::<chrono_tz::Tz>()
            .map(Tz::Tz)
            .map_err(|_| Error::InvalidEvent(format!("unknown timezone {name:?}"))),
        None => Ok(Tz::Tz(chrono_tz::UTC)),
    }
}

/// Builds the `rrule` crate's rule set for `event`'s rule.
fn build_rule_set(event: &Event, rule: &Rrule) -> Result<RRuleSet> {
    let tz = event_tz(event)?;
    let dt_start: DateTime<Tz> = event.starts_at.with_timezone(&tz);

    let mut builder = RRule::new(rule.freq.to_crate());
    if rule.interval != 1 {
        let interval = u16::try_from(rule.interval)
            .map_err(|_| Error::InvalidRrule("INTERVAL is too large".into()))?;
        builder = builder.interval(interval);
    }
    if !rule.by_day.is_empty() {
        let days = rule
            .by_day
            .iter()
            .map(|&day| NWeekday::new(None, day))
            .collect::<Vec<_>>();
        builder = builder.by_weekday(days);
    }
    if !rule.by_month_day.is_empty() {
        builder = builder.by_month_day(rule.by_month_day.clone());
    }
    if let Some(until) = rule.until {
        builder = builder.until(until.with_timezone(&tz));
    }
    if let Some(count) = rule.count {
        builder = builder.count(count);
    }
    builder
        .build(dt_start)
        .map_err(|e| Error::InvalidRrule(e.to_string()))
}

/// The calendar date an occurrence is anchored to for EXDATE matching.
fn occurrence_local_date(start: DateTime<Utc>, event: &Event) -> NaiveDate {
    if event.all_day {
        return start.date_naive();
    }
    match &event.tz {
        Some(name) => name
            .parse::<chrono_tz::Tz>()
            .map(|tz| start.with_timezone(&tz).date_naive())
            .unwrap_or_else(|_| start.date_naive()),
        None => start.date_naive(),
    }
}

fn is_excluded(start: DateTime<Utc>, event: &Event, exdates: &HashSet<NaiveDate>) -> bool {
    if exdates.is_empty() {
        return false;
    }
    exdates.contains(&occurrence_local_date(start, event))
}

fn occurrence(event: &Event, starts_at: DateTime<Utc>) -> Occurrence {
    Occurrence {
        event_id: event.id,
        starts_at,
        ends_at: starts_at + (event.ends_at - event.starts_at),
        all_day: event.all_day,
    }
}

/// Expands `event` into [`Occurrence`]s that overlap `range`.
///
/// A non-recurring event yields at most one occurrence. Recurring events are
/// expanded from their start time (in their own timezone, so DST never shifts
/// the wall-clock time) and instances whose date is in `event.exdates` are
/// dropped.
pub fn expand(event: &Event, range: &TimeRange) -> Result<Vec<Occurrence>> {
    event.validate()?;

    let Some(rrule_str) = &event.rrule else {
        return Ok(if range.overlaps(event.starts_at, event.ends_at) {
            vec![occurrence(event, event.starts_at)]
        } else {
            vec![]
        });
    };

    let rule = Rrule::parse(rrule_str)?;
    let rule_set = build_rule_set(event, &rule)?;
    let duration = event.ends_at - event.starts_at;
    let exdates: HashSet<NaiveDate> = event.exdates.iter().copied().collect();

    let mut occurrences = Vec::new();
    for start in rule_set.into_iter() {
        let start_utc = start.with_timezone(&Utc);
        if start_utc >= range.end {
            break; // occurrences are emitted in order; none can overlap after this
        }
        if start_utc + duration <= range.start {
            continue; // fully before the window
        }
        if !is_excluded(start_utc, event, &exdates) {
            occurrences.push(occurrence(event, start_utc));
        }
    }
    Ok(occurrences)
}

/// Expands a set of events over `range`, merged and sorted by start time.
///
/// This is the combined view a range query needs. Because a series with an
/// EXDATE drops the instance and an override is a separate event, an override
/// *replaces* rather than duplicates its series instance (S1.5).
pub fn expand_set(events: &[Event], range: &TimeRange) -> Result<Vec<Occurrence>> {
    let mut occurrences = Vec::new();
    for event in events {
        occurrences.extend(expand(event, range)?);
    }
    occurrences.sort_by_key(|o| (o.starts_at, o.ends_at, o.event_id));
    Ok(occurrences)
}

/// Finds the UTC start of the series occurrence whose local date is `target`.
fn find_occurrence_start(event: &Event, rule: &Rrule, target: NaiveDate) -> Result<DateTime<Utc>> {
    let rule_set = build_rule_set(event, rule)?;
    let mut seen = 0usize;
    for start in rule_set.into_iter() {
        seen += 1;
        if seen > 100_000 {
            return Err(Error::Recurrence(
                "could not locate occurrence within 100k instances".into(),
            ));
        }
        let start_utc = start.with_timezone(&Utc);
        let local = occurrence_local_date(start_utc, event);
        if local == target {
            return Ok(start_utc);
        }
        if local > target {
            break;
        }
    }
    Err(Error::Recurrence(format!(
        "no occurrence of this series on {target}"
    )))
}

/// Which part of a recurring series an edit applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditScope {
    /// Only the edited occurrence.
    This,
    /// The edited occurrence and everything after it.
    Future,
    /// The whole series.
    All,
}

/// Field changes for an occurrence edit. `title`/`location`/`notes` fall back
/// to the series values when `None`.
#[derive(Debug, Clone, PartialEq)]
pub struct OccurrenceChanges {
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub all_day: bool,
    pub title: Option<String>,
    pub location: Option<String>,
    pub notes: Option<String>,
}

fn apply_changes(event: &mut Event, changes: &OccurrenceChanges) {
    event.starts_at = changes.starts_at;
    event.ends_at = changes.ends_at;
    event.all_day = changes.all_day;
    if let Some(title) = &changes.title {
        event.title = title.clone();
    }
    if let Some(location) = &changes.location {
        event.location = Some(location.clone());
    }
    if let Some(notes) = &changes.notes {
        event.notes = Some(notes.clone());
    }
}

fn add_exdate(event: &mut Event, date: NaiveDate) {
    if !event.exdates.contains(&date) {
        event.exdates.push(date);
        event.exdates.sort_unstable();
    }
}

fn stamp() -> DateTime<Utc> {
    Utc::now()
}

/// Edits one occurrence of `series` with the given scope. Returns the events
/// that should replace `series` in the store:
///
/// - `All` → one modified event.
/// - `This` → the series with `date` added to `exdates`, plus a new
///   non-recurring override event at the new time.
/// - `Future` → the series truncated to end just before the edited instance,
///   plus a new recurring event at the new time carrying the same rule.
///
/// For a non-recurring event the scope is ignored and the event is simply
/// edited.
pub fn edit_occurrence(
    series: &Event,
    scope: EditScope,
    date: NaiveDate,
    changes: &OccurrenceChanges,
) -> Result<Vec<Event>> {
    series.validate()?;

    // Non-recurring events have no series semantics — just edit in place.
    let Some(rule_str) = series.rrule.as_deref() else {
        let mut edited = series.clone();
        apply_changes(&mut edited, changes);
        edited.updated_at = stamp();
        edited.validate()?;
        return Ok(vec![edited]);
    };
    if scope == EditScope::All {
        let mut edited = series.clone();
        apply_changes(&mut edited, changes);
        edited.updated_at = stamp();
        edited.validate()?;
        return Ok(vec![edited]);
    }

    let rule = Rrule::parse(rule_str)?;
    let occurrence_start = find_occurrence_start(series, &rule, date)?;

    match scope {
        EditScope::This => {
            let mut updated = series.clone();
            add_exdate(&mut updated, date);
            updated.updated_at = stamp();

            let override_event = Event {
                id: Uuid::new_v4(),
                calendar_id: series.calendar_id,
                uid: format!("{}@almanac.local", Uuid::new_v4()),
                title: changes
                    .title
                    .clone()
                    .unwrap_or_else(|| series.title.clone()),
                location: changes.location.clone().or_else(|| series.location.clone()),
                notes: changes.notes.clone().or_else(|| series.notes.clone()),
                starts_at: changes.starts_at,
                ends_at: changes.ends_at,
                all_day: changes.all_day,
                tz: series.tz.clone(),
                rrule: None,
                exdates: vec![],
                etag: None,
                updated_at: stamp(),
                created_at: stamp(),
                deleted_at: None,
            };
            override_event.validate()?;
            Ok(vec![updated, override_event])
        }
        EditScope::Future => {
            let mut updated = series.clone();
            let mut truncated_rule = rule.clone();
            truncated_rule.until = Some(occurrence_start - Duration::seconds(1));
            truncated_rule.count = None;
            updated.rrule = Some(truncated_rule.to_string());
            updated.updated_at = stamp();

            let mut head = series.clone();
            head.id = Uuid::new_v4();
            head.uid = format!("{}@almanac.local", Uuid::new_v4());
            apply_changes(&mut head, changes);
            head.rrule = Some(rule.to_string());
            head.exdates.clear();
            head.etag = None;
            head.created_at = stamp();
            head.updated_at = stamp();
            head.validate()?;
            Ok(vec![updated, head])
        }
        EditScope::All => unreachable!("handled above"),
    }
}

/// Deletes one occurrence of `series` with the given scope. Returns the events
/// that should replace `series` — empty means delete the series entirely.
///
/// - `All` → `vec![]` (delete the whole series).
/// - `This` → the series with `date` added to `exdates`.
/// - `Future` → the series truncated to end just before that instance.
pub fn delete_occurrence(series: &Event, scope: EditScope, date: NaiveDate) -> Result<Vec<Event>> {
    series.validate()?;

    let Some(rule_str) = series.rrule.as_deref() else {
        return Ok(vec![]);
    };
    if scope == EditScope::All {
        return Ok(vec![]);
    }

    let rule = Rrule::parse(rule_str)?;
    let occurrence_start = find_occurrence_start(series, &rule, date)?;

    let mut updated = series.clone();
    match scope {
        EditScope::This => add_exdate(&mut updated, date),
        EditScope::Future => {
            let mut truncated_rule = rule;
            truncated_rule.until = Some(occurrence_start - Duration::seconds(1));
            truncated_rule.count = None;
            updated.rrule = Some(truncated_rule.to_string());
        }
        EditScope::All => unreachable!("handled above"),
    }
    updated.updated_at = stamp();
    Ok(vec![updated])
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn dt(s: &str) -> DateTime<Utc> {
        Utc.from_utc_datetime(&s.parse().unwrap())
    }

    fn event(starts: &str, ends: &str, rrule: Option<&str>) -> Event {
        Event {
            id: Uuid::new_v4(),
            calendar_id: Uuid::new_v4(),
            uid: "test@example.com".into(),
            title: "Test".into(),
            location: None,
            notes: None,
            starts_at: dt(starts),
            ends_at: dt(ends),
            all_day: false,
            tz: None,
            rrule: rrule.map(str::to_string),
            exdates: vec![],
            etag: None,
            updated_at: dt("2026-08-01T00:00:00"),
            created_at: dt("2026-08-01T00:00:00"),
            deleted_at: None,
        }
    }

    fn aug() -> TimeRange {
        TimeRange::new(dt("2026-08-01T00:00:00"), dt("2026-09-01T00:00:00")).unwrap()
    }

    fn starts(occs: &[Occurrence]) -> Vec<String> {
        occs.iter()
            .map(|o| o.starts_at.format("%Y-%m-%dT%H:%M:%SZ").to_string())
            .collect()
    }

    #[test]
    fn parses_and_serializes_rrule() {
        let rule = Rrule::parse("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;COUNT=10").unwrap();
        assert_eq!(rule.freq, Frequency::Weekly);
        assert_eq!(rule.interval, 2);
        assert_eq!(rule.by_day, vec![Weekday::Mon, Weekday::Wed]);
        assert_eq!(rule.count, Some(10));
        assert_eq!(
            rule.to_string(),
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;COUNT=10"
        );
    }

    #[test]
    fn parse_rejects_ordinal_byday() {
        assert!(Rrule::parse("FREQ=MONTHLY;BYDAY=2MO").is_err());
    }

    #[test]
    fn parse_rejects_count_and_until_together() {
        assert!(Rrule::parse("FREQ=DAILY;COUNT=3;UNTIL=20260820T000000Z").is_err());
    }

    #[test]
    fn parse_ignores_unknown_rule_parts() {
        let rule = Rrule::parse("FREQ=DAILY;BYSECOND=0;X-CUSTOM=1").unwrap();
        assert_eq!(rule.freq, Frequency::Daily);
    }

    #[test]
    fn expand_weekly_byday() {
        let e = event(
            "2026-08-10T09:00:00",
            "2026-08-10T10:00:00",
            Some("FREQ=WEEKLY;BYDAY=MO,WE,FR"),
        );
        let occs = expand(&e, &aug()).unwrap();
        assert_eq!(
            starts(&occs),
            [
                "2026-08-10T09:00:00Z",
                "2026-08-12T09:00:00Z",
                "2026-08-14T09:00:00Z",
                "2026-08-17T09:00:00Z",
                "2026-08-19T09:00:00Z",
                "2026-08-21T09:00:00Z",
                "2026-08-24T09:00:00Z",
                "2026-08-26T09:00:00Z",
                "2026-08-28T09:00:00Z",
                "2026-08-31T09:00:00Z",
            ]
        );
    }

    #[test]
    fn expand_honors_until() {
        // Date-only UNTIL bounds the whole day inclusively: Aug 25 is the last
        // Tuesday included.
        let e = event(
            "2026-08-11T09:00:00",
            "2026-08-11T10:00:00",
            Some("FREQ=WEEKLY;BYDAY=TU;UNTIL=20260825"),
        );
        let occs = expand(&e, &aug()).unwrap();
        assert_eq!(
            starts(&occs),
            [
                "2026-08-11T09:00:00Z",
                "2026-08-18T09:00:00Z",
                "2026-08-25T09:00:00Z",
            ]
        );
    }

    #[test]
    fn expand_until_is_an_inclusive_instant() {
        // A DATE-TIME UNTIL is an exact bound: 00:00 on the 25th excludes the
        // 09:00 occurrence that same day.
        let e = event(
            "2026-08-11T09:00:00",
            "2026-08-11T10:00:00",
            Some("FREQ=WEEKLY;BYDAY=TU;UNTIL=20260825T000000Z"),
        );
        let occs = expand(&e, &aug()).unwrap();
        assert_eq!(
            starts(&occs),
            ["2026-08-11T09:00:00Z", "2026-08-18T09:00:00Z"]
        );
    }

    #[test]
    fn expand_honors_count() {
        let e = event(
            "2026-08-10T09:00:00",
            "2026-08-10T10:00:00",
            Some("FREQ=WEEKLY;BYDAY=MO;COUNT=4"),
        );
        let occs = expand(&e, &aug()).unwrap();
        assert_eq!(
            starts(&occs),
            [
                "2026-08-10T09:00:00Z",
                "2026-08-17T09:00:00Z",
                "2026-08-24T09:00:00Z",
                "2026-08-31T09:00:00Z",
            ]
        );
    }

    #[test]
    fn expand_daily_interval() {
        let e = event(
            "2026-08-01T08:00:00",
            "2026-08-01T09:00:00",
            Some("FREQ=DAILY;INTERVAL=2"),
        );
        let occs = expand(&e, &aug()).unwrap();
        assert_eq!(occs.len(), 16, "every other day across August");
        assert_eq!(starts(&occs)[0], "2026-08-01T08:00:00Z");
        assert_eq!(starts(&occs)[1], "2026-08-03T08:00:00Z");
        assert_eq!(starts(&occs)[15], "2026-08-31T08:00:00Z");
    }

    #[test]
    fn expand_monthly_bymonthday_skips_short_months() {
        let e = event(
            "2026-05-31T09:00:00",
            "2026-05-31T10:00:00",
            Some("FREQ=MONTHLY;BYMONTHDAY=31;COUNT=6"),
        );
        let occs = expand(&e, &aug()).unwrap();
        // May 31, Jul 31, Aug 31, Oct 31, Dec 31, Jan 31 — June/September have no 31st.
        assert_eq!(starts(&occs), ["2026-08-31T09:00:00Z"]);
    }

    #[test]
    fn expand_yearly_from_dtstart() {
        let e = event(
            "2025-08-13T09:00:00",
            "2025-08-13T10:00:00",
            Some("FREQ=YEARLY"),
        );
        let occs = expand(&e, &aug()).unwrap();
        assert_eq!(starts(&occs), ["2026-08-13T09:00:00Z"]);
    }

    #[test]
    fn expand_non_recurring_event() {
        let e = event("2026-08-13T09:00:00", "2026-08-13T10:00:00", None);
        let occs = expand(&e, &aug()).unwrap();
        assert_eq!(starts(&occs), ["2026-08-13T09:00:00Z"]);

        let outside = TimeRange::new(dt("2026-09-01T00:00:00"), dt("2026-10-01T00:00:00")).unwrap();
        assert!(expand(&e, &outside).unwrap().is_empty());
    }

    #[test]
    fn expand_drops_exdates() {
        let mut e = event(
            "2026-08-10T09:00:00",
            "2026-08-10T10:00:00",
            Some("FREQ=WEEKLY;BYDAY=MO,WE,FR"),
        );
        e.exdates = vec![
            chrono::NaiveDate::from_ymd_opt(2026, 8, 12).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2026, 8, 24).unwrap(),
        ];
        let occs = expand(&e, &aug()).unwrap();
        let expected = [
            "2026-08-10T09:00:00Z",
            "2026-08-14T09:00:00Z",
            "2026-08-17T09:00:00Z",
            "2026-08-19T09:00:00Z",
            "2026-08-21T09:00:00Z",
            "2026-08-26T09:00:00Z",
            "2026-08-28T09:00:00Z",
            "2026-08-31T09:00:00Z",
        ];
        assert_eq!(starts(&occs), expected);
    }

    #[test]
    fn expansion_respects_timezone_across_dst() {
        // 09:00 America/New_York, weekly on Monday. Nov 1 2026 is the US DST
        // fall-back; 09:00 EDT (13:00Z) becomes 09:00 EST (14:00Z).
        let mut e = event(
            "2026-10-26T13:00:00",
            "2026-10-26T13:30:00",
            Some("FREQ=WEEKLY;BYDAY=MO"),
        );
        e.tz = Some("America/New_York".into());
        let range = TimeRange::new(dt("2026-10-25T00:00:00"), dt("2026-11-09T00:00:00")).unwrap();
        let occs = expand(&e, &range).unwrap();
        assert_eq!(
            starts(&occs),
            ["2026-10-26T13:00:00Z", "2026-11-02T14:00:00Z"]
        );
    }

    #[test]
    fn edit_occurrence_across_dst_boundary() {
        // Series at 10:00 Europe/London across March 2026 spring forward.
        let mut series = event(
            "2026-03-23T10:00:00",
            "2026-03-23T11:00:00",
            Some("FREQ=WEEKLY;BYDAY=MO"),
        );
        series.tz = Some("Europe/London".into());

        // Edit future occurrences starting from March 30 (post-DST transition)
        let changes = OccurrenceChanges {
            starts_at: dt("2026-03-30T10:30:00"), // 11:30 BST = 10:30Z
            ends_at: dt("2026-03-30T11:30:00"),
            all_day: false,
            title: Some("New time after DST".into()),
            location: None,
            notes: None,
        };

        let result = edit_occurrence(
            &series,
            EditScope::Future,
            NaiveDate::from_ymd_opt(2026, 3, 30).unwrap(),
            &changes,
        )
        .unwrap();

        assert_eq!(result.len(), 2);
        // Original series capped at March 30
        assert!(result[0].rrule.as_ref().unwrap().contains("UNTIL="));
        // New series starts on March 30 with tz preserved
        assert_eq!(result[1].tz.as_deref(), Some("Europe/London"));
        assert_eq!(result[1].title, "New time after DST");
    }

    #[test]
    fn all_day_event_never_shifts_across_dst() {
        // Daily all-day event from March 7 to March 10
        let mut e = event(
            "2026-03-07T00:00:00",
            "2026-03-08T00:00:00",
            Some("FREQ=DAILY;COUNT=4"),
        );
        e.all_day = true;
        let range = TimeRange::new(dt("2026-03-06T00:00:00"), dt("2026-03-12T00:00:00")).unwrap();
        let occs = expand(&e, &range).unwrap();
        assert_eq!(
            starts(&occs),
            [
                "2026-03-07T00:00:00Z",
                "2026-03-08T00:00:00Z",
                "2026-03-09T00:00:00Z",
                "2026-03-10T00:00:00Z",
            ]
        );
    }

    #[test]
    fn override_replaces_series_instance() {
        // Series excludes Aug 12; a standalone override event sits at the same
        // slot. Combined expansion must contain exactly one occurrence there.
        let mut series = event(
            "2026-08-10T09:00:00",
            "2026-08-10T10:00:00",
            Some("FREQ=WEEKLY;BYDAY=MO,WE,FR"),
        );
        series.exdates = vec![chrono::NaiveDate::from_ymd_opt(2026, 8, 12).unwrap()];
        let override_event = event("2026-08-12T14:00:00", "2026-08-12T15:00:00", None);

        let occs = expand_set(&[series, override_event], &aug()).unwrap();
        let on_12th = occs
            .iter()
            .filter(|o| {
                o.starts_at.date_naive() == chrono::NaiveDate::from_ymd_opt(2026, 8, 12).unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(on_12th.len(), 1, "override replaces, not duplicates");
        assert_eq!(on_12th[0].starts_at, dt("2026-08-12T14:00:00"));
    }

    #[test]
    fn edit_this_scope_exdates_and_creates_override() {
        let series = event(
            "2026-08-10T09:00:00",
            "2026-08-10T10:00:00",
            Some("FREQ=WEEKLY;BYDAY=MO,WE,FR"),
        );
        let changes = OccurrenceChanges {
            starts_at: dt("2026-08-12T14:00:00"),
            ends_at: dt("2026-08-12T15:00:00"),
            all_day: false,
            title: None,
            location: None,
            notes: None,
        };
        let result = edit_occurrence(
            &series,
            EditScope::This,
            NaiveDate::from_ymd_opt(2026, 8, 12).unwrap(),
            &changes,
        )
        .unwrap();
        assert_eq!(result.len(), 2);

        let updated = &result[0];
        assert!(
            updated
                .exdates
                .contains(&NaiveDate::from_ymd_opt(2026, 8, 12).unwrap()),
            "edited instance excluded from series"
        );
        assert_eq!(updated.rrule.as_deref(), Some("FREQ=WEEKLY;BYDAY=MO,WE,FR"));

        let override_event = &result[1];
        assert!(override_event.rrule.is_none(), "override is standalone");
        assert_eq!(override_event.starts_at, dt("2026-08-12T14:00:00"));

        // The series now expands without Aug 12; the override covers it.
        let occs = expand_set(&[updated.clone(), override_event.clone()], &aug()).unwrap();
        let on_12th = occs
            .iter()
            .filter(|o| o.starts_at.date_naive() == NaiveDate::from_ymd_opt(2026, 8, 12).unwrap())
            .count();
        assert_eq!(on_12th, 1);
    }

    #[test]
    fn edit_future_scope_truncates_series_and_starts_new_head() {
        let series = event(
            "2026-08-10T09:00:00",
            "2026-08-10T10:00:00",
            Some("FREQ=WEEKLY;BYDAY=MO,WE,FR"),
        );
        let changes = OccurrenceChanges {
            starts_at: dt("2026-08-12T14:00:00"),
            ends_at: dt("2026-08-12T15:00:00"),
            all_day: false,
            title: None,
            location: None,
            notes: None,
        };
        let result = edit_occurrence(
            &series,
            EditScope::Future,
            NaiveDate::from_ymd_opt(2026, 8, 12).unwrap(),
            &changes,
        )
        .unwrap();
        assert_eq!(result.len(), 2);

        let truncated = &result[0];
        assert_eq!(
            truncated.rrule.as_deref(),
            Some("FREQ=WEEKLY;BYDAY=MO,WE,FR;UNTIL=20260812T085959Z"),
            "series ends 1s before the edited instance's 09:00 start"
        );

        let head = &result[1];
        assert_eq!(head.starts_at, dt("2026-08-12T14:00:00"));
        assert_eq!(head.rrule.as_deref(), Some("FREQ=WEEKLY;BYDAY=MO,WE,FR"));

        // The truncated series keeps only the Aug 10 instance; the head picks
        // up at the edited 14:00 slot and continues Mon/Wed/Fri through August.
        let occs = expand_set(&[truncated.clone(), head.clone()], &aug()).unwrap();
        assert_eq!(
            starts(&occs),
            [
                "2026-08-10T09:00:00Z",
                "2026-08-12T14:00:00Z",
                "2026-08-14T14:00:00Z",
                "2026-08-17T14:00:00Z",
                "2026-08-19T14:00:00Z",
                "2026-08-21T14:00:00Z",
                "2026-08-24T14:00:00Z",
                "2026-08-26T14:00:00Z",
                "2026-08-28T14:00:00Z",
                "2026-08-31T14:00:00Z",
            ]
        );
    }

    #[test]
    fn edit_all_scope_modifies_series_in_place() {
        let series = event(
            "2026-08-10T09:00:00",
            "2026-08-10T10:00:00",
            Some("FREQ=WEEKLY;BYDAY=MO,WE,FR"),
        );
        let changes = OccurrenceChanges {
            starts_at: dt("2026-08-10T10:00:00"),
            ends_at: dt("2026-08-10T11:00:00"),
            all_day: false,
            title: Some("Moved".into()),
            location: None,
            notes: None,
        };
        let result = edit_occurrence(
            &series,
            EditScope::All,
            NaiveDate::from_ymd_opt(2026, 8, 10).unwrap(),
            &changes,
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        let edited = &result[0];
        assert_eq!(edited.title, "Moved");
        assert_eq!(edited.starts_at, dt("2026-08-10T10:00:00"));
        assert_eq!(edited.rrule.as_deref(), Some("FREQ=WEEKLY;BYDAY=MO,WE,FR"));
    }

    #[test]
    fn delete_this_excludes_only_that_occurrence() {
        let series = event(
            "2026-08-10T09:00:00",
            "2026-08-10T10:00:00",
            Some("FREQ=WEEKLY;BYDAY=MO,WE,FR"),
        );
        let result = delete_occurrence(
            &series,
            EditScope::This,
            NaiveDate::from_ymd_opt(2026, 8, 14).unwrap(),
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0]
            .exdates
            .contains(&NaiveDate::from_ymd_opt(2026, 8, 14).unwrap()));
    }

    #[test]
    fn delete_future_truncates_series() {
        let series = event(
            "2026-08-10T09:00:00",
            "2026-08-10T10:00:00",
            Some("FREQ=WEEKLY;BYDAY=MO,WE,FR"),
        );
        let result = delete_occurrence(
            &series,
            EditScope::Future,
            NaiveDate::from_ymd_opt(2026, 8, 12).unwrap(),
        )
        .unwrap();
        assert_eq!(
            result[0].rrule.as_deref(),
            Some("FREQ=WEEKLY;BYDAY=MO,WE,FR;UNTIL=20260812T085959Z")
        );
    }

    #[test]
    fn delete_all_returns_empty() {
        let series = event(
            "2026-08-10T09:00:00",
            "2026-08-10T10:00:00",
            Some("FREQ=WEEKLY;BYDAY=MO,WE,FR"),
        );
        assert!(delete_occurrence(
            &series,
            EditScope::All,
            NaiveDate::from_ymd_opt(2026, 8, 10).unwrap()
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn edit_non_recurring_ignores_scope() {
        let series = event("2026-08-13T09:00:00", "2026-08-13T10:00:00", None);
        let changes = OccurrenceChanges {
            starts_at: dt("2026-08-13T11:00:00"),
            ends_at: dt("2026-08-13T12:00:00"),
            all_day: false,
            title: None,
            location: None,
            notes: None,
        };
        for scope in [EditScope::This, EditScope::Future, EditScope::All] {
            let result = edit_occurrence(
                &series,
                scope,
                NaiveDate::from_ymd_opt(2026, 8, 13).unwrap(),
                &changes,
            )
            .unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].starts_at, dt("2026-08-13T11:00:00"));
        }
    }
}
