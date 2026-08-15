//! VTODO parsing and serialization for tasks / to-dos (Roadmap 4.5).

use quill_store::CalendarTask;

/// Parse VTODO iCalendar text into a list of `CalendarTask`.
pub fn parse_vtodo_tasks(ics_text: &str, default_account_id: u32) -> Vec<CalendarTask> {
    let mut tasks = Vec::new();
    let mut in_vtodo = false;
    let mut title = String::new();
    let mut due_at_ms: Option<i64> = None;
    let mut completed_at_ms: Option<i64> = None;
    let mut priority: Option<u32> = None;

    for line in ics_text.lines() {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("BEGIN:VTODO") {
            in_vtodo = true;
            title.clear();
            due_at_ms = None;
            completed_at_ms = None;
            priority = None;
        } else if trimmed.eq_ignore_ascii_case("END:VTODO") {
            if in_vtodo && !title.is_empty() {
                tasks.push(CalendarTask {
                    id: 0,
                    account_id: default_account_id,
                    title: title.clone(),
                    due_at_ms,
                    completed_at_ms,
                    priority,
                });
            }
            in_vtodo = false;
        } else if in_vtodo {
            if let Some(val) = trimmed.strip_prefix("SUMMARY:") {
                title = val.to_string();
            } else if trimmed.starts_with("DUE") {
                if let Some(pos) = trimmed.find(':') {
                    due_at_ms = parse_iso_or_utc(&trimmed[pos + 1..]);
                }
            } else if trimmed.starts_with("COMPLETED:") {
                completed_at_ms = parse_iso_or_utc(&trimmed[10..]);
            } else if let Some(val) = trimmed.strip_prefix("PRIORITY:") {
                priority = val.parse().ok();
            } else if let Some(val) = trimmed.strip_prefix("STATUS:") {
                if val.eq_ignore_ascii_case("COMPLETED") && completed_at_ms.is_none() {
                    completed_at_ms = Some(chrono::Utc::now().timestamp_millis());
                }
            }
        }
    }

    tasks
}

/// Serialize a `CalendarTask` into a RFC 5545 VTODO component string.
pub fn serialize_vtodo(task: &CalendarTask) -> String {
    let mut lines = Vec::new();
    lines.push("BEGIN:VCALENDAR".to_string());
    lines.push("VERSION:2.0".to_string());
    lines.push("PRODID:-//Quill//Almanac Calendar//EN".to_string());
    lines.push("BEGIN:VTODO".to_string());
    lines.push(format!("UID:quill-task-{}@quill.app", task.id));
    lines.push(format!("SUMMARY:{}", task.title));

    if let Some(due) = task.due_at_ms {
        if let Some(dt) = chrono::DateTime::from_timestamp_millis(due) {
            lines.push(format!("DUE:{}", dt.format("%Y%m%dT%H%M%SZ")));
        }
    }

    if let Some(completed) = task.completed_at_ms {
        if let Some(dt) = chrono::DateTime::from_timestamp_millis(completed) {
            lines.push(format!("COMPLETED:{}", dt.format("%Y%m%dT%H%M%SZ")));
            lines.push("STATUS:COMPLETED".to_string());
        }
    } else {
        lines.push("STATUS:NEEDS-ACTION".to_string());
    }

    if let Some(p) = task.priority {
        lines.push(format!("PRIORITY:{}", p));
    }

    lines.push("END:VTODO".to_string());
    lines.push("END:VCALENDAR".to_string());
    lines.join("\r\n")
}

fn parse_iso_or_utc(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.timestamp_millis())
        .ok()
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%SZ")
                .map(|ndt| ndt.and_utc().timestamp_millis())
                .ok()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vtodo_roundtrip() {
        let task = CalendarTask {
            id: 42,
            account_id: 1,
            title: "Prepare presentation".into(),
            due_at_ms: Some(1773000000000),
            completed_at_ms: None,
            priority: Some(1),
        };

        let ics = serialize_vtodo(&task);
        assert!(ics.contains("BEGIN:VTODO"));
        assert!(ics.contains("SUMMARY:Prepare presentation"));

        let parsed = parse_vtodo_tasks(&ics, 1);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].title, "Prepare presentation");
        assert_eq!(parsed[0].due_at_ms, Some(1773000000000));
        assert_eq!(parsed[0].priority, Some(1));
    }
}
