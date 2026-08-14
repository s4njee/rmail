//! Golden-file tests for recurrence expansion (S1.4).
//!
//! Each `tests/fixtures/<name>.ics` is a known VEVENT whose expansion over a
//! query window must match `<name>.expected.json` exactly (a list of UTC
//! occurrence start times in RFC 3339). Fixtures cover FREQ/INTERVAL/BYDAY/
//! BYMONTHDAY/UNTIL/COUNT, EXDATE exclusion, and DST-correct timezone
//! expansion — the cases that must never regress silently.

use std::fs;
use std::path::Path;

use calendar_core::ical;
use calendar_core::recurrence::expand;
use calendar_core::TimeRange;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ExpectedFixture {
    range: [String; 2],
    expected: Vec<String>,
}

#[test]
fn expansions_match_golden_fixtures() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");

    let mut ics_files = fs::read_dir(&fixtures)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "ics"))
        .collect::<Vec<_>>();
    ics_files.sort();
    assert!(
        !ics_files.is_empty(),
        "expected at least one .ics golden fixture"
    );

    for path in &ics_files {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let ics = fs::read_to_string(path).unwrap();
        let expected_path = fixtures.join(format!("{name}.expected.json"));
        let expected: ExpectedFixture =
            serde_json::from_str(&fs::read_to_string(expected_path).unwrap())
                .unwrap_or_else(|e| panic!("bad expected fixture for {name}: {e}"));

        let events = ical::parse_ical(&ics).unwrap_or_else(|e| panic!("parse {name}: {e}"));
        assert_eq!(
            events.len(),
            1,
            "{name} fixture must contain exactly one event"
        );

        let start: chrono::DateTime<chrono::Utc> = expected.range[0].parse().unwrap();
        let end: chrono::DateTime<chrono::Utc> = expected.range[1].parse().unwrap();
        let range = TimeRange::new(start, end).unwrap();

        let occurrences = expand(&events[0], &range).unwrap();
        let actual = occurrences
            .iter()
            .map(|o| o.starts_at.format("%Y-%m-%dT%H:%M:%SZ").to_string())
            .collect::<Vec<_>>();

        assert_eq!(actual, expected.expected, "golden mismatch for {name}");
    }
}
