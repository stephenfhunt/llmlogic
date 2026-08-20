//! Temporal values (`spec.md` §4): civil dates, civil timestamps, exact
//! durations — their domains, their text form, and the civil-calendar
//! arithmetic underneath.
//!
//! Three decisions from §17 (2026-08-19) are what make this module small enough
//! to be dependency-free:
//!
//! - **A timestamp is civil.** It denotes a date and a clock reading, not an
//!   instant, so there is no zone database and no leap-second table here. A
//!   zoned source column is normalized to UTC at the import boundary (§13),
//!   which is the only place a zone is ever seen.
//! - **A duration is exact**, counted in microseconds. There is no month or
//!   year unit, so no value in this module has a data-dependent length — which
//!   is also what leaves `m` unambiguously *minutes* in the literal grammar.
//! - **Text is one grammar, used three times.** The `@`-sigilled literal (§3),
//!   `string as date` (§8) and §13's CSV inference all read the forms below;
//!   [`Date`], [`Timestamp`] and [`Duration`] render the *unsigilled* text, and
//!   the `@` is added by the printer (§14). So rendering and reading are
//!   inverse by construction rather than by two implementations agreeing.
//!
//! Day↔calendar conversion is Howard Hinnant's `days_from_civil` /
//! `civil_from_days` (proleptic Gregorian, epoch 1970-01-01), which is exact
//! integer arithmetic with no lookup tables.

use std::fmt;

/// Microseconds in one second, minute, hour and day — the unit ladder the
/// duration grammar and its rendering both walk.
const US_PER_MS: i64 = 1_000;
const US_PER_S: i64 = 1_000_000;
const US_PER_MIN: i64 = 60 * US_PER_S;
const US_PER_HOUR: i64 = 60 * US_PER_MIN;
const US_PER_DAY: i64 = 24 * US_PER_HOUR;

/// Years are four digits (§3), which bounds every value in this module: a date
/// is within `0000-01-01 ..= 9999-12-31`, so a timestamp's microsecond count
/// stays far inside `i64` and every difference of two timestamps is a
/// representable duration.
const MIN_YEAR: i64 = 0;
const MAX_YEAR: i64 = 9999;

/// A civil day, as days since 1970-01-01 (proleptic Gregorian).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Date(i32);

/// A civil date **and** time, as microseconds since 1970-01-01T00:00:00.
///
/// Civil, not an instant: no zone, so this is a calendar reading and a clock
/// reading held together, and subtraction of two of them is exact (§4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Timestamp(i64);

/// A signed elapsed quantity, in microseconds. Always exact — §4 has no
/// calendar duration, so no duration's length depends on where it is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Duration(i64);

/// Why a piece of text is not the temporal value it looked like.
///
/// Every variant names the *component* at fault rather than reporting a
/// position, because these surface as lexical errors on a literal (§3) where
/// "unexpected character" is exactly the message §12 exists to avoid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemporalError {
    /// Not any temporal form — the shape is wrong, not one field of it.
    Shape,
    /// A field is out of range for its neighbours: `2026-02-30`, `10:61:00`.
    Component { field: &'static str, value: String },
    /// Beyond the four-digit-year range this module represents.
    Range,
    /// A duration unit that does not exist, or one §4 deliberately excludes.
    Unit(String),
    /// Duration components out of order, or one unit used twice.
    Order,
}

impl TemporalError {
    /// The message body a diagnostic renders (§12 supplies the surroundings).
    pub fn message(&self) -> String {
        match self {
            TemporalError::Shape => "not a date, timestamp, or duration".to_string(),
            TemporalError::Component { field, value } => {
                format!("{value} is not a valid {field}")
            }
            TemporalError::Range => "outside the representable range (years 0000-9999)".to_string(),
            TemporalError::Unit(unit) => {
                if unit == "M" || unit == "Y" {
                    format!(
                        "`{unit}` is a calendar unit, which has no fixed length; \
                         durations are exact (use days, or `std/time`'s `truncate`)"
                    )
                } else {
                    format!("`{unit}` is not a duration unit (d, h, m, s, ms, us)")
                }
            }
            TemporalError::Order => {
                "duration components must be largest-first and each used once".to_string()
            }
        }
    }

    /// The fix a diagnostic suggests, where there is an actionable one.
    pub fn suggestion(&self) -> Option<String> {
        match self {
            TemporalError::Unit(unit) if unit == "M" || unit == "Y" => Some(
                "for a month or a year, group with `truncate(D, month, M)` from \
                 `import \"std/time\".`"
                    .to_string(),
            ),
            _ => None,
        }
    }
}

// --- Civil calendar arithmetic (Hinnant) ---

/// Days since 1970-01-01 for a civil year/month/day, assuming the date exists.
///
/// Exact integer arithmetic over the proleptic Gregorian calendar: shift the
/// year so the leap day lands at the end of the era, then count eras of 400
/// years, which is the cycle the calendar actually repeats on.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (m + 9) % 12; // March-based month, [0, 11]
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// The civil year/month/day of a day count — the inverse of [`days_from_civil`].
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Days in a civil month, leap years included.
fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

impl Date {
    /// A date from civil parts, validating the day against the month (so
    /// `2026-02-30` is rejected rather than normalized — §12 would rather say
    /// which field is wrong than silently mean a different day).
    pub fn from_ymd(year: i64, month: i64, day: i64) -> Result<Date, TemporalError> {
        if !(MIN_YEAR..=MAX_YEAR).contains(&year) {
            return Err(TemporalError::Range);
        }
        if !(1..=12).contains(&month) {
            return Err(TemporalError::Component {
                field: "month",
                value: month.to_string(),
            });
        }
        if day < 1 || day > days_in_month(year, month) {
            return Err(TemporalError::Component {
                field: "day",
                value: day.to_string(),
            });
        }
        Ok(Date(days_from_civil(year, month, day) as i32))
    }

    /// The civil year, month and day.
    pub fn ymd(self) -> (i64, i64, i64) {
        civil_from_days(self.0 as i64)
    }

    /// Days since 1970-01-01.
    pub fn days(self) -> i32 {
        self.0
    }

    /// A date from a raw day count, range-checked.
    pub fn from_days(days: i64) -> Result<Date, TemporalError> {
        let (year, _, _) = civil_from_days(days);
        if !(MIN_YEAR..=MAX_YEAR).contains(&year) {
            return Err(TemporalError::Range);
        }
        Ok(Date(days as i32))
    }

    /// Midnight of this day — the exact widening `date as timestamp` performs
    /// (§8). Exact because a timestamp is civil: no zone can move midnight.
    pub fn at_midnight(self) -> Timestamp {
        Timestamp(self.0 as i64 * US_PER_DAY)
    }
}

impl Timestamp {
    /// A timestamp from civil parts and a microsecond-of-second.
    pub fn from_parts(
        year: i64,
        month: i64,
        day: i64,
        hour: i64,
        minute: i64,
        second: i64,
        micros: i64,
    ) -> Result<Timestamp, TemporalError> {
        let date = Date::from_ymd(year, month, day)?;
        if !(0..=23).contains(&hour) {
            return Err(TemporalError::Component {
                field: "hour",
                value: hour.to_string(),
            });
        }
        if !(0..=59).contains(&minute) {
            return Err(TemporalError::Component {
                field: "minute",
                value: minute.to_string(),
            });
        }
        // 60 is a leap second; a civil timestamp has no room for one (§4).
        if !(0..=59).contains(&second) {
            return Err(TemporalError::Component {
                field: "second",
                value: second.to_string(),
            });
        }
        Ok(Timestamp(
            date.0 as i64 * US_PER_DAY
                + hour * US_PER_HOUR
                + minute * US_PER_MIN
                + second * US_PER_S
                + micros,
        ))
    }

    /// Civil year, month, day, hour, minute, second, microsecond.
    pub fn parts(self) -> (i64, i64, i64, i64, i64, i64, i64) {
        // Floor-divide so pre-epoch timestamps keep a non-negative time of day.
        let days = self.0.div_euclid(US_PER_DAY);
        let rem = self.0.rem_euclid(US_PER_DAY);
        let (year, month, day) = civil_from_days(days);
        (
            year,
            month,
            day,
            rem / US_PER_HOUR,
            (rem % US_PER_HOUR) / US_PER_MIN,
            (rem % US_PER_MIN) / US_PER_S,
            rem % US_PER_S,
        )
    }

    /// Microseconds since 1970-01-01T00:00:00.
    pub fn micros(self) -> i64 {
        self.0
    }

    /// A timestamp from a raw microsecond count, range-checked.
    pub fn from_micros(micros: i64) -> Result<Timestamp, TemporalError> {
        let (year, _, _) = civil_from_days(micros.div_euclid(US_PER_DAY));
        if !(MIN_YEAR..=MAX_YEAR).contains(&year) {
            return Err(TemporalError::Range);
        }
        Ok(Timestamp(micros))
    }

    /// The civil day this timestamp falls in — what `std/time`'s
    /// `truncate(T, day, D)` computes, and what `timestamp as date` refuses to
    /// do silently (§8).
    pub fn date(self) -> Date {
        Date(self.0.div_euclid(US_PER_DAY) as i32)
    }
}

impl Duration {
    /// A duration from a microsecond count.
    pub fn from_micros(micros: i64) -> Duration {
        Duration(micros)
    }

    /// The microsecond count.
    pub fn micros(self) -> i64 {
        self.0
    }
}

// --- Rendering: the unsigilled canonical text (§14 adds the `@`) ---

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (y, m, d) = self.ymd();
        write!(f, "{y:04}-{m:02}-{d:02}")
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (y, mo, d, h, mi, s, us) = self.parts();
        write!(f, "{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}")?;
        // The fractional part appears only when nonzero (§14), and trailing
        // zeros are trimmed so one value has one spelling.
        if us != 0 {
            let frac = format!("{us:06}");
            write!(f, ".{}", frac.trim_end_matches('0'))?;
        }
        Ok(())
    }
}

impl fmt::Display for Duration {
    /// Largest-unit decomposition, zero components omitted, `0s` for zero
    /// (§14). Normalizing here is what makes printing a function of the value
    /// rather than of how it was written: `90m` and `PT1H30M` both print
    /// `1h30m`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 0 {
            return write!(f, "0s");
        }
        if self.0 < 0 {
            write!(f, "-")?;
        }
        // `unsigned_abs` rather than `-n`: i64::MIN has no positive counterpart.
        let mut rest = self.0.unsigned_abs();
        for (unit_us, suffix) in [
            (US_PER_DAY, "d"),
            (US_PER_HOUR, "h"),
            (US_PER_MIN, "m"),
            (US_PER_S, "s"),
            (US_PER_MS, "ms"),
            (1, "us"),
        ] {
            let unit_us = unit_us as u64;
            let n = rest / unit_us;
            if n != 0 {
                write!(f, "{n}{suffix}")?;
                rest -= n * unit_us;
            }
        }
        Ok(())
    }
}

// --- Reading: one grammar, three callers (§3 literal, §8 cast, §13 import) ---

/// Which temporal type a piece of text spells, for callers that accept any
/// (the `@` literal, and §13's column-type inference).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Temporal {
    Date(Date),
    Timestamp(Timestamp),
    Duration(Duration),
}

/// Reads any temporal form from **unsigilled** text.
///
/// The three forms are distinguishable by shape with no backtracking: a
/// duration starts with a digit-or-sign followed by a unit letter (or `P`), a
/// date is `YYYY-MM-DD`, and a timestamp is a date followed by `T`.
pub fn parse_temporal(text: &str) -> Result<Temporal, TemporalError> {
    if looks_like_duration(text) {
        return parse_duration(text).map(Temporal::Duration);
    }
    if text.contains('T') {
        return parse_timestamp(text).map(Temporal::Timestamp);
    }
    parse_date(text).map(Temporal::Date)
}

/// Does this text take the duration shape? A leading `P`/`-P` (the ISO alias),
/// or digits followed by a unit letter rather than a `-`.
fn looks_like_duration(text: &str) -> bool {
    let body = text.strip_prefix('-').unwrap_or(text);
    if body.starts_with('P') {
        return true;
    }
    let digits = body.chars().take_while(|c| c.is_ascii_digit()).count();
    digits > 0 && body[digits..].starts_with(|c: char| c.is_ascii_alphabetic())
}

/// Reads a date in ISO-8601 extended form, `YYYY-MM-DD`.
pub fn parse_date(text: &str) -> Result<Date, TemporalError> {
    let (year, month, day) = split_date(text)?;
    Date::from_ymd(year, month, day)
}

fn split_date(text: &str) -> Result<(i64, i64, i64), TemporalError> {
    let bytes = text.as_bytes();
    // Fixed width: four-digit year, two-digit month and day (§3). Fixed width
    // is what makes the form unambiguous against arithmetic on three integers.
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(TemporalError::Shape);
    }
    Ok((
        parse_digits(&text[0..4])?,
        parse_digits(&text[5..7])?,
        parse_digits(&text[8..10])?,
    ))
}

fn parse_digits(text: &str) -> Result<i64, TemporalError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(TemporalError::Shape);
    }
    text.parse::<i64>().map_err(|_| TemporalError::Range)
}

/// Reads a timestamp: a date, `T`, `HH:MM:SS`, and an optional fractional
/// second of up to six digits.
///
/// A zone offset is **not** accepted: §4's timestamp is civil, so there is
/// nothing for one to mean. §13 converts a zoned source column to UTC at the
/// import boundary instead, which is the one place the conversion is stated.
pub fn parse_timestamp(text: &str) -> Result<Timestamp, TemporalError> {
    let (date_text, time_text) = text.split_once('T').ok_or(TemporalError::Shape)?;
    let (year, month, day) = split_date(date_text)?;
    let (clock, frac) = match time_text.split_once('.') {
        Some((clock, frac)) => (clock, Some(frac)),
        None => (time_text, None),
    };
    let bytes = clock.as_bytes();
    if bytes.len() != 8 || bytes[2] != b':' || bytes[5] != b':' {
        return Err(TemporalError::Shape);
    }
    let hour = parse_digits(&clock[0..2])?;
    let minute = parse_digits(&clock[3..5])?;
    let second = parse_digits(&clock[6..8])?;
    let micros = match frac {
        None => 0,
        Some(frac) => {
            if frac.is_empty() || frac.len() > 6 || !frac.bytes().all(|b| b.is_ascii_digit()) {
                // More than six digits is a precision the value model cannot
                // hold, and §13 refuses a lossy narrowing rather than rounding.
                return Err(TemporalError::Component {
                    field: "fractional second",
                    value: frac.to_string(),
                });
            }
            parse_digits(frac)? * 10i64.pow(6 - frac.len() as u32)
        }
    };
    Timestamp::from_parts(year, month, day, hour, minute, second, micros)
}

/// Reads a duration, in the friendly form (`1d12h`, `90m`, `500ms`, `0s`) or
/// the ISO-8601 alias (`P1D`, `PT36H`).
pub fn parse_duration(text: &str) -> Result<Duration, TemporalError> {
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let magnitude = if body.starts_with('P') {
        parse_iso_duration(body)?
    } else {
        parse_friendly_duration(body)?
    };
    Ok(Duration(if negative {
        magnitude.checked_neg().ok_or(TemporalError::Range)?
    } else {
        magnitude
    }))
}

/// The unit ladder, longest suffix first so `ms` is never read as `m` then `s`.
const UNITS: [(&str, i64); 6] = [
    ("ms", US_PER_MS),
    ("us", 1),
    ("d", US_PER_DAY),
    ("h", US_PER_HOUR),
    ("m", US_PER_MIN),
    ("s", US_PER_S),
];

/// Rank in the largest-first order components must appear in.
fn unit_rank(unit: &str) -> usize {
    match unit {
        "d" => 0,
        "h" => 1,
        "m" => 2,
        "s" => 3,
        "ms" => 4,
        _ => 5,
    }
}

fn parse_friendly_duration(text: &str) -> Result<i64, TemporalError> {
    if text.is_empty() {
        return Err(TemporalError::Shape);
    }
    let mut total: i64 = 0;
    let mut rest = text;
    let mut last_rank: Option<usize> = None;
    while !rest.is_empty() {
        let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 {
            return Err(TemporalError::Shape);
        }
        let value = parse_digits(&rest[..digits])?;
        rest = &rest[digits..];
        let (unit, unit_us) = UNITS
            .iter()
            .find(|(unit, _)| rest.starts_with(unit))
            .copied()
            .ok_or_else(|| TemporalError::Unit(unit_text(rest)))?;
        rest = &rest[unit.len()..];
        // Largest-first and each unit once: a repeat or a swap is a typo, and
        // summing it silently would mean something the writer did not.
        let rank = unit_rank(unit);
        if last_rank.is_some_and(|last| rank <= last) {
            return Err(TemporalError::Order);
        }
        last_rank = Some(rank);
        total = value
            .checked_mul(unit_us)
            .and_then(|scaled| total.checked_add(scaled))
            .ok_or(TemporalError::Range)?;
    }
    Ok(total)
}

/// Splits `digits[.digits]` off the head, returning the whole part, the
/// fractional digits, and the tail.
fn split_decimal(text: &str) -> Result<(i64, &str, &str), TemporalError> {
    let whole_len = text.chars().take_while(|c| c.is_ascii_digit()).count();
    if whole_len == 0 {
        return Err(TemporalError::Shape);
    }
    let whole = parse_digits(&text[..whole_len])?;
    let rest = &text[whole_len..];
    let Some(after_point) = rest.strip_prefix('.') else {
        return Ok((whole, "", rest));
    };
    let frac_len = after_point
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .count();
    if frac_len == 0 {
        return Err(TemporalError::Shape);
    }
    Ok((whole, &after_point[..frac_len], &after_point[frac_len..]))
}

/// A fractional component's microseconds, refusing a precision the value model
/// cannot hold rather than rounding it away (§13's rule, applied to text).
fn fractional_micros(fraction: &str, unit_us: i64) -> Result<i64, TemporalError> {
    if fraction.is_empty() {
        return Ok(0);
    }
    let digits = parse_digits(fraction)?;
    let scale = 10i64
        .checked_pow(fraction.len() as u32)
        .ok_or(TemporalError::Range)?;
    let scaled = digits.checked_mul(unit_us).ok_or(TemporalError::Range)?;
    if scaled % scale != 0 {
        return Err(TemporalError::Component {
            field: "fractional component",
            value: fraction.to_string(),
        });
    }
    Ok(scaled / scale)
}

/// The unit-ish text at the head of `rest`, for the error message.
fn unit_text(rest: &str) -> String {
    rest.chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
}

/// The ISO-8601 alias: `P[nD][T[nH][nM][nS]]`, weeks accepted as exactly seven
/// days. `Y` and `M` before the `T` are calendar units, which §4 excludes — and
/// the error says so rather than reporting an unknown unit.
fn parse_iso_duration(text: &str) -> Result<i64, TemporalError> {
    let body = &text[1..];
    let (date_part, time_part) = match body.split_once('T') {
        Some((date, time)) => (date, Some(time)),
        None => (body, None),
    };
    if date_part.is_empty() && time_part.map(str::is_empty).unwrap_or(true) {
        return Err(TemporalError::Shape);
    }
    let mut total: i64 = 0;
    for (part, units) in [
        (date_part, &[("W", 7 * US_PER_DAY), ("D", US_PER_DAY)][..]),
        (
            time_part.unwrap_or(""),
            &[("H", US_PER_HOUR), ("M", US_PER_MIN), ("S", US_PER_S)][..],
        ),
    ] {
        let mut rest = part;
        while !rest.is_empty() {
            // ISO allows a fractional component (`PT0.5S`), and refusing a
            // well-formed standard spelling would buy nothing — the alias
            // exists precisely so text from elsewhere reads.
            let (whole, fraction, tail) = split_decimal(rest)?;
            rest = tail;
            let (unit, unit_us) = units
                .iter()
                .find(|(unit, _)| rest.starts_with(unit))
                .copied()
                .ok_or_else(|| TemporalError::Unit(unit_text(rest)))?;
            rest = &rest[unit.len()..];
            let scaled = whole.checked_mul(unit_us).ok_or(TemporalError::Range)?;
            total = scaled
                .checked_add(fractional_micros(fraction, unit_us)?)
                .and_then(|component| total.checked_add(component))
                .ok_or(TemporalError::Range)?;
        }
    }
    Ok(total)
}

// --- Lenient reading, for §13's declared-type coercion ---

/// Reads a timestamp for an **explicitly declared** column (§13).
///
/// Deliberately more permissive than [`parse_timestamp`], which types a literal
/// and infers a CSV column: a declared type *coerces*, and the space-separated
/// form (`2026-08-19 10:30:00`) is what database and spreadsheet exports carry.
/// Three widenings, all of them stated in §13:
///
/// - a space in place of the `T`;
/// - a bare date, which means midnight — the same exact widening `date as
///   timestamp` performs;
/// - a trailing zone offset (`Z`, `+01:00`, `-0500`), **applied and dropped**,
///   since §4's timestamp is civil. This is the one place a zone is understood
///   at all, and the offset is applied rather than ignored so the instant
///   survives even though its displayed clock reading may change.
pub fn parse_timestamp_lenient(text: &str) -> Result<Timestamp, TemporalError> {
    let text = text.trim();
    let (body, offset_micros) = split_offset(text)?;
    let body = body.replacen(' ', "T", 1);
    let base = if body.contains('T') {
        parse_timestamp(&body)?
    } else {
        parse_date(&body)?.at_midnight()
    };
    // Subtract the offset: a reading of 10:30+01:00 is 09:30 UTC.
    Timestamp::from_micros(
        base.micros()
            .checked_sub(offset_micros)
            .ok_or(TemporalError::Range)?,
    )
}

/// Splits a trailing zone offset off a timestamp, returning its microseconds.
fn split_offset(text: &str) -> Result<(&str, i64), TemporalError> {
    if let Some(body) = text.strip_suffix('Z').or_else(|| text.strip_suffix('z')) {
        return Ok((body, 0));
    }
    // An offset's sign sits at index 10 or later (past the date's own hyphens),
    // which is what keeps `2026-08-19` from being read as an offset.
    let Some(position) = text
        .char_indices()
        .filter(|(i, c)| *i >= 10 && (*c == '+' || *c == '-'))
        .map(|(i, _)| i)
        .next_back()
    else {
        return Ok((text, 0));
    };
    let (body, offset) = text.split_at(position);
    let negative = offset.starts_with('-');
    let digits: String = offset[1..].chars().filter(|c| *c != ':').collect();
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(TemporalError::Component {
            field: "zone offset",
            value: offset.to_string(),
        });
    }
    let hours = parse_digits(&digits[0..2])?;
    let minutes = parse_digits(&digits[2..4])?;
    if hours > 23 || minutes > 59 {
        return Err(TemporalError::Component {
            field: "zone offset",
            value: offset.to_string(),
        });
    }
    let magnitude = hours * US_PER_HOUR + minutes * US_PER_MIN;
    Ok((body, if negative { -magnitude } else { magnitude }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// A date somewhere in the representable range, as civil parts.
    fn arb_date() -> impl Strategy<Value = Date> {
        (0i64..=9999, 1i64..=12, 1i64..=31).prop_filter_map("a real calendar day", |(y, m, d)| {
            Date::from_ymd(y, m, d).ok()
        })
    }

    fn arb_timestamp() -> impl Strategy<Value = Timestamp> {
        (arb_date(), 0i64..24, 0i64..60, 0i64..60, 0i64..1_000_000).prop_map(
            |(date, h, mi, s, us)| {
                let (y, mo, d) = date.ymd();
                Timestamp::from_parts(y, mo, d, h, mi, s, us).expect("parts are in range")
            },
        )
    }

    fn arb_duration() -> impl Strategy<Value = Duration> {
        // Wide enough to reach every unit of the decomposition, both signs.
        (-400_000_000_000_000i64..400_000_000_000_000).prop_map(Duration::from_micros)
    }

    proptest! {
        /// **T1** — a temporal value's canonical text reads back as the same
        /// value, for all three types. This is §14's closure property at the
        /// value layer: what the printer emits, the lexer must accept.
        #[test]
        fn t1_date_text_round_trips(date in arb_date()) {
            prop_assert_eq!(parse_date(&date.to_string()), Ok(date));
            prop_assert_eq!(parse_temporal(&date.to_string()), Ok(Temporal::Date(date)));
        }

        #[test]
        fn t1_timestamp_text_round_trips(ts in arb_timestamp()) {
            prop_assert_eq!(parse_timestamp(&ts.to_string()), Ok(ts));
            prop_assert_eq!(parse_temporal(&ts.to_string()), Ok(Temporal::Timestamp(ts)));
        }

        #[test]
        fn t1_duration_text_round_trips(duration in arb_duration()) {
            prop_assert_eq!(parse_duration(&duration.to_string()), Ok(duration));
            prop_assert_eq!(
                parse_temporal(&duration.to_string()),
                Ok(Temporal::Duration(duration))
            );
        }

        /// The non-vacuity half of T1: the duration generator actually reaches
        /// every unit of the decomposition and both signs, so a round-trip that
        /// only ever saw `0s` cannot pass for a proof (`testing.md` rule 2).
        #[test]
        fn t1_duration_generator_reaches_every_unit(
            durations in proptest::collection::vec(arb_duration(), 200)
        ) {
            let texts: Vec<String> = durations.iter().map(Duration::to_string).collect();
            for unit in ["d", "h", "m", "s", "ms", "us"] {
                prop_assert!(
                    texts.iter().any(|t| t.contains(unit)),
                    "no generated duration used `{}`",
                    unit
                );
            }
            prop_assert!(texts.iter().any(|t| t.starts_with('-')), "no negative duration");
        }

        /// Civil conversion is a bijection on days — the property both
        /// `Date::ymd` and every extraction relation rest on.
        #[test]
        fn civil_conversion_round_trips(days in -1_000_000i64..1_000_000) {
            let (y, m, d) = civil_from_days(days);
            prop_assert_eq!(days_from_civil(y, m, d), days);
        }

        /// Consecutive days differ by exactly one, which is what makes
        /// `date - date` a day count rather than an approximation.
        #[test]
        fn consecutive_days_are_one_apart(days in -1_000_000i64..1_000_000) {
            let (y, m, d) = civil_from_days(days);
            let (y2, m2, d2) = civil_from_days(days + 1);
            prop_assert_eq!(days_from_civil(y2, m2, d2) - days_from_civil(y, m, d), 1);
        }
    }

    #[test]
    fn a_day_that_does_not_exist_names_the_field() {
        assert_eq!(
            Date::from_ymd(2026, 2, 30),
            Err(TemporalError::Component {
                field: "day",
                value: "30".to_string()
            })
        );
        assert!(Date::from_ymd(2024, 2, 29).is_ok(), "2024 is a leap year");
        assert!(Date::from_ymd(2023, 2, 29).is_err(), "2023 is not");
        assert!(Date::from_ymd(2000, 2, 29).is_ok(), "2000 is a leap year");
        assert!(Date::from_ymd(1900, 2, 29).is_err(), "1900 is not");
    }

    #[test]
    fn duration_prints_the_canonical_decomposition() {
        // Printing is a function of the *value*: two spellings of one duration
        // print identically (§14).
        assert_eq!(parse_duration("90m").unwrap().to_string(), "1h30m");
        assert_eq!(parse_duration("36h").unwrap().to_string(), "1d12h");
        assert_eq!(parse_duration("1d12h").unwrap().to_string(), "1d12h");
        assert_eq!(parse_duration("0s").unwrap().to_string(), "0s");
        assert_eq!(parse_duration("-1d12h").unwrap().to_string(), "-1d12h");
        assert_eq!(parse_duration("1500ms").unwrap().to_string(), "1s500ms");
        assert_eq!(Duration::from_micros(1).to_string(), "1us");
    }

    #[test]
    fn the_iso_alias_reads_and_prints_friendly() {
        assert_eq!(parse_duration("P1D").unwrap().to_string(), "1d");
        assert_eq!(parse_duration("PT36H").unwrap().to_string(), "1d12h");
        assert_eq!(parse_duration("P1DT12H").unwrap().to_string(), "1d12h");
        assert_eq!(parse_duration("P1W").unwrap().to_string(), "7d");
        // ISO's fractional component reads, and normalizes on the way out.
        assert_eq!(parse_duration("PT0.5S").unwrap().to_string(), "500ms");
        assert_eq!(parse_duration("P0.5D").unwrap().to_string(), "12h");
        // A precision the value model cannot hold is refused, not rounded.
        assert!(matches!(
            parse_duration("PT0.0000001S"),
            Err(TemporalError::Component {
                field: "fractional component",
                ..
            })
        ));
    }

    #[test]
    fn a_calendar_unit_says_why_it_does_not_exist() {
        // §4 has no calendar duration, and the message points at the construct
        // that does the job rather than reporting an unknown unit.
        let error = parse_duration("P1M").expect_err("months are excluded");
        assert_eq!(error, TemporalError::Unit("M".to_string()));
        assert!(
            error.message().contains("no fixed length"),
            "{}",
            error.message()
        );
        assert!(error.suggestion().is_some_and(|s| s.contains("truncate")));
        assert_eq!(
            parse_duration("P1Y"),
            Err(TemporalError::Unit("Y".to_string()))
        );
        // The friendly form has no such unit to confuse: `m` is minutes.
        assert_eq!(parse_duration("1m").unwrap().micros(), 60 * US_PER_S);
    }

    #[test]
    fn duration_components_are_largest_first_and_used_once() {
        assert_eq!(parse_duration("1h1d"), Err(TemporalError::Order));
        assert_eq!(parse_duration("1d1d"), Err(TemporalError::Order));
        assert_eq!(
            parse_duration("1x"),
            Err(TemporalError::Unit("x".to_string()))
        );
        assert_eq!(parse_duration("d"), Err(TemporalError::Shape));
    }

    #[test]
    fn a_timestamp_carries_only_the_precision_it_can_hold() {
        let ts = parse_timestamp("2026-08-19T10:30:00.5").unwrap();
        assert_eq!(ts.to_string(), "2026-08-19T10:30:00.5");
        assert_eq!(
            parse_timestamp("2026-08-19T10:30:00").unwrap().to_string(),
            "2026-08-19T10:30:00"
        );
        assert!(matches!(
            parse_timestamp("2026-08-19T10:30:00.1234567"),
            Err(TemporalError::Component {
                field: "fractional second",
                ..
            })
        ));
        assert!(matches!(
            parse_timestamp("2026-08-19T10:61:00"),
            Err(TemporalError::Component {
                field: "minute",
                ..
            })
        ));
        // A zone offset is not a civil timestamp (§4) — the literal refuses it.
        assert_eq!(
            parse_timestamp("2026-08-19T10:30:00Z"),
            Err(TemporalError::Shape)
        );
    }

    #[test]
    fn the_shape_test_separates_the_three_forms() {
        assert!(matches!(
            parse_temporal("2026-08-19"),
            Ok(Temporal::Date(_))
        ));
        assert!(matches!(
            parse_temporal("2026-08-19T10:30:00"),
            Ok(Temporal::Timestamp(_))
        ));
        assert!(matches!(parse_temporal("1d12h"), Ok(Temporal::Duration(_))));
        assert!(matches!(parse_temporal("-1d"), Ok(Temporal::Duration(_))));
        assert!(matches!(parse_temporal("P1D"), Ok(Temporal::Duration(_))));
        // Not a temporal at all: the date shape is fixed-width on purpose.
        assert_eq!(parse_temporal("2026-8-19"), Err(TemporalError::Shape));
    }

    #[test]
    fn declared_column_coercion_is_the_permissive_reader() {
        // §13: a declared type coerces, and reads what real exports carry.
        assert_eq!(
            parse_timestamp_lenient("2026-08-19 10:30:00")
                .unwrap()
                .to_string(),
            "2026-08-19T10:30:00"
        );
        // A bare date is midnight — the same exact widening `as timestamp` does.
        assert_eq!(
            parse_timestamp_lenient("2026-08-19").unwrap(),
            Date::from_ymd(2026, 8, 19).unwrap().at_midnight()
        );
        // An offset is applied and dropped: the instant survives, the clock
        // reading may change (§13).
        assert_eq!(
            parse_timestamp_lenient("2026-08-19T10:30:00+01:00")
                .unwrap()
                .to_string(),
            "2026-08-19T09:30:00"
        );
        assert_eq!(
            parse_timestamp_lenient("2026-08-19T10:30:00-05:00")
                .unwrap()
                .to_string(),
            "2026-08-19T15:30:00"
        );
        assert_eq!(
            parse_timestamp_lenient("2026-08-19T10:30:00Z")
                .unwrap()
                .to_string(),
            "2026-08-19T10:30:00"
        );
    }

    #[test]
    fn midnight_and_the_day_of_a_timestamp_are_inverse() {
        let date = Date::from_ymd(2026, 8, 19).unwrap();
        assert_eq!(date.at_midnight().date(), date);
        let ts = parse_timestamp("2026-08-19T10:30:00").unwrap();
        assert_eq!(ts.date(), date);
        // Before the epoch too, where a truncating division would be wrong.
        let old = parse_timestamp("1900-03-04T01:00:00").unwrap();
        assert_eq!(old.date(), Date::from_ymd(1900, 3, 4).unwrap());
    }
}
