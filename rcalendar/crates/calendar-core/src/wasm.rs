//! FFI / WASM / JSON recurrence expansion bindings (S7.4).
//!
//! Provides a string/JSON-based interface to the pure-Rust recurrence engine
//! for consumption by non-Rust runtimes (WASM, Node, Python, C ABI).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::{Event, Occurrence, TimeRange};
use crate::recurrence::expand;

/// Request payload for expanding recurrence via JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandRecurrenceRequest {
    pub event: Event,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

/// Response payload from expanding recurrence via JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandRecurrenceResponse {
    pub occurrences: Vec<Occurrence>,
    pub count: usize,
}

/// Expands recurrence rules from a JSON request string, returning a JSON response string.
///
/// Designed for easy integration with WASM `wasm_bindgen` or Node/C FFI bindings.
pub fn expand_recurrence_json(request_json: &str) -> Result<String, String> {
    let req: ExpandRecurrenceRequest =
        serde_json::from_str(request_json).map_err(|e| format!("Invalid JSON request: {e}"))?;

    let range = TimeRange::new(req.from, req.to).map_err(|e| e.to_string())?;
    let occurrences = expand(&req.event, &range).map_err(|e| e.to_string())?;
    let count = occurrences.len();

    let res = ExpandRecurrenceResponse { occurrences, count };
    serde_json::to_string(&res).map_err(|e| format!("Serialization error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use uuid::Uuid;

    #[test]
    fn json_recurrence_expansion_round_trips() {
        let now = Utc::now();
        let event = Event {
            id: Uuid::new_v4(),
            calendar_id: Uuid::new_v4(),
            uid: "json-test@example.com".into(),
            title: "Daily Standup".into(),
            location: None,
            notes: None,
            starts_at: now,
            ends_at: now + Duration::minutes(30),
            all_day: false,
            tz: None,
            rrule: Some("FREQ=DAILY;COUNT=5".into()),
            exdates: vec![],
            etag: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };

        let req = ExpandRecurrenceRequest {
            event,
            from: now - Duration::days(1),
            to: now + Duration::days(10),
        };

        let json_input = serde_json::to_string(&req).unwrap();
        let json_output = expand_recurrence_json(&json_input).unwrap();
        let res: ExpandRecurrenceResponse = serde_json::from_str(&json_output).unwrap();

        assert_eq!(res.count, 5);
        assert_eq!(res.occurrences.len(), 5);
    }
}
