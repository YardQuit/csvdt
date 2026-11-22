// Reading and writing RFC3339 timestamps and ISO 8601 durations.

use chrono::{DateTime, Datelike, FixedOffset, Timelike};

// The marker every conversion writes in place of a result it could not
// produce. One word for all of them, which is what makes `grep -c parse_err`
// the answer to "did anything fail", see KNOWN LIMITS.
pub(crate) const PARSE_ERR: &str = "parse_err";

// Whether a moment can be written as RFC3339 at all. Its date-fullyear is
// four digits, so 9999 is the last year there is; chrono reaches far further
// (to +/-262143) and falls back to ISO 8601's expanded form outside that,
// which is a different format wearing the same shape.
//
// The floor is 0001 rather than 0000, which is the one place this range is
// narrower than four digits allow. There is no year zero: the Gregorian and
// Julian calendars run 1 BC straight into AD 1, and ISO 8601 only writes 0000
// for 1 BC by prior agreement between the parties exchanging the data. RFC3339
// makes no such agreement, so a 0000 in a file is a value from outside the
// calendar the rest of the column is in, far more likely a sentinel or a
// miscounted epoch than a date anybody recorded.
//
// The year is read from the value as it will be rendered, not from its UTC
// year, because a conversion can carry it over the edge without moving the
// instant: 9999-12-31T23:59:59+00:00 is year 10000 on a +14:00 clock while
// still 9999 in UTC. Generic over the zone for that reason: -u renders in
// UTC, -o in a fixed offset, -l in a named zone, and all three have to be
// asked about the calendar they will actually print.
pub(crate) fn is_rfc3339_year<Zone: chrono::TimeZone>(datetime: &DateTime<Zone>) -> bool {
    (1..=9999).contains(&datetime.year())
}

// Whether the offset a moment will be rendered with can be written as RFC3339
// at all. Its time-offset is "+"/"-" HH ":" MM, whole minutes, with nowhere
// to put a seconds part.
//
// Only -l can produce one that is not, and it does so from ordinary data. A
// zone's offset before it adopted standard time is the local mean time of the
// place, which is whatever fraction of an hour its longitude works out to:
// Europe/Amsterdam +00:17:30, America/New_York -04:56:02. Nor is that only
// ancient history, Africa/Monrovia kept -00:44:30 until January 1972, inside
// the range of real logs.
//
// chrono renders those by rounding the offset to the nearest minute while the
// clock time keeps its seconds, so the two halves of the result disagree:
// 1971-06-01T12:00:00Z would come out as 1971-06-01T11:15:30-00:45, which
// reads back as 12:00:30Z. A timestamp naming a different instant from the one
// it was made from is the quiet wrong answer this tool exists to avoid, so
// such a value is marked instead, and the input is still there beside it.
pub(crate) fn is_rfc3339_offset<Zone: chrono::TimeZone>(datetime: &DateTime<Zone>) -> bool {
    use chrono::Offset;
    datetime.offset().fix().local_minus_utc() % 60 == 0
}

// Renders a converted moment as RFC3339, or the parse_err marker when the
// conversion has taken it outside the format. Writing chrono's expanded form
// instead would put a value in an RFC3339 column that is not RFC3339, cannot be
// read back, and gives no sign of the boundary it crossed. The same goes for a
// sub-minute offset, which chrono renders by rounding the offset alone and so
// moves the instant.
pub(crate) fn to_rfc3339_in_range<Zone: chrono::TimeZone>(datetime: DateTime<Zone>) -> String
where
    Zone::Offset: std::fmt::Display,
{
    match is_rfc3339_year(&datetime) && is_rfc3339_offset(&datetime) {
        true => datetime.to_rfc3339(),
        false => String::from(PARSE_ERR),
    }
}

// Parses an RFC3339 timestamp, tolerating a trailing numeric offset that's
// missing its colon (e.g. "+0000" or "-0530" instead of "+00:00"/"-05:30").
// Such timestamps are common in data exported from tools that follow
// RFC2822/ISO 8601's looser offset format instead of strict RFC3339. The
// strict parse is tried first and used as-is when it succeeds, so
// well-formed input pays no extra cost.
pub(crate) fn parse_rfc3339_lenient(value: &str) -> Result<DateTime<FixedOffset>, chrono::ParseError> {
    // Surrounding whitespace is skipped here rather than trimmed out of the
    // field, so a file written with ", " separators converts without --trim
    // having to alter what is passed through. Trimming the field first would
    // reach inside quoted fields and throw away spaces the source had quoted
    // to keep.
    let value = value.trim();
    // chrono accepts any number of fractional digits and silently truncates
    // to nanoseconds: a tenth digit vanishes with no error. This tool's
    // promise is the opposite, "what the source had is what comes out", so
    // precision a value cannot carry is refused into parse_err rather than
    // quietly cut.
    if fractional_digits(value) > 9 {
        return Err(refused());
    }
    let strict = chrono::DateTime::parse_from_rfc3339(value);
    let parsed = if strict.is_ok() {
        strict
    } else {
        match insert_offset_colon(value) {
            Some(fixed) => chrono::DateTime::parse_from_rfc3339(&fixed),
            None => strict,
        }
    };
    match parsed {
        Ok(stamp) if !leap_second_is_possible(stamp) => Err(refused()),
        // Year 0000 is four digits but no year of the calendar the format
        // uses, see is_rfc3339_year. Refused on the way in, so every action
        // marks it the same way rather than only those that write a
        // timestamp: -s would otherwise copy it out and -d measure from it.
        Ok(stamp) if stamp.year() == 0 => Err(refused()),
        other => other,
    }
}

// The error parse_rfc3339_lenient gives for a value chrono reads but this tool
// refuses. Made by parsing an empty string, since chrono gives no way to
// construct one.
pub(crate) fn refused() -> chrono::ParseError {
    match chrono::DateTime::parse_from_rfc3339("") {
        Err(error) => error,
        Ok(_) => unreachable!("an empty string is not a timestamp"),
    }
}

// Whether a value wearing :60 is somewhere a leap second can be.
//
// chrono reads a :60 seconds field on any minute at all and folds it onto the
// second that follows, so 2024-06-15T14:00:60Z and 14:00:60Z's neighbour
// 14:01:00Z arrive as one instant: -d between them is PT0S, and no conversion
// can tell them apart. That makes a damaged value: :60 is what a formatter
// that rounds 59.5 up without carrying writes, read as a different instant
// than the one it names, quietly, where this tool's rule is to mark what it
// cannot answer faithfully. Written back out wearing :60, it would also not be
// RFC3339 on an ordinary minute: standard readers refuse it.
//
// A leap second is the last second of a UTC month and is simultaneous
// everywhere, so the test is on the instant rather than on the clock face the
// record happens to carry: 2017-01-01T05:29:60+05:30 is the real one at the
// end of 2016 and is kept, while 2016-12-31T23:59:60+02:00 reads 23:59:60 on
// its own clock but lands mid-evening in UTC and is not. Structural rather
// than a table of the leap seconds IERS has declared: a table would refuse
// every future one until the binary was rebuilt, and the format itself
// permits them.
pub(crate) fn leap_second_is_possible(stamp: DateTime<FixedOffset>) -> bool {
    // chrono carries a leap second as :59 with a nanosecond field run past a
    // whole second, which is the only way to recognise one after parsing.
    if stamp.nanosecond() < 1_000_000_000 {
        return true;
    }
    let utc = stamp.with_timezone(&chrono::Utc);
    utc.hour() == 23 && utc.minute() == 59 && is_last_day_of_month(utc.date_naive())
}

pub(crate) fn is_last_day_of_month(date: chrono::NaiveDate) -> bool {
    match date.succ_opt() {
        Some(next) => next.month() != date.month(),
        // 9999-12-31 has no next day to compare against, and is one.
        None => true,
    }
}

// How many digits follow the first '.', which in anything RFC3339-shaped is
// the start of the fractional seconds, no other part of the format holds a
// dot. A value where it is something else entirely fails to parse either way.
pub(crate) fn fractional_digits(value: &str) -> usize {
    match value.find('.') {
        None => 0,
        Some(dot) => value[dot + 1..]
            .bytes()
            .take_while(|byte| byte.is_ascii_digit())
            .count(),
    }
}

// If `value` ends in a 4-digit numeric offset with no colon (e.g. "+0000"),
// returns it with a colon inserted (e.g. "+00:00"). Returns None for
// anything else, including offsets that already have a colon, or a "Z"
// suffix, so it never touches already-valid RFC3339 strings.
pub(crate) fn insert_offset_colon(value: &str) -> Option<String> {
    if value.len() < 5 {
        return None;
    }
    let sign_index = value.len() - 5;
    let sign = value.as_bytes()[sign_index];
    if sign != b'+' && sign != b'-' {
        return None;
    }
    // Everything below slices `value` by byte index, which panics on a
    // position inside a character. Safe only because the check above found an
    // ASCII byte here: no byte of a multi-byte character can equal '+' or '-',
    // so sign_index is a character boundary, and the four ASCII digits
    // confirmed next make sign_index+3 one as well. A sign accepted in some
    // other alphabet, Unicode's minus, say, would need is_char_boundary
    // before any of this. A value ending mid-character otherwise reaches here
    // routinely, since the last five bytes of anything at all are tested.
    let offset_digits = &value[sign_index+1..];
    if offset_digits.len() == 4 && offset_digits.bytes().all(|b| b.is_ascii_digit()) {
        let mut fixed = String::with_capacity(value.len() + 1);
        fixed.push_str(&value[..sign_index+3]);
        fixed.push(':');
        fixed.push_str(&value[sign_index+3..]);
        Some(fixed)
    } else {
        None
    }
}

// Formats a duration as an ISO 8601 duration string: P[n]DT[n]H[n]M[n]S.
// Zero-valued components are omitted (a bare "P" with nothing after it
// would be invalid, so an all-zero duration renders as "PT0S"). Not the
// same as chrono::TimeDelta's own Display, which dumps the whole duration
// as a single raw-seconds "PT<n>S" instead of breaking it into d/h/m/s.
pub(crate) fn format_iso8601_duration(delta: chrono::TimeDelta, round_seconds: bool) -> String {
    // The difference is taken low-to-high at both call sites, but that does
    // not make it non-negative: chrono orders timestamps by their clock
    // fields while subtraction accounts a leap second's extra time, so
    // across 23:59:60 the "later" operand can subtract to less. Taking the
    // magnitude here restores what the formatting below relies on,
    // subsec_nanos() in 0..=999_999_999, or the sign of a sub-second
    // delta lands inside the fraction as a literal '-' (PT0.-5S).
    let delta = delta.abs();
    let mut total_seconds = delta.num_seconds();
    let mut nanos = delta.subsec_nanos();

    if round_seconds {
        // Half-up to the nearest second. Only reached with --round-seconds.
        if nanos >= 500_000_000 {
            total_seconds += 1;
        }
        nanos = 0;
    }

    let days = total_seconds / 86400;
    let hours = (total_seconds / 3600) % 24;
    let minutes = (total_seconds / 60) % 60;
    let seconds = total_seconds % 60;

    let mut result = String::from("P");
    if days != 0 {
        result.push_str(&format!("{}D", days));
    }
    if hours != 0 || minutes != 0 || seconds != 0 || nanos != 0 {
        result.push('T');
        if hours != 0 {
            result.push_str(&format!("{}H", hours));
        }
        if minutes != 0 {
            result.push_str(&format!("{}M", minutes));
        }
        if seconds != 0 || nanos != 0 {
            if nanos != 0 {
                // Zero-pad to nanosecond width, then drop trailing zeros so
                // half a second is ".5" rather than ".500000000".
                let fraction = format!("{:09}", nanos);
                result.push_str(&format!("{}.{}S", seconds, fraction.trim_end_matches('0')));
            } else {
                result.push_str(&format!("{}S", seconds));
            }
        }
    }
    if result == "P" {
        result.push_str("T0S");
    }
    result
}
