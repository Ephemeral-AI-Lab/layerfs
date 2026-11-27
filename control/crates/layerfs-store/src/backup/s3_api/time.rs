//! The two timestamp forms S3 replies carry, as milliseconds since the Unix
//! epoch: `Last-Modified` (`Wed, 21 Oct 2015 07:28:00 GMT`) and a listing's
//! `LastModified` (`2015-10-21T07:28:00.000Z`). Anything else is `None`.

/// Days from 1970-01-01 to the civil date (proleptic Gregorian).
fn days(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_cycle - 719_468
}

fn number(text: &str, digits: usize) -> Option<i64> {
    (text.len() == digits && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

fn millis(date: (i64, i64, i64), time: &str, fraction: u64) -> Option<u64> {
    let (year, month, day) = date;
    let mut parts = time.split(':');
    let hour = number(parts.next()?, 2)?;
    let minute = number(parts.next()?, 2)?;
    let second = number(parts.next()?, 2)?;
    let valid = parts.next().is_none()
        && (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && hour < 24
        && minute < 60
        && second < 61
        && year >= 1970;
    if !valid {
        return None;
    }
    let seconds = days(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second;
    u64::try_from(seconds)
        .ok()?
        .checked_mul(1000)?
        .checked_add(fraction)
}

/// `Wed, 21 Oct 2015 07:28:00 GMT`.
pub fn http_date(text: &str) -> Option<u64> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut words = text.split(' ');
    let _weekday = words.next()?.strip_suffix(',')?;
    let day = number(words.next()?, 2)?;
    let month = words.next()?;
    let month = MONTHS.iter().position(|m| *m == month)? as i64 + 1;
    let year = number(words.next()?, 4)?;
    let time = words.next()?;
    (words.next()? == "GMT" && words.next().is_none()).then_some(())?;
    millis((year, month, day), time, 0)
}

/// `2015-10-21T07:28:00Z` or with a fraction, `…:00.123Z`.
pub fn iso8601(text: &str) -> Option<u64> {
    let (date, time) = text.strip_suffix('Z')?.split_once('T')?;
    let mut parts = date.split('-');
    let year = number(parts.next()?, 4)?;
    let month = number(parts.next()?, 2)?;
    let day = number(parts.next()?, 2)?;
    parts.next().is_none().then_some(())?;
    let (time, fraction) = match time.split_once('.') {
        Some((time, digits))
            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) =>
        {
            let first: String = digits.chars().chain("000".chars()).take(3).collect();
            (time, first.parse().ok()?)
        }
        Some(_) => return None,
        None => (time, 0),
    };
    millis((year, month, day), time, fraction)
}
