// The zone -l/--local converts into, from TZ or from the system.

use std::error::Error;

// Whether a TZ value spells POSIX rules rather than naming a zone. POSIX
// writes an abbreviation of three or more letters, or a bracketed form like
// "<+04>", followed immediately by its offset: "EST5EDT,M3.2.0,M11.1.0",
// "UTC+3", "GMT0BST,M3.5.0/1,M10.5.0". A zone name never carries a digit or a
// sign in that position, which is what separates rules from a misspelt name.
pub(crate) fn is_posix_tz_rule(value: &str) -> bool {
    let after_abbreviation = if let Some(bracketed) = value.strip_prefix('<') {
        match bracketed.find('>') {
            Some(end) => &bracketed[end + 1..],
            None => return false,
        }
    } else {
        let letters = value.chars().take_while(char::is_ascii_alphabetic).count();
        if letters < 3 {
            return false;
        }
        &value[letters..]
    };
    matches!(after_abbreviation.chars().next(), Some('+' | '-' | '0'..='9'))
}

// The offsets a POSIX rule declares, standard then daylight when it has one,
// in seconds east of UTC, or None if the rule is not shaped the way POSIX
// says. A daylight name with no offset of its own is an hour ahead. POSIX
// writes the sign the other way round from everyone else: the value is what
// must be added to local time to reach UTC, so "EST5" is UTC-5 and "IST-2"
// is UTC+2.
pub(crate) fn posix_rule_offsets(value: &str) -> Option<(i32, Option<i32>)> {
    let (standard, rest) = posix_offset(after_posix_abbreviation(value)?)?;
    if rest.is_empty() || rest.starts_with(',') {
        return Some((standard, None));
    }
    let tail = after_posix_abbreviation(rest)?;
    let daylight = match posix_offset(tail) {
        Some((offset, _)) => offset,
        None => standard + 3600,
    };
    Some((standard, Some(daylight)))
}

pub(crate) fn after_posix_abbreviation(value: &str) -> Option<&str> {
    match value.strip_prefix('<') {
        Some(bracketed) => Some(&bracketed[bracketed.find('>')? + 1..]),
        None => Some(&value[value.chars().take_while(char::is_ascii_alphabetic).count()..]),
    }
}

// The offset at the start of TAIL, in seconds east of UTC, and what follows
// it. POSIX writes offsets west of UTC, hence the negation.
pub(crate) fn posix_offset(tail: &str) -> Option<(i32, &str)> {
    let (sign, rest) = match tail.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, tail.strip_prefix('+').unwrap_or(tail)),
    };
    let length = rest.chars().take_while(|c| c.is_ascii_digit() || *c == ':').count();
    if length == 0 {
        return None;
    }
    let mut parts = rest[..length].split(':');
    let hours: i32 = parts.next()?.parse().ok()?;
    let minutes: i32 = match parts.next() {
        Some(field) => field.parse().ok()?,
        None => 0,
    };
    let seconds: i32 = match parts.next() {
        Some(field) => field.parse().ok()?,
        None => 0,
    };
    Some((-sign * (hours * 3600 + minutes * 60 + seconds), &rest[length..]))
}

// Whether chrono made sense of the POSIX rule in TZ, judged by whether it
// agrees with the rule about standard time.
//
// chrono reads the rule itself, and where it cannot, a transition time
// past 24:00, which real zones publish; Asia/Jerusalem's own footer is
// IST-2IDT,M3.4.4/26,M10.5.0, it falls back to UTC without a word. Left
// alone, `-l` would answer +00:00 for a rule naming a zone hours away and
// exit 0, the same quiet mistake the unknown-name refusal exists to stop.
//
// Two probes six months apart, because whichever of them falls in standard
// time must carry the offset the rule declares, and which one that is
// depends on the hemisphere. A rule chrono cannot read is replaced by UTC or
// by the system's own zone, whose offset can match by chance, so a rule with
// daylight saving must show both of its offsets. A zone whose standard offset
// really is UTC cannot be told apart from a rule chrono dropped, but then the
// standard half of the answer is right either way.
pub(crate) fn chrono_reads_posix_rule(standard: i32, daylight: Option<i32>) -> bool {
    use chrono::{Offset, TimeZone};
    let offsets: Vec<i32> = [1u32, 7].iter().filter_map(|month| {
        let naive = chrono::NaiveDate::from_ymd_opt(2024, *month, 15)?.and_hms_opt(12, 0, 0)?;
        Some(chrono::Local.offset_from_utc_datetime(&naive).fix().local_minus_utc())
    }).collect();
    offsets_follow_rule(&offsets, standard, daylight)
}

// Whether the OFFSETS chrono gave at the two probes are the rule's own: the
// standard offset, and the daylight one when the rule has one. Apart from
// chrono_reads_posix_rule so the decision can be tested without depending on
// the zone of the machine the tests run on, which is what chrono falls back to.
pub(crate) fn offsets_follow_rule(offsets: &[i32], standard: i32, daylight: Option<i32>) -> bool {
    offsets.contains(&standard) && daylight.is_none_or(|offset| offsets.contains(&offset))
}

// Resolves the zone -l works in, from TZ or from the platform, once at
// startup rather than per row.
//
// The zone is looked up in the IANA database bundled with the binary
// (chrono-tz) instead of read from the host's tzdata files. That keeps a
// conversion reproducible: the same input gives the same output on any
// machine, whatever tzdata the operating system happens to carry, which
// matters when the result may be relied on as evidence. It also sidesteps a
// defect in chrono's own tzdata reader, which never applies Morocco's
// daylight-saving rule and so reports Africa/Casablanca and Africa/El_Aaiun
// an hour early year-round.
//
// Ok(Some) is a zone found in the bundled database. Ok(None) means the value
// does not name a zone and is left to chrono's own reading, POSIX rules, or
// the path form glibc allows. Err refuses a value that is neither, because
// otherwise a misspelt zone would fall through to UTC and `-l` would report
// +00:00 as though it were the zone that was meant. That is the same quiet
// mistake `-o` refuses, and it is worse here, since a plausible timestamp
// carries no sign of it.
pub(crate) fn local_timezone() -> Result<Option<chrono_tz::Tz>, Box<dyn Error>> {
    let mut from_tz = true;
    let name = match std::env::var("TZ") {
        // POSIX: an empty TZ means UTC.
        Ok(value) if value.is_empty() => String::from("UTC"),
        // A leading ':' is POSIX's implementation-defined form, e.g.
        // ":America/Chicago", strip it before looking the zone up.
        Ok(value) => value.trim_start_matches(':').to_string(),
        // TZ unset: ask the platform which zone it's configured for. If it
        // will not say, chrono's own guess is all that is left.
        Err(_) => match iana_time_zone::get_timezone() {
            Ok(name) => {
                from_tz = false;
                name
            },
            Err(_) => return Ok(None),
        },
    };

    if let Ok(zone) = name.parse::<chrono_tz::Tz>() {
        return Ok(Some(zone));
    }
    // A path is glibc's other implementation-defined form, e.g.
    // TZ=:/usr/share/zoneinfo/Europe/Oslo. Left to chrono rather than
    // refused, since it is a real way to name a zone even though it is not
    // a name, but only when it names a file, since chrono answers UTC for
    // one that does not exist, and a mistyped path deserves the answer a
    // mistyped name gets.
    // Both checks below apply only where chrono reads TZ at all. Windows
    // takes its zone from the system rather than from the environment, so
    // neither form is honoured there in the first place, '--local's help
    // says as much, and there is nothing for a wrong value to spoil.
    // Checking anyway would misread the platform: chrono's answer comes
    // from the system zone, which agrees with a rule only by coincidence,
    // so every rule would be refused as unreadable rather than none.
    if name.starts_with('/') {
        if !cfg!(unix) || std::path::Path::new(&name).is_file() {
            return Ok(None);
        }
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("TZ names the file '{}', which does not exist. '--local' \
                     would otherwise have reported UTC as though it were the \
                     zone that file holds. Check the path, or set TZ to a zone \
                     name", name))));
    }
    if is_posix_tz_rule(&name) {
        // Left to chrono, which reads the rule form itself, but checked
        // first, since a rule it cannot read becomes UTC silently.
        if cfg!(unix) && posix_rule_offsets(&name).is_some_and(|(standard, daylight)| {
            !chrono_reads_posix_rule(standard, daylight)
        }) {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("TZ holds the POSIX rules '{}', which this build \
                         cannot read -- most often a transition time past \
                         24:00, which some zones do publish. '--local' would \
                         otherwise have reported UTC as though those rules had \
                         been followed. Name the zone instead, or use \
                         '--offset' for a fixed offset", name))));
        }
        return Ok(None);
    }

    // Which message depends on where the name came from. Blaming TZ for a
    // name the platform supplied would send the reader checking the spelling
    // of a variable they never set, and a system-configured zone cannot be
    // misspelt, so the likely cause there is the other one: a zone newer
    // than this build's database. Refused either way rather than answered
    // from the host's own rules, which would quietly use a different rule
    // source for this one zone than for every other.
    let message = if from_tz {
        format!("TZ is set to '{}', which is not a zone in the IANA database \
                 built into this binary (tzdata {}). '--local' would otherwise \
                 have reported UTC as though it were that zone. Check the \
                 spelling, or use a newer build if the zone is newer than \
                 this one", name, chrono_tz::IANA_TZDB_VERSION)
    } else {
        format!("this system is configured for the zone '{}', which is not in \
                 the IANA database built into this binary (tzdata {}). \
                 '--local' would otherwise have reported UTC as though it were \
                 that zone. Use a newer build if the zone is newer than this \
                 one, or set TZ to a zone this build knows",
                name, chrono_tz::IANA_TZDB_VERSION)
    };
    Err(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, message)))
}
