//! Value parsers for attribute values and texts.
//!
//! A value parser is any function `FnOnce(&str) -> Result<T, E>` where `E: Display`.
//! The parsers in this module return `Result<T, String>`. All formats are strict:
//! surrounding whitespace is not allowed.
//!
//! ```
//! use use_xml::parse;
//!
//! let root = use_xml::parse_str(r#"<item price="12.50" count="3" kind="b" />"#)?;
//! let price = root.attr("price", parse::decimal_non_negative)?;
//! let count = root.attr("count", parse::u32)?;
//! let kind = root.attr("kind", parse::enumeration(&[("a", 1), ("b", 2)]))?;
//! assert_eq!((price.to_string(), count, kind), ("12.50".to_owned(), 3, 2));
//! root.check_all_used()?;
//! # Ok::<(), use_xml::Error>(())
//! ```

use rust_decimal::Decimal;
use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};

/// Returns the string unchanged.
///
/// # Errors
///
/// Never fails. The `Result` is returned only for consistency with the other parsers.
pub fn string(s: &str) -> Result<String, String> {
    Ok(s.to_owned())
}

/// Returns the string if it is not empty.
///
/// # Errors
///
/// Returns an error message if the string is empty.
pub fn string_non_empty(s: &str) -> Result<String, String> {
    if s.is_empty() {
        Err("expected non-empty string".to_owned())
    } else {
        Ok(s.to_owned())
    }
}

/// Returns the string if it contains at least one character
/// which is not XML whitespace (space, tab, carriage return, line feed).
///
/// Other Unicode whitespace (e.g. `U+00A0` no-break space) counts as a non-whitespace character.
///
/// # Errors
///
/// Returns an error message if the string is empty or consists only of XML whitespace.
pub fn string_non_whitespace(s: &str) -> Result<String, String> {
    if s.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r')) {
        Err("expected string with at least one non-whitespace character".to_owned())
    } else {
        Ok(s.to_owned())
    }
}

/// Returns a parser which accepts only one of the given strings.
pub fn one_of<'a>(cases: &'a [&'a str]) -> impl Fn(&str) -> Result<String, String> + 'a {
    move |s| {
        if cases.contains(&s) {
            Ok(s.to_owned())
        } else {
            Err(format!("expected one of {cases:?}"))
        }
    }
}

/// Returns a parser which maps the given strings to values.
pub fn enumeration<'a, T: Clone>(
    cases: &'a [(&'a str, T)],
) -> impl Fn(&str) -> Result<T, String> + 'a {
    move |s| match cases.iter().find(|(key, _)| *key == s) {
        Some((_, value)) => Ok(value.clone()),
        None => {
            let keys: Vec<&str> = cases.iter().map(|(key, _)| *key).collect();
            Err(format!("expected one of {keys:?}"))
        }
    }
}

macro_rules! integer_parsers {
    ($signs:literal: $($t:ident),*) => {
        $(
            #[doc = concat!("Parses `", stringify!($t), "` with `from_str`: digits with an optional leading ", $signs, ".")]
            ///
            /// # Errors
            ///
            /// Returns an error message if the string is not a number in this format
            /// or the number is out of the range of the type.
            pub fn $t(s: &str) -> Result<$t, String> {
                s.parse::<$t>().map_err(|_| concat!("expected ", stringify!($t)).to_owned())
            }
        )*
    };
}

integer_parsers!("`+` or `-`": i8, i16, i32, i64, i128, isize);
integer_parsers!("`+`": u8, u16, u32, u64, u128, usize);

/// Parses a decimal number: an optional leading `+` or `-`, digits,
/// and an optional `.` followed by digits. No exponent, no thousands separators.
///
/// The scale of the result is the number of digits after `.`. If the number cannot be
/// represented with this scale (e.g. it has more than 28 digits after `.`), trailing zeros
/// after `.` are removed and the scale decreases. Numbers which still cannot be represented
/// exactly by [`Decimal`] are rejected.
///
/// ```
/// use use_xml::parse;
///
/// assert_eq!(parse::decimal("12.50").unwrap().to_string(), "12.50");
/// let thirty_zeros = format!("1.{}", "0".repeat(30));
/// assert_eq!(parse::decimal(&thirty_zeros).unwrap().to_string(), "1");
/// assert!(parse::decimal(&format!("0.{}1", "0".repeat(28))).is_err());
/// ```
///
/// # Errors
///
/// Returns an error message if the string is not in this format
/// or the number cannot be represented exactly by [`Decimal`].
pub fn decimal(s: &str) -> Result<Decimal, String> {
    let error = || "expected decimal".to_owned();
    let unsigned = s.strip_prefix(['+', '-']).unwrap_or(s);
    let (int_part, frac_part) = match unsigned.split_once('.') {
        Some((int_part, frac_part)) => (int_part, Some(frac_part)),
        None => (unsigned, None),
    };
    let all_digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    if !all_digits(int_part) || !frac_part.is_none_or(all_digits) {
        return Err(error());
    }
    // `from_str_exact` does not accept a leading `+`.
    let s = s.strip_prefix('+').unwrap_or(s);
    if let Ok(parsed) = Decimal::from_str_exact(s) {
        return Ok(parsed);
    }
    // Retry without trailing zeros after `.`; they do not change the value.
    let Some(frac_part) = frac_part else {
        return Err(error());
    };
    let trimmed_frac = frac_part.trim_end_matches('0');
    if trimmed_frac.len() == frac_part.len() {
        return Err(error());
    }
    // `s` ends with `frac_part`, so the length of the rest is on a character boundary
    // (all removed characters are ASCII zeros). A trailing `.` is removed too.
    let trimmed = &s[..s.len() - (frac_part.len() - trimmed_frac.len())];
    let trimmed = trimmed.strip_suffix('.').unwrap_or(trimmed);
    Decimal::from_str_exact(trimmed).map_err(|_| error())
}

/// Like [`decimal`] but rejects negative numbers.
///
/// # Errors
///
/// Returns an error message if [`decimal`] fails or the number is negative.
pub fn decimal_non_negative(s: &str) -> Result<Decimal, String> {
    let d = decimal(s)?;
    if d < Decimal::ZERO {
        Err("expected non-negative decimal".to_owned())
    } else {
        Ok(d)
    }
}

/// Parses exactly `true` or `false`.
///
/// # Errors
///
/// Returns an error message for any other string.
pub fn bool(s: &str) -> Result<bool, String> {
    match s {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err("expected bool".to_owned()),
    }
}

/// Parses a fixed number of ASCII digits.
fn digits(s: &[u8]) -> Option<u32> {
    s.iter().try_fold(0u32, |acc, &b| {
        b.is_ascii_digit().then(|| acc * 10 + u32::from(b - b'0'))
    })
}

/// Parses `YYYY-MM-DDTHH:MM:SS[.fffffffff]` (or with a space instead of `T`)
/// and returns the date time and the rest of the input.
fn primitive_date_time(s: &str) -> Option<(PrimitiveDateTime, &str)> {
    let b = s.as_bytes();
    if b.len() < 19
        || b[4] != b'-'
        || b[7] != b'-'
        || (b[10] != b'T' && b[10] != b' ')
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let year = i32::try_from(digits(&b[0..4])?).ok()?;
    let month = Month::try_from(u8::try_from(digits(&b[5..7])?).ok()?).ok()?;
    let day = u8::try_from(digits(&b[8..10])?).ok()?;
    let hour = u8::try_from(digits(&b[11..13])?).ok()?;
    let minute = u8::try_from(digits(&b[14..16])?).ok()?;
    let second = u8::try_from(digits(&b[17..19])?).ok()?;

    let mut rest = &s[19..];
    let mut nanosecond = 0;
    if let Some(after_dot) = rest.strip_prefix('.') {
        let len = after_dot.bytes().take_while(u8::is_ascii_digit).count();
        if !(1..=9).contains(&len) {
            return None;
        }
        // `len <= 9`, so the value and the padding fit into `u32`.
        nanosecond = digits(&after_dot.as_bytes()[..len])? * 10u32.pow(9 - len as u32);
        rest = &after_dot[len..];
    }

    let date = Date::from_calendar_date(year, month, day).ok()?;
    let time = Time::from_hms_nano(hour, minute, second, nanosecond).ok()?;
    Some((PrimitiveDateTime::new(date, time), rest))
}

/// Parses a UTC date time `YYYY-MM-DDTHH:MM:SS[.fffffffff]Z`.
///
/// A space may be used instead of `T`. The fraction has 1–9 digits.
/// Only `Z` is accepted as the offset; use [`date_time_offset`] for other offsets.
///
/// # Errors
///
/// Returns an error message if the string is not in this format
/// or it is not a valid date and time (e.g. February 30 or hour 24).
pub fn date_time_utc(s: &str) -> Result<OffsetDateTime, String> {
    match primitive_date_time(s) {
        Some((dt, "Z")) => Ok(dt.assume_utc()),
        _ => Err("expected UTC date time in format YYYY-MM-DDTHH:MM:SS[.fffffffff]Z".to_owned()),
    }
}

/// Parses a date time with an offset `YYYY-MM-DDTHH:MM:SS[.fffffffff](Z|+HH:MM|-HH:MM)`.
///
/// A space may be used instead of `T`. The fraction has 1–9 digits.
/// Date times which cannot be converted to UTC (e.g. `9999-12-31T23:59:59-01:00`) are rejected.
///
/// # Errors
///
/// Returns an error message if the string is not in this format, it is not a valid date and time,
/// the offset is out of range (hours above 23 or minutes above 59),
/// or the date time cannot be converted to UTC.
pub fn date_time_offset(s: &str) -> Result<OffsetDateTime, String> {
    let parse_offset = |rest: &str| -> Option<UtcOffset> {
        if rest == "Z" {
            return Some(UtcOffset::UTC);
        }
        let b = rest.as_bytes();
        if b.len() != 6 || b[3] != b':' {
            return None;
        }
        let sign: i8 = match b[0] {
            b'+' => 1,
            b'-' => -1,
            _ => return None,
        };
        let hours = i8::try_from(digits(&b[1..3])?).ok()?;
        let minutes = i8::try_from(digits(&b[4..6])?).ok()?;
        if hours > 23 || minutes > 59 {
            return None;
        }
        UtcOffset::from_hms(sign * hours, sign * minutes, 0).ok()
    };
    primitive_date_time(s)
        .and_then(|(dt, rest)| {
            let dt = dt.assume_offset(parse_offset(rest)?);
            // `time` accepts a date time whose UTC equivalent is outside of its supported range,
            // but then e.g. `OffsetDateTime::to_offset(UtcOffset::UTC)` panics.
            // We work around it by rejecting such date times, so the caller never gets a value
            // which panics on conversion to UTC.
            dt.checked_to_offset(UtcOffset::UTC)?;
            Some(dt)
        })
        .ok_or_else(|| {
            "expected date time in format YYYY-MM-DDTHH:MM:SS[.fffffffff](Z|+HH:MM|-HH:MM)"
                .to_owned()
        })
}

/// Converts the result of `parser` with `f`.
///
/// ```
/// use use_xml::parse;
///
/// let even = parse::map(parse::u32, |n| if n % 2 == 0 { Ok(n / 2) } else { Err("expected even number".to_owned()) });
/// assert_eq!(even("8"), Ok(4));
/// assert!(even("7").is_err());
/// ```
pub fn map<T, U>(
    parser: impl Fn(&str) -> Result<T, String>,
    f: impl Fn(T) -> Result<U, String>,
) -> impl Fn(&str) -> Result<U, String> {
    move |s| f(parser(s)?)
}

/// Checks the result of `parser` with `check`, which returns an error message for invalid values.
///
/// ```
/// use use_xml::parse;
///
/// let small = parse::validate(parse::u32, |n| (*n > 10).then(|| "expected at most 10".to_owned()));
/// assert_eq!(small("8"), Ok(8));
/// assert!(small("11").is_err());
/// ```
pub fn validate<T>(
    parser: impl Fn(&str) -> Result<T, String>,
    check: impl Fn(&T) -> Option<String>,
) -> impl Fn(&str) -> Result<T, String> {
    move |s| {
        let value = parser(s)?;
        match check(&value) {
            None => Ok(value),
            Some(message) => Err(message),
        }
    }
}
