//! Calendar providers & subscriptions synchronizer (Roadmap 4.4).
//!
//! Supports Google Calendar, Microsoft 365, and read-only .ics / webcal subscriptions.

use chrono::Utc;
use quill_store::sqlite::SqliteStore;
use quill_store::types::{CalendarEvent, CalendarSubscription};
use reqwest::Client;

/// Synchronize a remote read-only `.ics` / `webcal://` calendar subscription into local store.
pub async fn sync_ics_subscription(
    store: &SqliteStore,
    sub: &CalendarSubscription,
) -> Result<usize, String> {
    let mut target_url = sub.url.trim().to_string();
    if let Some(rest) = target_url.strip_prefix("webcal://") {
        target_url = format!("https://{}", rest);
    } else if let Some(rest) = target_url.strip_prefix("http://") {
        target_url = format!("http://{}", rest);
    }

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(&target_url)
        .header("User-Agent", "Quill/1.0 (Calendar Subscription)")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch subscription {}: {}", target_url, e))?;

    if !response.status().is_success() {
        return Err(format!(
            "HTTP error {} fetching {}",
            response.status(),
            target_url
        ));
    }

    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    let events = calendar_core::ical::parse_ical(&body).map_err(|e| e.to_string())?;
    let mut count = 0;

    let now_ms = Utc::now().timestamp_millis();
    // Dedup against the full store (not a rolling window), so an event that
    // drifts outside [now-30d, now+180d] isn't re-inserted on every sync.
    let mut existing_events = store.list_events(0, i64::MAX / 2);

    for ev in events {
        let start_ms = ev.starts_at.timestamp_millis();
        let end_ms = ev.ends_at.timestamp_millis();

        let exists = existing_events
            .iter()
            .any(|e| e.account_id == 1 && e.title == ev.title && e.start_ms == start_ms);

        if !exists {
            let created = store.create_event(CalendarEvent {
                id: 0,
                account_id: 1,
                title: ev.title.clone(),
                start_ms,
                end_ms,
                all_day: ev.all_day,
                location: ev.location.clone(),
                notes: ev.notes.clone(),
                alarm_minutes_before: None,
                timezone: ev.tz.clone(),
                travel_time_minutes: None,
                calendar_source: None,
                calendar_name: None,
                calendar_color: None,
                color: None,
            })?;
            existing_events.push(created);
            count += 1;
        }
    }

    store.update_subscription_refreshed(sub.id, now_ms)?;
    Ok(count)
}

/// Tailored palette for discovered Google calendars without a background color.
const DEFAULT_COLORS: &[&str] = &[
    "#1F6FEB", // Blue
    "#C2410C", // Rust/Orange
    "#0F766E", // Teal
    "#7C3AED", // Violet
    "#B91C1C", // Red
    "#D97706", // Amber
    "#059669", // Green
    "#DB2777", // Pink
];

/// Synchronize a Google account's calendars via the Google Calendar v3 API.
///
/// Discovers every calendar in the user's calendarList (not just `primary`,
/// which is frequently empty when events live in other calendars) and upserts
/// each one's expanded events into the local store — mirroring the standalone
/// Almanac app's Google sync. A broad ±1-year window keeps the visible range
/// (and a bit of navigation either way) populated; re-syncing is idempotent
/// (dedup by account · title · start). Events are tagged with their source
/// calendar (id, name, color) so the UI can show each calendar separately.
pub async fn sync_google_calendar_api(
    store: &SqliteStore,
    account_id: u32,
    access_token: &str,
) -> Result<usize, String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;

    // 1. Discover every calendar the user has access to.
    let cal_list_url =
        "https://www.googleapis.com/calendar/v3/users/me/calendarList?maxResults=250";
    let cal_list_res = client
        .get(cal_list_url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("Google calendarList request failed: {e}"))?;
    if !cal_list_res.status().is_success() {
        return Err(format!(
            "Google calendarList returned status {}",
            cal_list_res.status()
        ));
    }
    let cal_list: calendar_core::sync::google_model::GoogleCalendarList = cal_list_res
        .json()
        .await
        .map_err(|e| format!("Failed to parse Google calendarList JSON: {e}"))?;

    // (gcal_id, summary, color) for each remote calendar.
    let mut remote_calendars: Vec<(String, String, String)> = cal_list
        .items
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let color = c
                .background_color
                .unwrap_or_else(|| DEFAULT_COLORS[i % DEFAULT_COLORS.len()].to_string());
            (c.id, c.summary, color)
        })
        .collect();
    if remote_calendars.is_empty() {
        remote_calendars.push((
            "primary".to_string(),
            "Primary".to_string(),
            DEFAULT_COLORS[0].to_string(),
        ));
    }

    // 2. Fetch each calendar's expanded events (±1 year) and write to the store.
    let now = chrono::Utc::now();
    let time_min = (now - chrono::Duration::days(365))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let time_max = (now + chrono::Duration::days(365))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    // 3. Skip calendars the user has removed (their local events were deleted
    //    and the source is excluded so a re-sync doesn't bring them back).
    let removed: Vec<String> = store
        .removed_calendar_sources()
        .into_iter()
        .filter(|c| c.account_id == account_id)
        .map(|c| c.source)
        .collect();

    let mut count = 0;
    let mut existing = store.list_events(0, i64::MAX / 2);
    for (calendar_id, calendar_name, calendar_color) in remote_calendars {
        if removed.contains(&calendar_id) {
            continue;
        }
        let url = format!(
            "https://www.googleapis.com/calendar/v3/calendars/{}/events?singleEvents=true&maxResults=2500&timeMin={}&timeMax={}",
            urlencoding_encode(&calendar_id),
            time_min,
            time_max
        );
        let res = client
            .get(&url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| format!("Google events request for {calendar_id} failed: {e}"))?;
        if !res.status().is_success() {
            return Err(format!(
                "Google events returned status {} for {calendar_id}",
                res.status()
            ));
        }
        let json_text = res
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {e}"))?;

        let items =
            calendar_core::sync::google_model::parse_gcal_events_json(&json_text, uuid::Uuid::nil());
        for ev in items {
            if ev.deleted_at.is_some() {
                continue;
            }
            let start_ms = ev.starts_at.timestamp_millis();
            if let Some(existing_idx) = existing
                .iter()
                .position(|e| e.account_id == account_id && e.title == ev.title && e.start_ms == start_ms)
            {
                // Backfill the source tag on re-sync — previously-synced events
                // (migration 14 predates source tagging) carry no source and
                // would otherwise never appear under their calendar.
                if existing[existing_idx].calendar_source.is_none() {
                    let mut updated = existing[existing_idx].clone();
                    updated.calendar_source = Some(calendar_id.clone());
                    updated.calendar_name = Some(calendar_name.clone());
                    updated.calendar_color = Some(calendar_color.clone());
                    existing[existing_idx] = updated.clone();
                    store.update_event(updated)?;
                }
                continue;
            }
            let created = store.create_event(CalendarEvent {
                id: 0,
                account_id,
                title: ev.title,
                start_ms,
                end_ms: ev.ends_at.timestamp_millis(),
                all_day: ev.all_day,
                location: ev.location,
                notes: ev.notes,
                alarm_minutes_before: Some(15),
                calendar_source: Some(calendar_id.clone()),
                calendar_name: Some(calendar_name.clone()),
                calendar_color: Some(calendar_color.clone()),
                color: None,
                timezone: ev.tz,
                travel_time_minutes: None,
            })?;
            existing.push(created);
            count += 1;
        }
    }

    Ok(count)
}

/// Synchronize Microsoft 365 Calendar using OAuth bearer token and Microsoft Graph API.
pub async fn sync_ms365_calendar_api(
    store: &SqliteStore,
    account_id: u32,
    access_token: &str,
) -> Result<usize, String> {
    let client = Client::new();
    let url = "https://graph.microsoft.com/v1.0/me/events?$top=100&$select=subject,bodyPreview,start,end,location,isAllDay";

    let res = client
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("Microsoft Graph API request failed: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("Microsoft Graph API returned status {}", res.status()));
    }

    let json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;

    let items = json.get("value").and_then(|v| v.as_array());
    let mut count = 0;
    let mut existing = store.list_events(0, i64::MAX / 2);

    if let Some(events) = items {
        for ev in events {
            let title = ev.get("subject").and_then(|s| s.as_str()).unwrap_or("Untitled Event").to_string();
            let notes = ev.get("bodyPreview").and_then(|s| s.as_str()).map(str::to_string);
            let location = ev.get("location").and_then(|l| l.get("displayName")).and_then(|d| d.as_str()).map(str::to_string);
            let all_day = ev.get("isAllDay").and_then(|a| a.as_bool()).unwrap_or(false);

            let start_str = ev.get("start").and_then(|s| s.get("dateTime")).and_then(|d| d.as_str()).unwrap_or("");
            let end_str = ev.get("end").and_then(|s| s.get("dateTime")).and_then(|d| d.as_str()).unwrap_or("");
            let start_tz = ev.get("start").and_then(|s| s.get("timeZone")).and_then(|t| t.as_str()).unwrap_or("UTC");
            let end_tz = ev.get("end").and_then(|s| s.get("timeZone")).and_then(|t| t.as_str()).unwrap_or("UTC");

            // Graph returns offsetless dateTimes plus a separate timeZone. The
            // previous code fell back to `Utc::now()` on the parse failure,
            // writing every event at the sync instant — skip malformed events
            // instead of corrupting their times.
            let Some(start_ms) = parse_graph_datetime(start_str, start_tz) else {
                continue;
            };
            let end_ms = parse_graph_datetime(end_str, end_tz).unwrap_or(start_ms + 3_600_000);

            if existing
                .iter()
                .any(|e| e.account_id == account_id && e.title == title && e.start_ms == start_ms)
            {
                continue;
            }
            let created = store.create_event(CalendarEvent {
                id: 0,
                account_id,
                title,
                start_ms,
                end_ms,
                all_day,
                location,
                notes,
                alarm_minutes_before: Some(15),
                timezone: Some(start_tz.to_string()),
                travel_time_minutes: None,
                calendar_source: None,
                calendar_name: None,
                calendar_color: None,
                color: None,
            })?;
            existing.push(created);
            count += 1;
        }
    }

    Ok(count)
}

/// Parse a Microsoft Graph `dateTime` + `timeZone` pair into UTC millis. Graph
/// returns offsetless strings like "2026-08-14T14:00:00.0000000" with a
/// separate `timeZone` ("UTC" or an IANA name); fall back to RFC 3339 when the
/// string carries its own offset. Returns `None` for unparseable values.
fn parse_graph_datetime(dt_str: &str, tz: &str) -> Option<i64> {
    if dt_str.is_empty() {
        return None;
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(dt_str) {
        return Some(dt.timestamp_millis());
    }
    let naive = chrono::NaiveDateTime::parse_from_str(dt_str, "%Y-%m-%dT%H:%M:%S%.f").ok()?;
    if tz.eq_ignore_ascii_case("UTC") || tz.is_empty() {
        return Some(naive.and_utc().timestamp_millis());
    }
    let tz: chrono_tz::Tz = tz.parse().ok()?;
    naive.and_local_timezone(tz).earliest().map(|d| d.timestamp_millis())
}

fn parse_google_datetime(
    start: &Option<calendar_core::sync::google_model::GoogleEventDateTime>,
    end: &Option<calendar_core::sync::google_model::GoogleEventDateTime>,
) -> (i64, i64, bool) {
    let now = Utc::now().timestamp_millis();
    let mut start_ms = now;
    let mut end_ms = now + 3_600_000;
    let mut all_day = false;

    if let Some(s) = start {
        if let Some(dt) = s.date_time {
            start_ms = dt.timestamp_millis();
        } else if let Some(d) = s.date {
            all_day = true;
            start_ms = d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis();
        }
    }

    if let Some(e) = end {
        if let Some(dt) = e.date_time {
            end_ms = dt.timestamp_millis();
        } else if let Some(d) = e.date {
            end_ms = d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis();
        }
    }

    (start_ms, end_ms, all_day)
}

fn urlencoding_encode(s: &str) -> String {
    s.replace('%', "%25")
        .replace('#', "%23")
        .replace('@', "%40")
        .replace('/', "%2F")
        .replace(' ', "%20")
        .replace(':', "%3A")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_google_datetime() {
        let (s, e, all_day) = parse_google_datetime(&None, &None);
        assert!(!all_day);
        assert_eq!(e - s, 3_600_000);
    }
}
