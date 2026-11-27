//! Times as results show them (call surface 2.0): a `*_at` key is ISO-8601
//! in UTC, to the second.

/// Milliseconds since 1970 as `2026-11-18T09:16:59Z`; a time before 1970
/// answers 1970's first second.
pub fn iso(ms: i64) -> String {
    let seconds = ms.max(0) / 1000;
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    // days since 1970-01-01 as a civil date (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}
