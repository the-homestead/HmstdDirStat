//! Value parsers for the query language: sizes, ages and dates.
//!
//! Sizes are binary (`kb` = 1024). Ages are `d`, `w` (7 days), `m` (30 days) or
//! `y` (365 days). Dates are `YYYY-MM-DD` in UTC.

use crate::ast::Cmp;

const DAY: i64 = 86_400;

/// What a `modified:` term compares: an age, or an absolute time.
pub enum Modified {
    Age(Cmp<i64>),
    At(Cmp<i64>),
}

#[derive(Clone, Copy)]
enum Op {
    Eq,
    Gt,
    Ge,
    Lt,
    Le,
}

fn make_cmp<T>(op: Op, value: T) -> Cmp<T> {
    match op {
        Op::Eq => Cmp::Eq(value),
        Op::Gt => Cmp::Gt(value),
        Op::Ge => Cmp::Ge(value),
        Op::Lt => Cmp::Lt(value),
        Op::Le => Cmp::Le(value),
    }
}

/// Splits an optional leading operator from the rest of the value.
fn split_op(input: &str) -> (Option<Op>, &str) {
    for (prefix, op) in [
        (">=", Op::Ge),
        ("<=", Op::Le),
        (">", Op::Gt),
        ("<", Op::Lt),
        ("=", Op::Eq),
    ] {
        if let Some(rest) = input.strip_prefix(prefix) {
            return (Some(op), rest);
        }
    }
    (None, input)
}

/// `size:` value: `empty`, a comparison such as `>1gb`, or a range `1gb..2gb`.
pub fn size(input: &str) -> Result<Cmp<u64>, String> {
    if input.eq_ignore_ascii_case("empty") {
        return Ok(Cmp::Eq(0));
    }
    if let Some((low, high)) = input.split_once("..") {
        let low = bytes(low)?;
        let high = bytes(high)?;
        if low > high {
            return Err(format!("range '{input}' starts above its end"));
        }
        return Ok(Cmp::Between(low, high));
    }
    let (op, rest) = split_op(input);
    Ok(make_cmp(op.unwrap_or(Op::Eq), bytes(rest)?))
}

/// A number with an optional unit: `1024`, `1.5gb`, `10 MB`.
fn bytes(input: &str) -> Result<u64, String> {
    let text = input.trim();
    let split = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(text.len());
    let number: f64 = text[..split]
        .parse()
        .map_err(|_| format!("expected a size such as 10mb, found '{input}'"))?;
    let multiplier: u64 = match text[split..].trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" => 1 << 10,
        "m" | "mb" => 1 << 20,
        "g" | "gb" => 1 << 30,
        "t" | "tb" => 1 << 40,
        unit => return Err(format!("unknown size unit '{unit}'")),
    };
    Ok((number * multiplier as f64).round() as u64)
}

/// `modified:` value: `today`, an age such as `<7d`, or a date such as `>2024-01-01`
/// or `2024-01-01..2024-06-30`.
pub fn modified(input: &str) -> Result<Modified, String> {
    if input.eq_ignore_ascii_case("today") {
        return Ok(Modified::Age(Cmp::Lt(DAY)));
    }
    if let Some((low, high)) = input.split_once("..") {
        let start = day_start(low)?;
        let end = day_start(high)? + DAY - 1;
        if start > end {
            return Err(format!("range '{input}' starts after its end"));
        }
        return Ok(Modified::At(Cmp::Between(start, end)));
    }
    let (op, rest) = split_op(input);
    if let Some(secs) = duration(rest) {
        // A bare age means "within the last N": younger than N.
        let op = op.unwrap_or(Op::Lt);
        return Ok(Modified::Age(make_cmp(op, secs)));
    }
    let start = day_start(rest)?;
    let end = start + DAY - 1;
    Ok(Modified::At(match op {
        None | Some(Op::Eq) => Cmp::Between(start, end),
        Some(Op::Gt) => Cmp::Gt(end),
        Some(Op::Ge) => Cmp::Ge(start),
        Some(Op::Lt) => Cmp::Lt(start),
        Some(Op::Le) => Cmp::Le(end),
    }))
}

/// `7d`, `2w`, `3m`, `1y` as seconds. `None` when the text is not a duration.
fn duration(input: &str) -> Option<i64> {
    let unit = input.chars().last()?;
    let count: i64 = input[..input.len() - unit.len_utf8()].parse().ok()?;
    let per_unit = match unit.to_ascii_lowercase() {
        'd' => DAY,
        'w' => 7 * DAY,
        'm' => 30 * DAY,
        'y' => 365 * DAY,
        _ => return None,
    };
    count.checked_mul(per_unit)
}

/// Midnight UTC at the start of a `YYYY-MM-DD` date, as Unix seconds.
fn day_start(input: &str) -> Result<i64, String> {
    let parts: Vec<&str> = input.trim().split('-').collect();
    let [year, month, day] = parts.as_slice() else {
        return Err(format!("expected a date as YYYY-MM-DD, found '{input}'"));
    };
    let year: i64 = year.parse().map_err(|_| format!("bad year in '{input}'"))?;
    let month: i64 = month
        .parse()
        .map_err(|_| format!("bad month in '{input}'"))?;
    let day: i64 = day.parse().map_err(|_| format!("bad day in '{input}'"))?;
    if !(1..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
    {
        return Err(format!("'{input}' is not a real calendar date"));
    }
    Ok(days_from_civil(year, month, day) * DAY)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates_match_known_days() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(2024, 2, 29), 19_782);
    }

    #[test]
    fn invalid_calendar_dates_are_rejected() {
        assert!(day_start("2025-02-29").is_err());
        assert!(day_start("2024-02-29").is_ok());
        assert!(day_start("2024-13-01").is_err());
        assert!(day_start("2024-1").is_err());
    }

    #[test]
    fn sizes_use_binary_units_and_ranges() {
        assert_eq!(bytes("1kb").unwrap(), 1024);
        assert_eq!(bytes("1.5gb").unwrap(), 1_610_612_736);
        assert!(bytes("3zb").is_err());
        assert!(size("2gb..1gb").is_err());
    }
}
