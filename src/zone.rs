//! The zone -l/--local converts into, from TZ or from the system.

use std::error::Error;
use jiff::tz::TimeZone;

/// Whether a TZ value spells POSIX rules rather than naming a zone. POSIX
/// writes an abbreviation of three or more letters, or a bracketed form like
/// "<+04>", followed immediately by its offset: "EST5EDT,M3.2.0,M11.1.0",
/// "UTC+3", "GMT0BST,M3.5.0/1,M10.5.0". A zone name never carries a digit or a
/// sign in that position, which is what separates rules from a misspelt name.
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

/// The zone NAME names in the IANA database bundled with the binary, or None.
///
/// The name must be spelt exactly as the database spells it. jiff's lookup
/// folds case, so america/new_york would find America/New_York, but zone
/// names are case-sensitive everywhere TZ is read, and a name in the wrong
/// case is a misspelling, refused like any other.
pub(crate) fn bundled_zone(name: &str) -> Option<TimeZone> {
    let zone = jiff::tz::db().get(name).ok()?;
    (zone.iana_name() == Some(name)).then_some(zone)
}

/// Resolves the zone -l works in, from TZ or from the platform, once at
/// startup rather than per row.
///
/// A zone name is looked up in the IANA database bundled with the binary
/// (jiff-tzdb) instead of read from the host's tzdata files. That keeps a
/// conversion reproducible: the same input gives the same output on any
/// machine, whatever tzdata the operating system happens to carry, which
/// matters when the result may be relied on as evidence.
///
/// TZ may also hold POSIX rules, read by jiff, or on Unix the path of a zone
/// file, read from that file. Anything else is refused, because otherwise a
/// misspelt zone would fall through to UTC and `-l` would report +00:00 as
/// though it were the zone that was meant. That is the same quiet mistake
/// `-o` refuses, and it is worse here, since a plausible timestamp carries no
/// sign of it.
pub(crate) fn local_timezone() -> Result<TimeZone, Box<dyn Error>> {
    match std::env::var("TZ") {
        // POSIX: an empty TZ means UTC.
        Ok(value) if value.is_empty() => Ok(TimeZone::UTC),
        // A leading ':' is POSIX's implementation-defined form, e.g.
        // ":America/Chicago", stripped before the zone is looked up.
        Ok(value) => zone_from_tz(value.trim_start_matches(':')),
        Err(_) => system_zone(),
    }
}

/// The zone a TZ value NAME stands for, with its leading ':' already gone.
pub(crate) fn zone_from_tz(name: &str) -> Result<TimeZone, Box<dyn Error>> {
    let refusal = |message: String| -> Box<dyn Error> {
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, message))
    };
    if let Some(zone) = bundled_zone(name) {
        return Ok(zone);
    }
    // A path is glibc's other implementation-defined form, e.g.
    // TZ=:/usr/share/zoneinfo/Europe/Oslo. Honoured rather than refused,
    // since it is a real way to name a zone even though it is not a name,
    // and read from the file it names, which is what it asks for. A
    // mistyped path deserves the answer a mistyped name gets, and so does a
    // file that holds no zone. Windows takes its zone from the system rather
    // than from a file, as '--local's help says, so there the system's zone
    // is used, as it would be with TZ unset.
    if name.starts_with('/') {
        if !cfg!(unix) {
            return system_zone();
        }
        if !std::path::Path::new(name).is_file() {
            return Err(refusal(format!(
                "TZ names the file '{}', which does not exist. '--local' would \
                 otherwise have reported UTC as though it were the zone that \
                 file holds. Check the path, or set TZ to a zone name", name)));
        }
        return zone_file(name).map_err(|error| refusal(format!(
            "TZ names the file '{}', which is not a zone file this build can \
             read ({}). Check the path, or set TZ to a zone name", name, error)));
    }
    if is_posix_tz_rule(name) {
        return TimeZone::posix(name).map_err(|error| refusal(format!(
            "TZ holds the POSIX rules '{}', which cannot be read ({}). Correct \
             the rules, name the zone instead, or use '--offset' for a fixed \
             offset", name, error)));
    }
    Err(refusal(format!(
        "TZ is set to '{}', which is not a zone in the IANA database built \
         into this binary (tzdata {}). '--local' would otherwise have reported \
         UTC as though it were that zone. Check the spelling, or use a newer \
         build if the zone is newer than this one", name, jiff_tzdb::VERSION)))
}

/// The zone the platform is configured for, when TZ is unset.
///
/// The platform is asked for the zone's name, which is then looked up in the
/// bundled database like any other. A name the database does not know is
/// refused rather than answered from the host's own rules, which would
/// quietly use a different rule source for this one zone than for every
/// other. Blaming TZ for it would send the reader checking the spelling of a
/// variable they never set, and a configured zone cannot be misspelt, so the
/// likely cause named is the other one: a zone newer than this build.
///
/// A platform that will not name its zone leaves its zone file, on Unix
/// /etc/localtime, read as it stands; with none, UTC.
pub(crate) fn system_zone() -> Result<TimeZone, Box<dyn Error>> {
    match iana_time_zone::get_timezone() {
        Ok(name) => bundled_zone(&name).ok_or_else(|| -> Box<dyn Error> {
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, format!(
                "this system is configured for the zone '{}', which is not in \
                 the IANA database built into this binary (tzdata {}). \
                 '--local' would otherwise have reported UTC as though it were \
                 that zone. Use a newer build if the zone is newer than this \
                 one, or set TZ to a zone this build knows",
                name, jiff_tzdb::VERSION)))
        }),
        Err(_) if cfg!(unix) => Ok(zone_file("/etc/localtime").unwrap_or(TimeZone::UTC)),
        Err(_) => Ok(TimeZone::UTC),
    }
}

/// The zone held in the TZif file at PATH.
pub(crate) fn zone_file(path: &str) -> Result<TimeZone, Box<dyn Error>> {
    Ok(TimeZone::tzif(path, &std::fs::read(path)?)?)
}
