//! Reading and writing RFC3339 timestamps and ISO 8601 durations.

use jiff::civil::{Date, DateTime};
use jiff::tz::Offset;
use jiff::{SignedDuration, Timestamp};

/// The marker every conversion writes in place of a result it could not
/// produce. One word for all of them, which is what makes `grep -c parse_err`
/// the answer to "did anything fail", see KNOWN LIMITS.
pub(crate) const PARSE_ERR: &str = "parse_err";

/// A timestamp as a record writes it: the clock reading, the offset beside
/// it, and whether its seconds field said 60.
///
/// Kept as the record wrote it rather than as an instant, for two reasons.
/// -s/--split hands back the record's own date, time and offset, which an
/// instant has forgotten. And the range RFC3339 can write runs to
/// 9999-12-31T23:59:59.999999999 on any clock, so its instants reach into
/// year 10000 in UTC (9999-12-31T23:59:59-05:00 is one), which is further
/// than jiff's Timestamp goes. Every conversion here is therefore done by
/// moving the clock reading by a difference of offsets, which stays inside
/// the four-digit years for every value that can be written at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Stamp {
    /// The clock reading, with a leap second's :60 held as :59.
    clock: DateTime,
    /// The offset the clock reading is on.
    offset: Offset,
    /// Whether the seconds field said 60. Such a value counts as the moment
    /// one second after its :59 reading, the first instant of the next UTC
    /// day, and is written back with its :60 on whatever clock it is shown.
    leap: bool,
}

impl Stamp {
    /// The moment CLOCK reads on a clock OFFSET from UTC, a leap second when
    /// LEAP is set. CLOCK holds a leap second as its :59.
    pub(crate) fn new(clock: DateTime, offset: Offset, leap: bool) -> Stamp {
        Stamp { clock, offset, leap }
    }

    /// The moment SECONDS after 1970-01-01T00:00:00Z, on UTC's clock, or None
    /// where that falls outside the years RFC3339 can write.
    pub(crate) fn from_epoch_seconds(seconds: i64) -> Option<Stamp> {
        let clock = match Timestamp::from_second(seconds) {
            Ok(instant) => Offset::UTC.to_datetime(instant),
            Err(_) => DateTime::constant(1970, 1, 1, 0, 0, 0, 0).checked_add(SignedDuration::from_secs(seconds)).ok()?,
        };
        is_rfc3339_year(clock).then_some(Stamp::new(clock, Offset::UTC, false))
    }

    /// The same moment on a clock OFFSET from UTC, or None where that clock
    /// reads outside the years RFC3339 can write. A conversion can cross that
    /// edge without the instant moving far: 9999-12-31T23:59:59+00:00 is year
    /// 10000 on a +14:00 clock.
    pub(crate) fn on_offset(&self, offset: Offset) -> Option<Stamp> {
        let shift = i64::from(offset.seconds()) - i64::from(self.offset.seconds());
        let clock = shifted(self.clock, shift)?;
        is_rfc3339_year(clock).then_some(Stamp::new(clock, offset, self.leap))
    }

    /// The instant to ask a zone's rules about. The moment itself wherever a
    /// Timestamp can hold it, which is everywhere but the last 26 hours of
    /// 9999 in UTC and the first of 0001; there, the nearest instant a
    /// Timestamp can hold. No zone changes its offset in those hours, and a
    /// clock reading that far out is only ever written if it stays inside
    /// four digits, so the rules found at the edge are the ones in force.
    /// A leap second is asked about as its :59, which is the second it
    /// extends.
    pub(crate) fn instant_for_rules(&self) -> Timestamp {
        match self.offset.to_timestamp(self.clock) {
            Ok(instant) => instant,
            Err(_) if self.clock.year() > 0 => Timestamp::MAX,
            Err(_) => Timestamp::MIN,
        }
    }

    /// The UTC clock reading of this moment, a leap second shown as its :59,
    /// or None where that falls outside the years jiff can hold.
    pub(crate) fn utc_clock(&self) -> Option<DateTime> {
        self.clock.checked_sub(SignedDuration::from_secs(i64::from(self.offset.seconds()))).ok()
    }

    /// The time from EARLIER to this moment, negative when EARLIER is in
    /// fact the later of the two. A leap second counts as the moment after
    /// its :59 second ends, so it adds nothing to a span: elapsed civil time,
    /// not elapsed SI seconds, as KNOWN LIMITS says.
    pub(crate) fn since(&self, earlier: &Stamp) -> SignedDuration {
        let clocks = self.clock.duration_since(earlier.clock);
        let offsets = i64::from(self.offset.seconds()) - i64::from(earlier.offset.seconds());
        let leaps = i64::from(self.leap) - i64::from(earlier.leap);
        clocks - SignedDuration::from_secs(offsets - leaps)
    }

    /// The moment as RFC3339: an upper-case 'T', the fraction at three, six
    /// or nine digits, whichever first covers it, and the offset spelt out,
    /// +00:00 rather than Z.
    pub(crate) fn to_rfc3339(self) -> String {
        let mut text = String::with_capacity(35);
        self.write_date(&mut text);
        text.push('T');
        self.write_time(&mut text);
        write_offset(&mut text, self.offset);
        text
    }

    /// The moment as -s/--split writes it: the date, and the time followed by
    /// a space and the offset, both as the record carried them.
    pub(crate) fn split(self) -> (String, String) {
        let mut date = String::with_capacity(10);
        self.write_date(&mut date);
        let mut time = String::with_capacity(25);
        self.write_time(&mut time);
        time.push(' ');
        write_offset(&mut time, self.offset);
        (date, time)
    }

    /// Appends the date, YYYY-MM-DD, to TEXT.
    fn write_date(self, text: &mut String) {
        let clock = self.clock;
        push_digits(text, clock.year().unsigned_abs().into(), 4);
        text.push('-');
        push_digits(text, clock.month().unsigned_abs().into(), 2);
        text.push('-');
        push_digits(text, clock.day().unsigned_abs().into(), 2);
    }

    /// Appends the time, HH:MM:SS and any fraction, to TEXT. A leap second's
    /// seconds field is written 60, which is what the record said.
    fn write_time(self, text: &mut String) {
        let clock = self.clock;
        let second = if self.leap { 60 } else { clock.second() };
        push_digits(text, clock.hour().unsigned_abs().into(), 2);
        text.push(':');
        push_digits(text, clock.minute().unsigned_abs().into(), 2);
        text.push(':');
        push_digits(text, second.unsigned_abs().into(), 2);
        let nanos = clock.subsec_nanosecond().unsigned_abs();
        let (value, width) = match nanos {
            0 => return,
            _ if nanos.is_multiple_of(1_000_000) => (nanos / 1_000_000, 3),
            _ if nanos.is_multiple_of(1_000) => (nanos / 1_000, 6),
            _ => (nanos, 9),
        };
        text.push('.');
        push_digits(text, value, width);
    }
}

/// CLOCK moved SECONDS on, or None past the years jiff can hold.
///
/// A difference of two offsets is under two days, so this walks the date a
/// day at a time and rebuilds the time of day, which costs a fraction of
/// jiff's general addition. That runs once per row, and was most of the time
/// a conversion took.
fn shifted(clock: DateTime, seconds: i64) -> Option<DateTime> {
    if seconds == 0 {
        return Some(clock);
    }
    let of_day = i64::from(clock.hour()) * 3600 + i64::from(clock.minute()) * 60 + i64::from(clock.second()) + seconds;
    let (days, of_day) = (of_day.div_euclid(86_400), of_day.rem_euclid(86_400));
    if days.abs() > 2 {
        return clock.checked_add(SignedDuration::from_secs(seconds)).ok();
    }
    let mut date = clock.date();
    for _ in 0..days.abs() {
        date = if days > 0 { date.tomorrow() } else { date.yesterday() }.ok()?;
    }
    // Each part is in range by construction: of_day is under 86,400.
    let part = |value: i64| i8::try_from(value).ok();
    DateTime::new(
        date.year(), date.month(), date.day(),
        part(of_day / 3600)?, part(of_day / 60 % 60)?, part(of_day % 60)?,
        clock.subsec_nanosecond(),
    ).ok()
}

/// Appends OFFSET to TEXT as +HH:MM. Only an offset is_rfc3339_offset
/// accepts reaches here: a record's own offset is read in whole minutes, and
/// a converted one is checked before it is written.
fn write_offset(text: &mut String, offset: Offset) {
    let seconds = offset.seconds();
    let sign = if seconds < 0 { '-' } else { '+' };
    let magnitude = seconds.unsigned_abs();
    text.push(sign);
    push_digits(text, magnitude / 3600, 2);
    text.push(':');
    push_digits(text, magnitude / 60 % 60, 2);
}

/// Appends VALUE to TEXT in decimal, zero-padded to WIDTH digits. Written out
/// rather than left to format!, whose general machinery cost more than all
/// the rest of a conversion; VALUE never has more digits than WIDTH here.
fn push_digits(text: &mut String, value: u32, width: u32) {
    let mut divisor = 10_u32.pow(width - 1);
    while divisor > 0 {
        text.push(char::from(b'0' + (value / divisor % 10) as u8));
        divisor /= 10;
    }
}

/// Whether a clock reading's year can be written as RFC3339 at all. Its
/// date-fullyear is four digits, so 9999 is the last year there is.
///
/// The floor is 0001 rather than 0000, which is the one place this range is
/// narrower than four digits allow. There is no year zero: the Gregorian and
/// Julian calendars run 1 BC straight into AD 1, and ISO 8601 only writes 0000
/// for 1 BC by prior agreement between the parties exchanging the data. RFC3339
/// makes no such agreement, so a 0000 in a file is a value from outside the
/// calendar the rest of the column is in, far more likely a sentinel or a
/// miscounted epoch than a date anybody recorded.
///
/// The year is read from the clock the value will be written on, not from
/// its UTC year, because a conversion can carry it over the edge without
/// moving the instant: -u writes in UTC, -o on a fixed offset, -l in a named
/// zone, and each has to be asked about the calendar it will actually print.
pub(crate) fn is_rfc3339_year(clock: DateTime) -> bool {
    (1..=9999).contains(&clock.year())
}

/// Whether an offset can be written as RFC3339 at all. Its time-offset is
/// "+"/"-" HH ":" MM, whole minutes, under 24 hours, with nowhere to put a
/// seconds part.
///
/// Only -l can produce one that is not, and it does so from ordinary data. A
/// zone's offset before it adopted standard time is the local mean time of the
/// place, which is whatever fraction of an hour its longitude works out to:
/// Europe/Amsterdam +00:17:30, America/New_York -04:56:02. Nor is that only
/// ancient history, Africa/Monrovia kept -00:44:30 until January 1972, inside
/// the range of real logs. A POSIX rule in TZ can declare either kind: UTC+3:30:15
/// has seconds, and AAA-24 is a whole day east.
///
/// Rounding such an offset to the minute while the clock time keeps its
/// seconds would make the two halves of the result disagree:
/// 1971-06-01T12:00:00Z would come out as 1971-06-01T11:15:30-00:45, which
/// reads back as 12:00:30Z. A timestamp naming a different instant from the one
/// it was made from is the quiet wrong answer this tool exists to avoid, so
/// such a value is marked instead, and the input is still there beside it.
pub(crate) fn is_rfc3339_offset(offset: Offset) -> bool {
    let seconds = offset.seconds();
    seconds % 60 == 0 && seconds.abs() < 24 * 3600
}

/// Renders a converted moment as RFC3339, or the parse_err marker where the
/// conversion has taken it outside the format: None, for a clock reading past
/// the four-digit years, or an offset RFC3339 cannot write. Writing ISO 8601's
/// expanded year instead would put a value in an RFC3339 column that is not
/// RFC3339, cannot be read back, and gives no sign of the boundary it crossed.
pub(crate) fn to_rfc3339_in_range(converted: Option<Stamp>) -> String {
    match converted {
        Some(stamp) if is_rfc3339_offset(stamp.offset) => stamp.to_rfc3339(),
        _ => String::from(PARSE_ERR),
    }
}

/// Why a value was not read as a timestamp. One kind for every reason, since
/// every refusal is written the same way, as parse_err.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Refused;

impl std::fmt::Display for Refused {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("not an RFC3339 timestamp this tool reads")
    }
}

impl std::error::Error for Refused {}

/// Parses an RFC3339 timestamp, tolerating a trailing numeric offset that's
/// missing its colon (e.g. "+0000" or "-0530" instead of "+00:00"/"-05:30").
/// Such timestamps are common in data exported from tools that follow
/// RFC2822/ISO 8601's looser offset format instead of strict RFC3339. The
/// strict parse is tried first and used as-is when it succeeds, so
/// well-formed input pays no extra cost.
pub(crate) fn parse_rfc3339_lenient(value: &str) -> Result<Stamp, Refused> {
    // Surrounding whitespace is skipped here rather than trimmed out of the
    // field, so a file written with ", " separators converts without --trim
    // having to alter what is passed through. Trimming the field first would
    // reach inside quoted fields and throw away spaces the source had quoted
    // to keep.
    let value = value.trim();
    // Precision a value cannot carry is refused into parse_err rather than
    // quietly cut: this tool's promise is that what the source had is what
    // comes out, and a tenth digit has nowhere to go.
    if fractional_digits(value) > 9 {
        return Err(Refused);
    }
    let stamp = match parse_rfc3339(value) {
        Some(stamp) => stamp,
        None => insert_offset_colon(value).and_then(|fixed| parse_rfc3339(&fixed)).ok_or(Refused)?,
    };
    if stamp.leap && !leap_second_is_possible(stamp) {
        return Err(Refused);
    }
    // Year 0000 is four digits but no year of the calendar the format uses,
    // see is_rfc3339_year. Refused on the way in, so every action marks it
    // the same way rather than only those that write a timestamp: -s would
    // otherwise copy it out and -d measure from it.
    if stamp.clock.year() == 0 {
        return Err(Refused);
    }
    Ok(stamp)
}

/// Reads VALUE as RFC3339 and nothing wider, or None.
///
/// The shape is checked here, field by field, before jiff is asked whether
/// the date and time exist. A general date library reads much more than
/// RFC3339, which is a narrow profile of ISO 8601: a time without seconds
/// (T14Z), the basic format with no separators, a comma before the
/// fraction, offsets of hours alone or with seconds, +24:00, zone names in
/// brackets after the offset, years of six digits. Each of those names an
/// instant to such a reader and is not the format this tool promises, so
/// none of them is read.
///
/// What is read: four digits of year, '-', two of month, '-', two of day,
/// then 'T', 't' or a space, two digits each of hour, minute and second
/// with ':' between, an optional '.' and one to nine digits of fraction,
/// and the offset: 'Z' or 'z', or a sign and two digits each of hours and
/// minutes with ':' between, up to 23:59. The sign may be '+', '-' or the
/// Unicode minus sign U+2212, which a typeset document puts in an offset.
/// A seconds field of 60 is read as a leap second, for the caller to judge.
pub(crate) fn parse_rfc3339(value: &str) -> Option<Stamp> {
    let bytes = value.as_bytes();
    if bytes.len() < 20 {
        return None;
    }
    let number = |at: usize, width: usize| -> Option<i32> {
        bytes[at..at + width].iter().try_fold(0, |total, byte| match byte.is_ascii_digit() {
            true => Some(total * 10 + i32::from(byte - b'0')),
            false => None,
        })
    };
    let separators = [(4, b'-'), (7, b'-'), (13, b':'), (16, b':')];
    if separators.iter().any(|&(at, byte)| bytes[at] != byte) || !matches!(bytes[10], b'T' | b't' | b' ') {
        return None;
    }
    let (year, month, day) = (number(0, 4)?, number(5, 2)?, number(8, 2)?);
    let (hour, minute, second) = (number(11, 2)?, number(14, 2)?, number(17, 2)?);

    // The first 19 bytes are all ASCII, checked above, so 19 is a character
    // boundary and the slicing below cannot land inside a character.
    let mut rest = &value[19..];
    let mut nanos = 0;
    if let Some(fraction) = rest.strip_prefix('.') {
        let digits = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if !(1..=9).contains(&digits) {
            return None;
        }
        nanos = fraction[..digits].parse::<i32>().ok()? * 10_i32.pow(9 - digits as u32);
        rest = &fraction[digits..];
    }
    let offset = match rest {
        "Z" | "z" => 0,
        _ => {
            let (sign, hhmm) = match rest.strip_prefix('+') {
                Some(hhmm) => (1, hhmm),
                None => (-1, rest.strip_prefix('-').or_else(|| rest.strip_prefix('\u{2212}'))?),
            };
            let hhmm = hhmm.as_bytes();
            if hhmm.len() != 5 || hhmm[2] != b':' {
                return None;
            }
            let two = |at: usize| match (hhmm[at].is_ascii_digit(), hhmm[at + 1].is_ascii_digit()) {
                (true, true) => Some(i32::from(hhmm[at] - b'0') * 10 + i32::from(hhmm[at + 1] - b'0')),
                _ => None,
            };
            let (hours, minutes) = (two(0)?, two(3)?);
            if hours > 23 || minutes > 59 {
                return None;
            }
            sign * (hours * 3600 + minutes * 60)
        },
    };

    let leap = second == 60;
    let second = if leap { 59 } else { second };
    let clock = DateTime::new(
        i16::try_from(year).ok()?,
        i8::try_from(month).ok()?,
        i8::try_from(day).ok()?,
        i8::try_from(hour).ok()?,
        i8::try_from(minute).ok()?,
        i8::try_from(second).ok()?,
        nanos,
    ).ok()?;
    Some(Stamp::new(clock, Offset::from_seconds(offset).ok()?, leap))
}

/// Whether a value wearing :60 is somewhere a leap second can be.
///
/// A :60 on an ordinary minute is a damaged value: it is what a formatter
/// that rounds 59.5 up without carrying writes, and read, it would name the
/// same instant as the minute that follows it, so -d between 14:00:60Z and
/// 14:01:00Z would be PT0S and no conversion could tell them apart. Written
/// back out wearing :60, it would also not be RFC3339 on an ordinary minute:
/// standard readers refuse it. So it is marked, where this tool's rule is to
/// mark what it cannot answer faithfully.
///
/// A leap second is the last second of a UTC month and is simultaneous
/// everywhere, so the test is on the instant rather than on the clock face the
/// record happens to carry: 2017-01-01T05:29:60+05:30 is the real one at the
/// end of 2016 and is kept, while 2016-12-31T23:59:60+02:00 reads 23:59:60 on
/// its own clock but lands mid-evening in UTC and is not. Structural rather
/// than a table of the leap seconds IERS has declared: a table would refuse
/// every future one until the binary was rebuilt, and the format itself
/// permits them.
pub(crate) fn leap_second_is_possible(stamp: Stamp) -> bool {
    match stamp.utc_clock() {
        Some(utc) => utc.hour() == 23 && utc.minute() == 59 && is_last_day_of_month(utc.date()),
        None => false,
    }
}

/// Whether DATE is the last day of its month.
pub(crate) fn is_last_day_of_month(date: Date) -> bool {
    date.day() == date.days_in_month()
}

/// How many digits follow the first '.', which in anything RFC3339-shaped is
/// the start of the fractional seconds, no other part of the format holds a
/// dot. A value where it is something else entirely fails to parse either way.
pub(crate) fn fractional_digits(value: &str) -> usize {
    match value.find('.') {
        None => 0,
        Some(dot) => value[dot + 1..]
            .bytes()
            .take_while(|byte| byte.is_ascii_digit())
            .count(),
    }
}

/// If `value` ends in a 4-digit numeric offset with no colon (e.g. "+0000"),
/// returns it with a colon inserted (e.g. "+00:00"). Returns None for
/// anything else, including offsets that already have a colon, or a "Z"
/// suffix, so it never touches already-valid RFC3339 strings.
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

/// Formats a duration as an ISO 8601 duration string: `P[n]DT[n]H[n]M[n]S`.
/// Zero-valued components are omitted (a bare "P" with nothing after it
/// would be invalid, so an all-zero duration renders as "PT0S"). Not the
/// same as jiff's own rendering of a SignedDuration, which counts in hours
/// at most rather than breaking the span into days.
pub(crate) fn format_iso8601_duration(delta: SignedDuration, round_seconds: bool) -> String {
    // Either order of the two timestamps gives the same span: the magnitude
    // is what is written, and what the formatting below relies on,
    // subsec_nanos() in 0..=999_999_999, or the sign of a sub-second delta
    // would land inside the fraction as a literal '-' (PT0.-5S).
    let delta = delta.abs();
    let mut total_seconds = delta.as_secs();
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
