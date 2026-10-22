//! Timestamps. The UI expects ISO-8601 UTC strings, and everything the app
//! writes to disk should be readable without this program, so ISO-8601 is used
//! for project state, diary front matter and the activity log alike.

use std::time::{SystemTime, UNIX_EPOCH};

/// e.g. `2026-10-18T09:31:04.512Z`.
pub fn now_iso() -> String {
    to_iso(SystemTime::now())
}

pub fn to_iso(time: SystemTime) -> String {
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = duration.as_secs() as i64;
    let millis = duration.subsec_millis();

    let days = seconds.div_euclid(86_400);
    let remainder = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);

    let hour = remainder / 3_600;
    let minute = (remainder % 3_600) / 60;
    let second = remainder % 60;

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

/// Today's date as `YYYY-MM-DD`, used for default diary entry names.
pub fn today_key() -> String {
    let today = now_iso();
    today[..10].to_string()
}

/// Days since the Unix epoch to a civil date. Howard Hinnant's algorithm, which
/// avoids pulling a date library in for one conversion.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 { shifted } else { shifted - 146_096 } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_part + 2) / 5 + 1) as u32;
    let month = if month_part < 10 { month_part + 3 } else { month_part - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}
