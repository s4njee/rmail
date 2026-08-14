//! Date/time utilities: calendar-safe arithmetic and month-grid helpers.
//!
//! All functions are pure and operate on [`NaiveDate`] (dates without time),
//! which is all views need for day/month layout. Instant-level math lives in
//! the recurrence and timezone layers.
//!
//! Month arithmetic clamps to the last valid day of the target month — the
//! standard calendar behavior: `2026-01-31 + 1 month == 2026-02-28`, and
//! `2024-01-31 + 1 month == 2024-02-29` (leap year).

use chrono::{Datelike, Duration, NaiveDate, Weekday};

/// Adds `n` days to `date`.
pub fn add_days(date: NaiveDate, n: i64) -> NaiveDate {
    date + Duration::days(n)
}

/// Adds `n` weeks to `date`.
pub fn add_weeks(date: NaiveDate, n: i64) -> NaiveDate {
    date + Duration::weeks(n)
}

/// Adds `n` months to `date`, clamping the day to the target month's length.
///
/// `Jan 31 + 1 month -> Feb 28` (or 29 in a leap year); `Mar 31 - 1 month ->
/// Feb 28`. The day is clamped, never rolled into the following month.
pub fn add_months(date: NaiveDate, n: i32) -> NaiveDate {
    let total_months = date.year() * 12 + date.month() as i32 - 1 + n;
    let year = total_months.div_euclid(12);
    let month = total_months.rem_euclid(12) + 1;
    let day = date.day().min(days_in_month(year, month as u32));
    NaiveDate::from_ymd_opt(year, month as u32, day)
        .expect("year/month/day always form a valid date after clamping")
}

/// Whether `year` is a leap year.
fn is_leap_year(year: i32) -> bool {
    NaiveDate::from_ymd_opt(year, 2, 29).is_some()
}

/// Adds `n` years to `date`, clamping `Feb 29` to `Feb 28` in non-leap years
/// (and preserving it when the target year is itself a leap year).
pub fn add_years(date: NaiveDate, n: i32) -> NaiveDate {
    let year = date.year() + n;
    let day = if date.month() == 2 && date.day() == 29 && !is_leap_year(year) {
        28
    } else {
        date.day()
    };
    NaiveDate::from_ymd_opt(year, date.month(), day)
        .expect("year/month/day always form a valid date after clamping")
}

/// Returns the number of days in `month` of `year` (handles leap years).
pub fn days_in_month(year: i32, month: u32) -> u32 {
    let first = NaiveDate::from_ymd_opt(year, month, 1)
        .expect("year/month must form a valid date (month 1..=12)");
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap()
    };
    (next - first).num_days() as u32
}

/// The date of the start of `date`'s week, where the week begins on
/// `week_start` (e.g. `Weekday::Sun` or `Weekday::Mon`).
pub fn start_of_week(date: NaiveDate, week_start: Weekday) -> NaiveDate {
    let from_week_start =
        (date.weekday().num_days_from_monday() + 7 - week_start.num_days_from_monday()) % 7;
    date - Duration::days(i64::from(from_week_start))
}

/// Generates the 42-cell (6-week) month grid for `month` of `year`, starting on
/// `week_start`. Cells before the 1st are trailing days of the previous month;
/// cells after the last day are leading days of the next month. This is the
/// fixed 6×7 layout every Almanac month view uses (5 weeks only when the last
/// grid row is empty — the layout still reserves it).
pub fn month_grid(year: i32, month: u32, week_start: Weekday) -> Vec<NaiveDate> {
    let first = NaiveDate::from_ymd_opt(year, month, 1)
        .expect("year/month must form a valid date (month 1..=12)");
    let start = start_of_week(first, week_start);
    (0..42).map(|i| start + Duration::days(i)).collect()
}

/// Number of the week containing `date`, counting weeks that begin on
/// `week_start`. The first week of the year is the one containing Jan 1.
pub fn week_number(date: NaiveDate, week_start: Weekday) -> u32 {
    let jan_1 = NaiveDate::from_ymd_opt(date.year(), 1, 1).unwrap();
    let days = (date - start_of_week(jan_1, week_start)).num_days();
    u32::try_from(days / 7 + 1).expect("week number fits in u32")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_days_crosses_month_and_year_boundaries() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
        assert_eq!(
            add_days(date, 1),
            NaiveDate::from_ymd_opt(2026, 2, 1).unwrap()
        );
        assert_eq!(
            add_days(date, -31),
            NaiveDate::from_ymd_opt(2025, 12, 31).unwrap()
        );
    }

    #[test]
    fn add_weeks_stays_on_the_same_weekday() {
        let date = NaiveDate::from_ymd_opt(2026, 8, 13).unwrap(); // Thursday
        let next = add_weeks(date, 1);
        assert_eq!(next, NaiveDate::from_ymd_opt(2026, 8, 20).unwrap());
        assert_eq!(next.weekday(), Weekday::Thu);
    }

    #[test]
    fn add_months_clamps_short_months() {
        let jan_31 = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
        assert_eq!(
            add_months(jan_31, 1),
            NaiveDate::from_ymd_opt(2026, 2, 28).unwrap(),
            "Jan 31 + 1 month clamps to Feb 28"
        );
        assert_eq!(
            add_months(jan_31, 2),
            NaiveDate::from_ymd_opt(2026, 3, 31).unwrap(),
            "Jan 31 + 2 months lands back on the 31st"
        );
        assert_eq!(
            add_months(jan_31, -1),
            NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(),
            "subtracting never clamps to the previous month's length"
        );
    }

    #[test]
    fn add_months_handles_leap_february() {
        let jan_31 = NaiveDate::from_ymd_opt(2024, 1, 31).unwrap();
        assert_eq!(
            add_months(jan_31, 1),
            NaiveDate::from_ymd_opt(2024, 2, 29).unwrap(),
            "2024 is a leap year"
        );
        let feb_29 = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        assert_eq!(
            add_months(feb_29, 12),
            NaiveDate::from_ymd_opt(2025, 2, 28).unwrap(),
            "Feb 29 + 12 months clamps to Feb 28 in a non-leap year"
        );
    }

    #[test]
    fn add_years_clamps_leap_day() {
        let leap_day = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        assert_eq!(
            add_years(leap_day, 1),
            NaiveDate::from_ymd_opt(2025, 2, 28).unwrap()
        );
        assert_eq!(
            add_years(leap_day, 4),
            NaiveDate::from_ymd_opt(2028, 2, 29).unwrap()
        );
    }

    #[test]
    fn days_in_month_is_leap_aware() {
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 1), 31);
        assert_eq!(days_in_month(2026, 12), 31);
        assert_eq!(days_in_month(2026, 4), 30);
    }

    #[test]
    fn start_of_week_respects_configurable_week_start() {
        let thu = NaiveDate::from_ymd_opt(2026, 8, 13).unwrap(); // Thursday
        assert_eq!(
            start_of_week(thu, Weekday::Sun),
            NaiveDate::from_ymd_opt(2026, 8, 9).unwrap()
        );
        assert_eq!(
            start_of_week(thu, Weekday::Mon),
            NaiveDate::from_ymd_opt(2026, 8, 10).unwrap()
        );
        // Mid-week Sunday-start week wraps to the previous Sunday.
        assert_eq!(
            start_of_week(NaiveDate::from_ymd_opt(2026, 8, 10).unwrap(), Weekday::Sun),
            NaiveDate::from_ymd_opt(2026, 8, 9).unwrap()
        );
    }

    #[test]
    fn month_grid_is_42_cells_aligned_to_week_start() {
        let august = month_grid(2026, 8, Weekday::Sun);
        assert_eq!(august.len(), 42);
        // Aug 1 2026 is a Saturday; the grid opens on the previous Sunday.
        assert_eq!(august[0], NaiveDate::from_ymd_opt(2026, 7, 26).unwrap());
        assert_eq!(august[6], NaiveDate::from_ymd_opt(2026, 8, 1).unwrap());
        assert_eq!(august[41], NaiveDate::from_ymd_opt(2026, 9, 5).unwrap());
        // Each row starts on the configured week start.
        for row in august.chunks_exact(7) {
            assert_eq!(row[0].weekday(), Weekday::Sun);
        }

        let feb_2024 = month_grid(2024, 2, Weekday::Mon);
        assert_eq!(feb_2024.len(), 42);
        // Feb 1 2024 is a Thursday; Monday-start grid opens Jan 29.
        assert_eq!(feb_2024[0], NaiveDate::from_ymd_opt(2024, 1, 29).unwrap());
        assert_eq!(feb_2024[3], NaiveDate::from_ymd_opt(2024, 2, 1).unwrap());
        assert_eq!(feb_2024[41], NaiveDate::from_ymd_opt(2024, 3, 10).unwrap());
    }

    #[test]
    fn week_number_counts_from_january() {
        let week_start = Weekday::Mon;
        assert_eq!(
            week_number(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), week_start),
            1
        );
        assert_eq!(
            week_number(NaiveDate::from_ymd_opt(2026, 8, 13).unwrap(), week_start),
            33
        );
    }
}
