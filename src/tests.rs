//! Unit tests for what the command-line tests in tests/cli.rs cannot reach.

use clap::builder::PossibleValue;
use jiff::SignedDuration;
use jiff::civil::DateTime;
use jiff::tz::Offset;
use crate::{cli::*, edit::*, input::*, report::*, timestamp::*, zone::*};

/// VALUE read as a timestamp, which it must be.
fn stamp(value: &str) -> Stamp {
    parse_rfc3339_lenient(value).unwrap()
}

/// The offset SECONDS east of UTC.
fn offset(seconds: i32) -> Offset {
    Offset::from_seconds(seconds).unwrap()
}

/// A paragraph ends at a blank line, and a blank line is not always
/// "\n\n". git hands a Windows checkout the corpus with CRLF endings,
/// where it is "\r\n\r\n" and the shorter pattern is nowhere in the
/// file, so -h would print the whole introduction there, the durations
/// paragraph included.
///
/// Asked here because it cannot be asked of the binary: this checkout has
/// one set of endings, and the question is about the other.
#[test]
fn a_paragraph_ends_at_a_blank_line_whatever_ended_the_lines() {
    assert_eq!(super::first_paragraph("One.\n\nTwo.\n"), "One.\n");
    assert_eq!(
        super::first_paragraph("One.\r\n\r\nTwo.\r\n"),
        "One.\r\n",
    );
    // No blank line at all is one paragraph, not none.
    assert_eq!(super::first_paragraph("Only.\n"), "Only.\n");
}

/// Folding a summary must not put an option name at the start of a line:
/// in the description column that reads as the start of a new entry, to
/// someone skimming and to anything parsing the output. Mending a bad
/// break after making it can bring a second option name to the line
/// start, or push a line past the column everything else was folded to.
#[test]
fn a_folded_line_never_begins_with_an_option_name() {
    let text = "The run finished, and values that did not convert are \
                counted on the standard error stream. Asking for \
                --help, --version or --list-options reports this too.";
    let folded = super::wrapped(text);
    assert!(folded.lines().count() > 1, "nothing was folded: {folded}");
    for line in folded.lines() {
        assert!(
            !line.starts_with("--"),
            "a folded line reads as an option entry: {line:?}",
        );
        assert!(
            line.chars().count() <= super::HELP_COLUMNS,
            "a folded line runs past the column: {line:?}",
        );
    }
}

/// Padding comes off the end of a line; the line ending does not.
///
/// The carriage return is the case that matters: on Windows the corpus is checked out CRLF, so a blank line inside
/// an option's long help is ten spaces and a CR. A trim that knew only
/// spaces and tabs would stop at the CR and leave all ten.
#[test]
fn padding_comes_off_the_line_and_the_ending_stays() {
    assert_eq!(super::without_trailing_padding("a   \nb\t\nc  "), "a\nb\nc");

    // CRLF: the return is kept, the spaces before it are not.
    assert_eq!(super::without_trailing_padding("a   \r\nb\r\n"), "a\r\nb\r\n");
    assert_eq!(super::without_trailing_padding("          \r\n"), "\r\n");

    // A line of nothing but spaces becomes empty, not shorter.
    assert_eq!(super::without_trailing_padding("          \n"), "\n");

    // And text that has no padding is handed back as it was, endings and
    // all, mixed ones included.
    let mixed = "one\r\ntwo\nthree\r\n";
    assert_eq!(super::without_trailing_padding(mixed), mixed);
}

use super::*;

/// The options of a run given none.
fn default_args() -> ArgumentFlags {
    ArgumentFlags {
        insert_position: InsertPosition::After,
        ..Default::default()
    }
}

/// A record, header or data, holding FIELDS.
fn record(fields: &[&str]) -> csv::StringRecord {
    csv::StringRecord::from(fields.to_vec())
}

/// The fields of RECORD, to compare against a list.
fn as_vec(record: &csv::StringRecord) -> Vec<&str> {
    record.iter().collect()
}

// ---- open_input ------------------------------------------------------

#[test]
fn a_directory_is_reported_as_one_on_either_platform() {
    // The platforms differ underneath: Unix opens a directory and fails at
    // the first read, Windows refuses the open as access denied. Neither
    // message says "directory", so the diagnosis is made on this side,
    // and this runs on both, which is the only way to know it holds.
    let directory = std::env::temp_dir();
    let error = open_input(&directory)
        .expect_err("a directory is not a readable input")
        .to_string();
    assert!(
        error.contains("it is a directory, not a file"),
        "should have said what it is, said: {error}",
    );
    assert!(
        error.contains(&directory.to_string_lossy().to_string()),
        "should have named it, said: {error}",
    );
}

// ---- abbreviated option values ---------------------------------------

/// The names of the values clap accepts.
fn names(values: &[PossibleValue]) -> Vec<String> {
    values.iter().map(|value| value.get_name().to_string()).collect()
}

#[test]
fn abbreviations_are_every_prefix_only_one_value_has() {
    // 'n' is left out: necessary, never and nonnumeric all start with it,
    // and resolving it to one of them would be a guess.
    assert_eq!(
        names(&abbreviated_values(&QUOTINGS)),
        vec![
            "always", "a", "al", "alw", "alwa", "alway",
            "necessary", "nec", "nece", "neces", "necess", "necessa", "necessar",
            "never", "nev", "neve",
            "nonnumeric", "no", "non", "nonn", "nonnu", "nonnum", "nonnume",
            "nonnumer", "nonnumeri",
        ],
    );

    // These four differ from the first letter, so every prefix belongs to
    // exactly one of them.
    assert_eq!(
        names(&abbreviated_values(&TRIMS)),
        vec![
            "all", "a", "al",
            "fields", "f", "fi", "fie", "fiel", "field",
            "headers", "h", "he", "hea", "head", "heade", "header",
            "none", "n", "no", "non",
        ],
    );
}

#[test]
fn a_value_resolves_whatever_its_case_and_however_short() {
    for given in ["all", "ALL", "All", "a", "AL"] {
        assert_eq!(resolve_value(&TRIMS, given), Some(csv::Trim::All), "{given}");
    }
    assert_eq!(resolve_value(&QUOTINGS, "NEV"), Some(Quoting::Never));
    assert_eq!(resolve_value(&QUOTINGS, "nonnumeric"), Some(Quoting::NonNumeric));
}

#[test]
fn an_ambiguous_or_unknown_value_resolves_to_nothing() {
    // Every value clap accepts resolves, so these cannot be reached
    // through the command line: the point is that if the two ever part
    // company, the result is no match rather than a wrong one.
    for given in ["n", "ne", "", "x", "alll", "fieldss"] {
        assert_eq!(resolve_value(&QUOTINGS, given), None, "{given}");
    }
}

#[test]
fn every_value_clap_accepts_also_resolves() {
    /// Guards against clap accepting a value that then matches nothing.
    /// Every accepted form of all four value lists has to resolve,
    /// so no option can gain a value it does not then act on.
    fn check<T: Copy>(choices: &Choices<T>) {
        for accepted in names(&abbreviated_values(choices)) {
            assert!(
                resolve_value(choices, &accepted).is_some(),
                "clap accepts {accepted:?}, which resolves to nothing",
            );
        }
    }
    check(&TRIMS);
    check(&QUOTINGS);
    check(&POSITIONS);
    check(&FLEXIBLE_MODES);
}

// ---- process_headers / process_record: one row per case ---------------

/// The options for a case that converts column 1 of the row, writing its
/// result at `position`.
fn converting(set: fn(&mut ArgumentFlags), position: InsertPosition) -> ArgumentFlags {
    let mut args = default_args();
    set(&mut args);
    args.selected_column = 1;
    args.insert_position = position;
    args
}

/// The default options with SET applied to them.
fn flags(set: impl FnOnce(&mut ArgumentFlags)) -> ArgumentFlags {
    let mut args = default_args();
    set(&mut args);
    args
}

/// Asks for -s.
fn split(args: &mut ArgumentFlags) {
    args.action = Action::Split;
}

/// Asks for -r.
fn rfc3339(args: &mut ArgumentFlags) {
    args.action = Action::ToRfc3339;
}

/// Asks for -u.
fn to_utc(args: &mut ArgumentFlags) {
    args.action = Action::ToUtc;
}

/// Asks for -l, in UTC.
fn local(args: &mut ArgumentFlags) {
    args.action = Action::ToLocal(jiff::tz::TimeZone::UTC);
}

/// Asks for the row-over-row -d.
fn duration(args: &mut ArgumentFlags) {
    args.action = Action::Duration { second: None };
}

/// Asks for -o to -06:00.
fn to_offset(args: &mut ArgumentFlags) {
    args.action = Action::ToOffset(offset(-6 * 3600));
}

/// One case: its label, the options, the row read and the row written, or
/// None where the run must refuse it with an error.
type Case<'a> = (&'a str, ArgumentFlags, Vec<&'a str>, Option<Vec<&'a str>>);

#[test]
fn process_headers_names_what_each_action_adds() {
    use InsertPosition::{After, Before, Replace};
    // Two-column --duration: the row-over-row column pair, written at -i.
    let paired = |position, insert_column| {
        flags(|args| {
            args.action = Action::Duration { second: None };
            args.selected_column = 1;
            args.action = Action::Duration { second: Some(2) };
            args.insert_column = insert_column;
            args.insert_position = position;
        })
    };
    let id_ts_val = vec!["id", "ts", "val"];
    let id_start_end = vec!["id", "start", "end"];
    let cases: Vec<Case> = vec![
        ("no action", default_args(), id_ts_val.clone(), Some(id_ts_val.clone())),
        (
            "split, default position is after its own column",
            flags(|args| {
                args.action = Action::Split;
                args.selected_column = 1;
            }),
            id_ts_val.clone(),
            Some(vec!["id", "ts", "<split_date", "<split_time", "val"]),
        ),
        (
            "split, -i before its own column",
            flags(|args| {
                split(args);
                args.selected_column = 1;
                args.insert_column = Some(1);
                args.insert_position = Before;
            }),
            id_ts_val.clone(),
            Some(vec!["id", "split_date>", "split_time>", "ts", "val"]),
        ),
        (
            "split, -i replace its own column",
            flags(|args| {
                split(args);
                args.selected_column = 1;
                args.insert_column = Some(1);
                args.insert_position = Replace;
            }),
            id_ts_val.clone(),
            Some(vec!["id", "split_date", "split_time", "val"]),
        ),
        (
            "split, -i after its own column",
            flags(|args| {
                split(args);
                args.selected_column = 1;
                args.insert_column = Some(1);
                args.insert_position = After;
            }),
            id_ts_val.clone(),
            Some(vec!["id", "ts", "<split_date", "<split_time", "val"]),
        ),
        (
            // -i targets an arbitrary column, decoupled from the action's
            // own column.
            "split, -i independent of its own column",
            flags(|args| {
                split(args);
                args.selected_column = 1;
                args.insert_column = Some(0);
                args.insert_position = Before;
            }),
            id_ts_val.clone(),
            Some(vec!["split_date>", "split_time>", "id", "ts", "val"]),
        ),
        ("rfc3339 before", converting(rfc3339, Before), id_ts_val.clone(),
            Some(vec!["id", "to_rfc3339>", "ts", "val"])),
        ("rfc3339 replace", converting(rfc3339, Replace), id_ts_val.clone(),
            Some(vec!["id", "to_rfc3339", "val"])),
        ("rfc3339 after", converting(rfc3339, After), id_ts_val.clone(),
            Some(vec!["id", "ts", "<to_rfc3339", "val"])),
        ("utc before", converting(to_utc, Before), id_ts_val.clone(),
            Some(vec!["id", "to_utc>", "ts", "val"])),
        ("utc replace", converting(to_utc, Replace), id_ts_val.clone(),
            Some(vec!["id", "to_utc", "val"])),
        ("utc after", converting(to_utc, After), id_ts_val.clone(),
            Some(vec!["id", "ts", "<to_utc", "val"])),
        ("local before", converting(local, Before), id_ts_val.clone(),
            Some(vec!["id", "to_local>", "ts", "val"])),
        ("local replace", converting(local, Replace), id_ts_val.clone(),
            Some(vec!["id", "to_local", "val"])),
        ("local after", converting(local, After), id_ts_val.clone(),
            Some(vec!["id", "ts", "<to_local", "val"])),
        ("offset before", converting(to_offset, Before), id_ts_val.clone(),
            Some(vec!["id", "to_offset>", "ts", "val"])),
        ("offset replace", converting(to_offset, Replace), id_ts_val.clone(),
            Some(vec!["id", "to_offset", "val"])),
        ("offset after", converting(to_offset, After), id_ts_val.clone(),
            Some(vec!["id", "ts", "<to_offset", "val"])),
        (
            "offset, column out of bounds",
            flags(|args| {
                to_offset(args);
                args.selected_column = 9;
            }),
            id_ts_val.clone(),
            None,
        ),
        ("duration before", converting(duration, Before), id_ts_val.clone(),
            Some(vec!["id", "duration>", "ts", "val"])),
        ("duration replace", converting(duration, Replace), id_ts_val.clone(),
            Some(vec!["id", "duration", "val"])),
        ("duration after", converting(duration, After), id_ts_val.clone(),
            Some(vec!["id", "ts", "<duration", "val"])),
        (
            "two-column duration defaults to the second column",
            flags(|args| {
                duration(args);
                args.selected_column = 1;
                args.action = Action::Duration { second: Some(2) };
            }),
            id_start_end.clone(),
            Some(vec!["id", "start", "end", "<row_duration"]),
        ),
        ("two-column duration, -i 2 before", paired(Before, Some(2)), id_start_end.clone(),
            Some(vec!["id", "start", "row_duration>", "end"])),
        ("two-column duration, -i 2 replace", paired(Replace, Some(2)), id_start_end.clone(),
            Some(vec!["id", "start", "row_duration"])),
        ("two-column duration, -i 2 after", paired(After, Some(2)), id_start_end.clone(),
            Some(vec!["id", "start", "end", "<row_duration"])),
        (
            "two-column duration, second column out of bounds",
            flags(|args| {
                duration(args);
                args.selected_column = 0;
                args.action = Action::Duration { second: Some(9) };
            }),
            id_start_end.clone(),
            None,
        ),
        (
            "remove",
            flags(|args| {
                args.action = Action::Remove(vec![1]);
            }),
            id_ts_val.clone(),
            Some(vec!["id", "val"]),
        ),
        (
            "arrange, named columns first and then the rest",
            flags(|args| {
                args.action = Action::Arrange { order: vec![3, 1], listed: vec![1, 3] };
            }),
            vec!["a", "b", "c", "d", "e"],
            Some(vec!["d", "b", "a", "c", "e"]),
        ),
        (
            // As --remove's parsing produces it: highest index first.
            "remove several columns",
            flags(|args| {
                args.action = Action::Remove(vec![3, 1]);
            }),
            vec!["id", "ts", "val", "note"],
            Some(vec!["id", "val"]),
        ),
        (
            "remove refuses the list if any column is out of bounds",
            flags(|args| {
                args.action = Action::Remove(vec![9, 0]);
            }),
            vec!["id", "ts"],
            None,
        ),
        (
            "remove, column out of bounds",
            flags(|args| {
                args.action = Action::Remove(vec![9]);
            }),
            id_ts_val.clone(),
            None,
        ),
        (
            // Bounds-checking by `vecbuff.len() - 1` would underflow (a
            // subtraction overflow panic) when the record has zero fields,
            // e.g. the header row on empty input. Any column
            // index must be a clean error against a zero-field record.
            "zero-field record",
            flags(|args| {
                args.action = Action::Remove(vec![0]);
            }),
            vec![],
            None,
        ),
    ];
    for (label, args, input, expected) in cases {
        let result = process_headers(&record(&input), &args);
        match expected {
            Some(expected) => {
                let written = result.unwrap_or_else(|error| panic!("{label}: {error}"));
                assert_eq!(as_vec(&written), expected, "{label}");
            },
            None => assert!(result.is_err(), "{label}: expected an error"),
        }
    }
}

/// A row as written, and the timestamp carried into the next.
type ProcessedRecord = (csv::StringRecord, Option<Stamp>);

/// Convenience wrapper for the cases below, which assert on a whole
/// StringRecord. The streaming path writes the borrowed field list
/// straight out instead.
fn process_record(
    record: &csv::StringRecord,
    args: &ArgumentFlags,
    dt_cache: Option<Stamp>,
) -> Result<ProcessedRecord, Box<dyn Error>> {
    let (edit, cache) = plan_row_edit(record, args, dt_cache)?;
    let mut fields: Vec<&str> = Vec::with_capacity(record.len() + 2);
    apply_row_edit(record, args, &edit, &mut fields);
    Ok((csv::StringRecord::from(fields), cache))
}

/// No row-over-row cache goes in, and none comes out, for any case here.
#[test]
fn process_record_writes_what_each_action_makes_of_a_row() {
    use InsertPosition::{After, Before, Replace};
    let huge = i64::MAX.to_string();
    let cases: Vec<Case> = vec![
        (
            "arrange keeps the width and puts the named columns first",
            flags(|args| {
                args.action = Action::Arrange { order: vec![2, 0], listed: vec![0, 2] };
            }),
            vec!["a", "b", "c", "d"],
            Some(vec!["c", "a", "b", "d"]),
        ),
        (
            "arrange, column out of bounds",
            flags(|args| {
                args.action = Action::Arrange { order: vec![9], listed: vec![9] };
            }),
            vec!["a", "b"],
            None,
        ),
        (
            "remove, column out of bounds",
            flags(|args| {
                args.action = Action::Remove(vec![9]);
            }),
            vec!["1", "2", "3"],
            None,
        ),
        (
            "remove at the last valid index",
            flags(|args| {
                args.action = Action::Remove(vec![2]);
            }),
            vec!["1", "2", "3"],
            Some(vec!["1", "2"]),
        ),
        (
            "zero-field record",
            flags(|args| {
                args.action = Action::Remove(vec![0]);
            }),
            vec![],
            None,
        ),
        (
            "split, default position",
            flags(|args| {
                split(args);
                args.selected_column = 1;
            }),
            vec!["1", "2024-01-15T10:30:00Z", "x"],
            Some(vec!["1", "2024-01-15T10:30:00Z", "2024-01-15", "10:30:00 +00:00", "x"]),
        ),
        (
            // 00:30+05:00 and 19:30Z the day before are the same instant,
            // but split must report the date and time as given in the
            // input's own offset: it does not convert to UTC first.
            "split reports the input's own offset",
            converting(split, Replace),
            vec!["1", "2024-01-01T00:30:00+05:00", "x"],
            Some(vec!["1", "2024-01-01", "00:30:00 +05:00", "x"]),
        ),
        (
            "split accepts an offset missing its colon",
            flags(|args| {
                split(args);
                args.selected_column = 1;
            }),
            vec!["1", "2024-01-15T10:30:00+0000", "x"],
            Some(vec!["1", "2024-01-15T10:30:00+0000", "2024-01-15", "10:30:00 +00:00", "x"]),
        ),
        // A data row's parse-error marker is the plain token in every
        // position; the '>' and '<' decorations belong to the header.
        (
            "split, invalid timestamp before",
            converting(split, Before),
            vec!["1", "not-a-date", "x"],
            Some(vec!["1", "parse_err", "parse_err", "not-a-date", "x"]),
        ),
        (
            "split, invalid timestamp replace",
            converting(split, Replace),
            vec!["1", "not-a-date", "x"],
            Some(vec!["1", "parse_err", "parse_err", "x"]),
        ),
        (
            "split, invalid timestamp after",
            converting(split, After),
            vec!["1", "not-a-date", "x"],
            Some(vec!["1", "not-a-date", "parse_err", "parse_err", "x"]),
        ),
        (
            "rfc3339, valid epoch",
            converting(rfc3339, Replace),
            vec!["1", "1700000000", "x"],
            Some(vec!["1", "2023-11-14T22:13:20+00:00", "x"]),
        ),
        (
            "rfc3339, non-numeric value",
            converting(rfc3339, Replace),
            vec!["1", "abc", "x"],
            Some(vec!["1", "parse_err", "x"]),
        ),
        (
            "rfc3339, epoch past the four-digit years",
            converting(rfc3339, Replace),
            vec!["1", &huge, "x"],
            Some(vec!["1", "parse_err", "x"]),
        ),
        (
            "utc converts the offset",
            converting(to_utc, Replace),
            vec!["1", "2024-06-01T10:00:00+05:00", "x"],
            Some(vec!["1", "2024-06-01T05:00:00+00:00", "x"]),
        ),
        (
            "utc accepts an offset missing its colon",
            converting(to_utc, Replace),
            vec!["1", "2024-06-01T10:00:00+0500", "x"],
            Some(vec!["1", "2024-06-01T05:00:00+00:00", "x"]),
        ),
        (
            // 01:00+05:00 is the previous UTC day: a pure offset shift,
            // with no zone or daylight-saving logic, but the date
            // rollover arithmetic still has to be right.
            "utc shifts the calendar date across midnight",
            converting(to_utc, Replace),
            vec!["1", "2024-01-01T01:00:00+05:00", "x"],
            Some(vec!["1", "2023-12-31T20:00:00+00:00", "x"]),
        ),
        (
            "utc, invalid timestamp",
            converting(to_utc, Replace),
            vec!["1", "nope", "x"],
            Some(vec!["1", "parse_err", "x"]),
        ),
        (
            "offset converts to the explicit target",
            converting(to_offset, Replace),
            vec!["1", "2024-06-15T14:00:00+02:00", "x"],
            Some(vec!["1", "2024-06-15T06:00:00-06:00", "x"]),
        ),
        (
            "offset accepts an offset missing its colon",
            converting(to_offset, Replace),
            vec!["1", "2024-06-15T14:00:00+0200", "x"],
            Some(vec!["1", "2024-06-15T06:00:00-06:00", "x"]),
        ),
        (
            // 00:30+02:00 on Jan 1 shifts back across both midnight and
            // the year boundary once re-expressed at -06:00.
            "offset shifts the calendar date and year across midnight",
            converting(to_offset, Replace),
            vec!["1", "2024-01-01T00:30:00+02:00", "x"],
            Some(vec!["1", "2023-12-31T16:30:00-06:00", "x"]),
        ),
        (
            "offset, invalid timestamp",
            converting(to_offset, Replace),
            vec!["1", "nope", "x"],
            Some(vec!["1", "parse_err", "x"]),
        ),
        (
            "offset, -i independent of its own column",
            flags(|args| {
                to_offset(args);
                args.selected_column = 1;
                args.insert_column = Some(0);
                args.insert_position = Before;
            }),
            vec!["1", "2024-06-15T14:00:00+02:00", "x"],
            Some(vec!["2024-06-15T06:00:00-06:00", "1", "2024-06-15T14:00:00+02:00", "x"]),
        ),
        (
            // -i can target a column that is neither of --duration's own
            // two columns.
            "two-column duration, -i independent of its columns",
            flags(|args| {
                args.action = Action::Duration { second: None };
                args.selected_column = 0;
                args.action = Action::Duration { second: Some(2) };
                args.insert_column = Some(1);
                args.insert_position = Replace;
            }),
            vec!["2024-01-01T00:00:00Z", "untouched", "2024-01-02T12:00:00Z"],
            Some(vec!["2024-01-01T00:00:00Z", "P1DT12H", "2024-01-02T12:00:00Z"]),
        ),
        (
            // Highest index first, so dropping one cannot shift another
            // still to come: columns 0 and 1 of the input, not 0 and then
            // whatever moved up.
            "remove several columns by input position",
            flags(|args| {
                args.action = Action::Remove(vec![1, 0]);
            }),
            vec!["a", "b", "c", "d"],
            Some(vec!["c", "d"]),
        ),
        (
            "remove drops the named column",
            flags(|args| {
                args.action = Action::Remove(vec![1]);
            }),
            vec!["1", "2", "3"],
            Some(vec!["1", "3"]),
        ),
        ("no action leaves the row alone", default_args(), vec!["1", "2", "3"],
            Some(vec!["1", "2", "3"])),
    ];
    for (label, args, input, expected) in cases {
        let result = process_record(&record(&input), &args, None);
        match expected {
            Some(expected) => {
                let (written, cache) = result.unwrap_or_else(|error| panic!("{label}: {error}"));
                assert_eq!(as_vec(&written), expected, "{label}");
                assert_eq!(cache, None, "{label}: the cache must stay empty");
            },
            None => assert!(result.is_err(), "{label}: expected an error"),
        }
    }
}

// ---- insert_offset_colon / parse_rfc3339_lenient -----------------------

#[test]
fn insert_offset_colon_fixes_positive_and_negative_offsets() {
    assert_eq!(
        insert_offset_colon("2024-01-15T10:30:00+0000"),
        Some("2024-01-15T10:30:00+00:00".to_string())
    );
    assert_eq!(
        insert_offset_colon("2024-06-01T10:00:00-0530"),
        Some("2024-06-01T10:00:00-05:30".to_string())
    );
}

#[test]
fn insert_offset_colon_leaves_already_valid_forms_alone() {
    assert_eq!(insert_offset_colon("2024-01-15T10:30:00+00:00"), None);
    assert_eq!(insert_offset_colon("2024-01-15T10:30:00Z"), None);
    assert_eq!(insert_offset_colon("not-a-date"), None);
    assert_eq!(insert_offset_colon(""), None);
}

#[test]
fn parse_rfc3339_lenient_accepts_missing_colon_offset() {
    assert_eq!(
        parse_rfc3339_lenient("2024-01-15T10:30:00+0000").unwrap(),
        stamp("2024-01-15T10:30:00+00:00")
    );
}

#[test]
fn parse_rfc3339_lenient_still_rejects_garbage() {
    assert!(parse_rfc3339_lenient("garbage+0000").is_err());
}

#[test]
fn insert_offset_colon_survives_a_value_ending_inside_a_character() {
    // It measures back five bytes from the end and reads the byte there,
    // which lands inside a character whenever the value ends in a
    // multi-byte one. Slicing there would panic, and the only thing
    // preventing it is that the byte has to be an ASCII '+' or '-' first.
    // Timestamp columns hold whatever the file holds, so these arrive.
    for value in [
        "2024-01-15T10:30:00+00\u{e9}0",
        "\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}",
        "\u{65e5}\u{672c}\u{8a9e}\u{30c6}\u{30b9}",
        "\u{2014}\u{2014}\u{2014}\u{2014}\u{2014}",
        "\u{e9}\u{e9}\u{e9}\u{e9}-1234",
        "x\u{e9}-1234",
    ] {
        // Calling it at all is the point: the slicing panics or it does
        // not. What comes back may well be Some, the last five bytes of
        // "eeee-1234" are an ASCII sign and four digits whatever precedes
        // them, and that is harmless, because what it builds still has
        // to parse as a timestamp, and does not.
        let _ = insert_offset_colon(value);
        assert!(parse_rfc3339_lenient(value).is_err(), "value={value:?}");
    }

    // And the form it exists for still works.
    assert_eq!(
        insert_offset_colon("2024-01-15T10:30:00+0000"),
        Some("2024-01-15T10:30:00+00:00".to_string()),
    );
}

// ---- parse_position_word ------------------------------------------------

#[test]
fn parse_position_word_accepts_full_words_and_abbreviations() {
    for word in ["before", "b", "replace", "r", "after", "a", "AFTER"] {
        assert!(parse_position_word(word).is_ok(), "word={word}");
    }
}

#[test]
fn parse_position_word_rejects_garbage() {
    assert!(parse_position_word("sideways").is_err());
}

// ---- process_record: to_local ------------------------------------------

#[test]
fn record_to_local_produces_the_same_instant_as_utc() {
    let mut args = default_args();
    args.action = Action::ToLocal(bundled_zone("Asia/Kolkata").unwrap());
    args.selected_column = 1;
    args.insert_position = InsertPosition::Replace;
    let cache: Option<Stamp> = None;
    let (result, _) = process_record(
        &record(&["1", "2024-06-01T10:00:00+05:00", "x"]),
        &args,
        cache,
    )
    .unwrap();
    let produced = as_vec(&result)[1].to_string();
    let parsed = stamp(&produced);
    let original = stamp("2024-06-01T10:00:00+05:00");
    assert_eq!(parsed.since(&original), SignedDuration::ZERO, "local conversion must preserve the instant");
}

#[test]
fn record_to_local_accepts_offset_missing_a_colon() {
    let mut args = default_args();
    args.action = Action::ToLocal(bundled_zone("Asia/Kolkata").unwrap());
    args.selected_column = 1;
    args.insert_position = InsertPosition::Replace;
    let cache: Option<Stamp> = None;
    let (result, _) = process_record(
        &record(&["1", "2024-06-01T10:00:00+0500", "x"]),
        &args,
        cache,
    )
    .unwrap();
    let produced = as_vec(&result)[1].to_string();
    let parsed = stamp(&produced);
    let original = stamp("2024-06-01T10:00:00+05:00");
    assert_eq!(parsed.since(&original), SignedDuration::ZERO, "local conversion must preserve the instant");
}

// ---- parse_offset -------------------------------------------------------

#[test]
fn parse_offset_accepts_colon_no_colon_and_z() {
    let texas = offset(-6 * 3600);
    assert_eq!(parse_offset("-06:00").unwrap(), texas);
    assert_eq!(parse_offset("-0600").unwrap(), texas);
    assert_eq!(parse_offset("z").unwrap(), Offset::UTC);
    assert_eq!(parse_offset("Z").unwrap(), Offset::UTC);
    assert_eq!(
        parse_offset("+0200").unwrap(),
        offset(2 * 3600)
    );
}

#[test]
fn parse_offset_rejects_garbage() {
    assert!(parse_offset("banana").is_err());
    assert!(parse_offset("").is_err());
}

/// An offset is taken by its shape alone. Glued onto a timestamp and read
/// with it, a fraction of a second or a trailing blank passed as part of
/// the timestamp and left the offset behind it accepted.
#[test]
fn parse_offset_takes_nothing_but_an_offset() {
    for given in [
        ".5Z", ".123+02:00", "+01:00 ", " +01:00", "+1:00", "+01:0", "+01:60", "+24:00",
        "+01::00", "01:00", "ZZ", "+0é:00", "+01:00:00",
    ] {
        assert!(parse_offset(given).is_err(), "{given:?} was accepted");
    }
    assert_eq!(parse_offset("-00:00").unwrap(), Offset::UTC);
    assert_eq!(parse_offset("+05:45").unwrap(), offset(5 * 3600 + 45 * 60));
}

// ---- format_iso8601_duration ---------------------------------------------

#[test]
fn format_iso8601_duration_omits_zero_components() {
    assert_eq!(format_iso8601_duration(SignedDuration::ZERO, false), "PT0S");
    assert_eq!(format_iso8601_duration(SignedDuration::from_secs(5), false), "PT5S");
    assert_eq!(
        format_iso8601_duration(SignedDuration::from_mins(1) + SignedDuration::from_secs(30), false),
        "PT1M30S"
    );
    assert_eq!(format_iso8601_duration(SignedDuration::from_hours(1), false), "PT1H");
    assert_eq!(
        format_iso8601_duration(SignedDuration::from_hours(24), false),
        "P1D",
        "a whole day with no time component must omit the T entirely"
    );
    assert_eq!(
        format_iso8601_duration(
            SignedDuration::from_hours(24) + SignedDuration::from_hours(6) + SignedDuration::from_mins(30)
        , false),
        "P1DT6H30M"
    );
    assert_eq!(
        format_iso8601_duration(SignedDuration::from_hours(3653 * 24), false),
        "P3653D",
        "large day counts stay a plain day component, not years/months"
    );
}

#[test]
fn format_iso8601_duration_keeps_sub_second_precision() {
    assert_eq!(
        format_iso8601_duration(SignedDuration::from_millis(999), false),
        "PT0.999S"
    );
    assert_eq!(
        format_iso8601_duration(SignedDuration::from_millis(1500), false),
        "PT1.5S"
    );
    // Trailing zeros are trimmed, so half a second is ".5" not
    // ".500000000".
    assert_eq!(
        format_iso8601_duration(SignedDuration::from_nanos(1), false),
        "PT0.000000001S"
    );
    assert_eq!(
        format_iso8601_duration(
            SignedDuration::from_hours(24) + SignedDuration::from_hours(3) + SignedDuration::from_millis(250),
            false
        ),
        "P1DT3H0.25S"
    );
}

#[test]
fn format_iso8601_duration_round_seconds_is_half_up_and_carries() {
    assert_eq!(format_iso8601_duration(SignedDuration::from_millis(400), true), "PT0S");
    assert_eq!(format_iso8601_duration(SignedDuration::from_millis(500), true), "PT1S");
    assert_eq!(format_iso8601_duration(SignedDuration::from_millis(999), true), "PT1S");
    // Rounding up must carry into the larger units, not produce "PT60S".
    assert_eq!(
        format_iso8601_duration(SignedDuration::from_millis(59_600), true),
        "PT1M"
    );
    assert_eq!(
        format_iso8601_duration(
            SignedDuration::from_hours(23) + SignedDuration::from_mins(59) + SignedDuration::from_millis(59_800),
            true
        ),
        "P1D"
    );
}

#[test]
fn format_iso8601_duration_whole_seconds_unaffected_by_rounding() {
    for round in [false, true] {
        assert_eq!(format_iso8601_duration(SignedDuration::ZERO, round), "PT0S");
        assert_eq!(
            format_iso8601_duration(SignedDuration::from_hours(24) + SignedDuration::from_hours(12), round),
            "P1DT12H"
        );
    }
}

// ---- process_record: duration (single column) --------------------------

#[test]
fn record_duration_first_row_is_zero_and_seeds_cache() {
    let mut args = default_args();
    args.action = Action::Duration { second: None };
    args.selected_column = 1;
    args.insert_position = InsertPosition::Replace;
    let cache: Option<Stamp> = None;
    let (result, new_cache) = process_record(
        &record(&["1", "2024-01-01T00:00:00Z"]),
        &args,
        cache,
    )
    .unwrap();
    assert_eq!(as_vec(&result), vec!["1", "PT0S"]);
    assert_eq!(
        new_cache,
        Some(stamp("2024-01-01T00:00:00Z"))
    );
}

#[test]
fn record_duration_accepts_offset_missing_a_colon() {
    let mut args = default_args();
    args.action = Action::Duration { second: None };
    args.selected_column = 1;
    args.insert_position = InsertPosition::Replace;
    let cache: Option<Stamp> = None;
    let (result, new_cache) = process_record(
        &record(&["1", "2024-01-01T00:00:00+0000"]),
        &args,
        cache,
    )
    .unwrap();
    assert_eq!(as_vec(&result), vec!["1", "PT0S"]);
    assert_eq!(
        new_cache,
        Some(stamp("2024-01-01T00:00:00+00:00"))
    );
}

#[test]
fn record_duration_accumulates_row_over_row_even_when_unsorted() {
    let mut args = default_args();
    args.action = Action::Duration { second: None };
    args.selected_column = 1;
    args.insert_position = InsertPosition::Replace;

    let mut cache: Option<Stamp> = None;

    let (r1, c1) =
        process_record(&record(&["1", "2024-01-01T00:00:00Z"]), &args, cache).unwrap();
    assert_eq!(as_vec(&r1), vec!["1", "PT0S"]);
    cache = c1;

    let (r2, c2) =
        process_record(&record(&["2", "2024-01-02T00:00:00Z"]), &args, cache).unwrap();
    assert_eq!(as_vec(&r2), vec!["2", "P1D"]);
    cache = c2;

    // Out-of-order timestamp: duration is the absolute difference from
    // the previous row, per the documented "sort first" caveat.
    let (r3, c3) =
        process_record(&record(&["3", "2024-01-01T12:00:00Z"]), &args, cache).unwrap();
    assert_eq!(as_vec(&r3), vec!["3", "PT12H"]);
    assert_eq!(
        c3,
        Some(stamp("2024-01-01T12:00:00Z"))
    );
}

#[test]
fn record_duration_correct_across_a_dst_spring_forward() {
    // US clocks jump 02:00 -> 03:00 on 2024-03-10, so these timestamps'
    // wall-clock difference is 2h but the real elapsed time is 1h,
    // duration must reflect the instant, not naive wall-clock
    // subtraction, or this would come out as 0d2h0m0s instead.
    let mut args = default_args();
    args.action = Action::Duration { second: None };
    args.selected_column = 1;
    args.insert_position = InsertPosition::Replace;
    let cache: Option<Stamp> = None;

    let (r1, c1) = process_record(
        &record(&["1", "2024-03-10T01:30:00-05:00"]),
        &args,
        cache,
    )
    .unwrap();
    assert_eq!(as_vec(&r1), vec!["1", "PT0S"]);

    let (r2, _) = process_record(
        &record(&["2", "2024-03-10T03:30:00-04:00"]),
        &args,
        c1,
    )
    .unwrap();
    assert_eq!(as_vec(&r2), vec!["2", "PT1H"]);
}

#[test]
fn record_duration_correct_across_a_dst_fall_back() {
    // US clocks fall back 02:00 -> 01:00 on 2024-11-03, so these
    // timestamps' wall-clock difference is 1h but the real elapsed
    // time is 2h.
    let mut args = default_args();
    args.action = Action::Duration { second: None };
    args.selected_column = 1;
    args.insert_position = InsertPosition::Replace;
    let cache: Option<Stamp> = None;

    let (r1, c1) = process_record(
        &record(&["1", "2024-11-03T00:30:00-04:00"]),
        &args,
        cache,
    )
    .unwrap();
    assert_eq!(as_vec(&r1), vec!["1", "PT0S"]);

    let (r2, _) = process_record(
        &record(&["2", "2024-11-03T01:30:00-05:00"]),
        &args,
        c1,
    )
    .unwrap();
    assert_eq!(as_vec(&r2), vec!["2", "PT2H"]);
}

#[test]
fn record_duration_parse_error_leaves_cache_untouched() {
    let mut args = default_args();
    args.action = Action::Duration { second: None };
    args.selected_column = 1;
    args.insert_position = InsertPosition::Replace;
    let seed = stamp("2024-01-01T00:00:00Z");

    let (result, new_cache) =
        process_record(&record(&["1", "not-a-date"]), &args, Some(seed)).unwrap();
    assert_eq!(as_vec(&result), vec!["1", "parse_err"]);
    assert_eq!(new_cache, Some(seed), "cache must not advance on a parse error");
}

// ---- process_record: duration (two columns / same-row) -----------------

#[test]
fn record_duration_two_column_computes_absolute_difference_regardless_of_order() {
    let mut args = default_args();
    args.action = Action::Duration { second: None };
    args.selected_column = 0;
    args.action = Action::Duration { second: Some(4) };
    args.insert_position = InsertPosition::Replace;
    let cache: Option<Stamp> = None;

    // Column 0 is earlier than column 4; the result replaces column 4.
    let (result, _) = process_record(
        &record(&["2024-01-01T00:00:00Z", "a", "b", "c", "2024-01-02T12:00:00Z"]),
        &args,
        cache,
    )
    .unwrap();
    assert_eq!(as_vec(&result), vec!["2024-01-01T00:00:00Z", "a", "b", "c", "P1DT12H"]);

    // column 0 later than column 4, same absolute duration
    let (result, _) = process_record(
        &record(&["2024-01-02T12:00:00Z", "a", "b", "c", "2024-01-01T00:00:00Z"]),
        &args,
        cache,
    )
    .unwrap();
    assert_eq!(as_vec(&result), vec!["2024-01-02T12:00:00Z", "a", "b", "c", "P1DT12H"]);
}

#[test]
fn record_duration_two_column_identical_timestamps_is_zero() {
    let mut args = default_args();
    args.action = Action::Duration { second: None };
    args.selected_column = 0;
    args.action = Action::Duration { second: Some(1) };
    args.insert_position = InsertPosition::Replace;
    let cache: Option<Stamp> = None;
    let (result, _) = process_record(
        &record(&["2024-01-01T00:00:00Z", "2024-01-01T00:00:00Z"]),
        &args,
        cache,
    )
    .unwrap();
    assert_eq!(as_vec(&result), vec!["2024-01-01T00:00:00Z", "PT0S"]);
}

#[test]
fn record_duration_two_column_accepts_offset_missing_a_colon() {
    let mut args = default_args();
    args.action = Action::Duration { second: None };
    args.selected_column = 0;
    args.action = Action::Duration { second: Some(1) };
    args.insert_position = InsertPosition::Replace;
    let cache: Option<Stamp> = None;
    let (result, _) = process_record(
        &record(&["2024-01-01T00:00:00+0000", "2024-01-02T00:00:00+0000"]),
        &args,
        cache,
    )
    .unwrap();
    assert_eq!(as_vec(&result), vec!["2024-01-01T00:00:00+0000", "P1D"]);
}

#[test]
fn record_duration_two_column_either_column_invalid_is_parse_err() {
    let mut args = default_args();
    args.action = Action::Duration { second: None };
    args.selected_column = 0;
    args.action = Action::Duration { second: Some(1) };
    args.insert_position = InsertPosition::Replace;
    let cache: Option<Stamp> = None;

    let (result, _) = process_record(
        &record(&["not-a-date", "2024-01-01T00:00:00Z"]),
        &args,
        cache,
    )
    .unwrap();
    assert_eq!(as_vec(&result), vec!["not-a-date", "parse_err"]);

    let (result, _) = process_record(
        &record(&["2024-01-01T00:00:00Z", "not-a-date"]),
        &args,
        cache,
    )
    .unwrap();
    assert_eq!(as_vec(&result), vec!["2024-01-01T00:00:00Z", "parse_err"]);
}

#[test]
fn record_duration_two_column_correct_between_two_countries_own_dst_offsets() {
    // US Eastern Daylight Time (-04:00) vs Central European Summer Time
    // (+02:00) on the same real day, each country's own DST-adjusted
    // offset, not a shared reference offset. 12:00 EDT = 16:00 UTC;
    // 20:00 CEST = 18:00 UTC; real difference is 2h.
    let mut args = default_args();
    args.action = Action::Duration { second: None };
    args.selected_column = 0;
    args.action = Action::Duration { second: Some(1) };
    args.insert_position = InsertPosition::Replace;
    let cache: Option<Stamp> = None;
    let (result, _) = process_record(
        &record(&["2024-07-04T12:00:00-04:00", "2024-07-04T20:00:00+02:00"]),
        &args,
        cache,
    )
    .unwrap();
    assert_eq!(
        as_vec(&result),
        vec!["2024-07-04T12:00:00-04:00", "PT2H"]
    );
}

#[test]
fn record_duration_two_column_does_not_touch_the_row_over_row_cache() {
    let mut args = default_args();
    args.action = Action::Duration { second: None };
    args.selected_column = 0;
    args.action = Action::Duration { second: Some(1) };
    args.insert_position = InsertPosition::Replace;
    let seed = stamp("2020-01-01T00:00:00Z");
    let (_, new_cache) = process_record(
        &record(&["2024-01-01T00:00:00Z", "2024-01-02T00:00:00Z"]),
        &args,
        Some(seed),
    )
    .unwrap();
    assert_eq!(new_cache, Some(seed), "two-column duration is independent of the dt_cache");
}

#[test]
fn record_duration_two_column_out_of_bound_second_column_is_a_clean_error() {
    let mut args = default_args();
    args.action = Action::Duration { second: None };
    args.selected_column = 0;
    args.action = Action::Duration { second: Some(9) };
    let cache: Option<Stamp> = None;
    let result = process_record(&record(&["1", "2"]), &args, cache);
    assert!(result.is_err());
}

// ---- is_rfc3339_year / is_rfc3339_offset / to_rfc3339_in_range ---------
//
// The two guards deciding whether a converted moment can be written at
// all. Reached through -u/-l/-o above, but tested here directly as well:
// what they answer at the boundary is the whole of their job, and going
// through a conversion to ask makes the boundary harder to place.

/// The clock reading at the start of the hour given.
fn clock_at(year: i16, month: i8, day: i8, hour: i8) -> DateTime {
    DateTime::new(year, month, day, hour, 0, 0, 0).unwrap()
}

#[test]
fn rfc3339_year_spans_0001_to_9999_and_no_further() {
    assert!(is_rfc3339_year(clock_at(2024, 6, 15, 12)));
    // Both ends of the range are inside it.
    assert!(is_rfc3339_year(clock_at(1, 1, 1, 0)));
    assert!(is_rfc3339_year(clock_at(9999, 12, 31, 23)));
    // And the first year below is not. The floor is one year short of four
    // digits, because there is no year zero for a record to have been
    // written in.
    assert!(!is_rfc3339_year(clock_at(0, 12, 31, 23)));
    assert!(!is_rfc3339_year(clock_at(-1, 12, 31, 23)));
    // The ceiling is where four digits run out, which no clock reading can
    // pass: moving one there gives nothing at all.
    let last = stamp("9999-12-31T23:00:00Z");
    assert!(last.on_offset(offset(3600)).is_none());
    assert!(last.on_offset(offset(0)).is_some());
}

#[test]
fn rfc3339_offset_is_whole_minutes_only() {
    for seconds in [0, 60, 30 * 60, 5 * 3600 + 45 * 60, -12 * 3600, 14 * 3600, 23 * 3600 + 59 * 60] {
        assert!(is_rfc3339_offset(offset(seconds)), "{seconds}s is whole minutes");
    }
    // Local mean time, which is what a zone used before standard time and
    // what RFC3339's +HH:MM has nowhere to put. Amsterdam's +00:17:30,
    // New York's -04:56:02, Monrovia's -00:44:30.
    for seconds in [17 * 60 + 30, -(4 * 3600 + 56 * 60 + 2), -(44 * 60 + 30)] {
        assert!(!is_rfc3339_offset(offset(seconds)), "{seconds}s is not whole minutes");
    }
    // Nor a day or more, which a POSIX rule can declare and HH cannot hold.
    for seconds in [24 * 3600, -24 * 3600, 25 * 3600] {
        assert!(!is_rfc3339_offset(offset(seconds)), "{seconds}s is a day or more");
    }
}

#[test]
fn to_rfc3339_in_range_marks_what_it_cannot_write() {
    assert_eq!(
        to_rfc3339_in_range(Some(stamp("2024-06-15T12:00:00Z"))),
        "2024-06-15T12:00:00+00:00"
    );
    // Past the four-digit year, the conversion gives nothing to write.
    assert_eq!(to_rfc3339_in_range(stamp("9999-12-31T23:00:00Z").on_offset(offset(3600))), PARSE_ERR);
}

#[test]
fn to_rfc3339_in_range_refuses_a_sub_minute_offset_rather_than_moving_the_instant() {
    // Africa/Monrovia's -00:44:30, which it kept until January 1972.
    // Rounding the offset to -00:45 while the clock time keeps its seconds
    // would name an instant 30 seconds from the one it was made from.
    let utc = stamp("1971-06-01T12:00:00Z");
    let moment = utc.on_offset(offset(-(44 * 60 + 30)));
    assert_eq!(moment.map(|moment| moment.since(&utc)), Some(SignedDuration::ZERO));
    assert_eq!(to_rfc3339_in_range(moment), PARSE_ERR);
}

// ---- row_wider_than_reference -------------------------------------------

#[test]
fn a_row_too_wide_is_explained_by_the_mode_that_hit_it() {
    // 'fix' takes its width from the first record, so a wider row later is
    // ordinary, the documented limit of reading once.
    let fix = row_wider_than_reference(4, 2, FillMode::FirstRecord).to_string();
    assert!(fix.contains("--flexible=fix"), "said: {fix}");
    assert!(fix.contains("width of the first record"), "said: {fix}");

    // 'widest' takes its width from a pass over the whole input, so no row
    // can be wider than it. Getting here means the file is not the one
    // that was counted, so the message names widest and the changed
    // input, not the first record.
    let widest = row_wider_than_reference(4, 2, FillMode::Widest).to_string();
    assert!(widest.contains("--flexible=widest"), "said: {widest}");
    assert!(widest.contains("changed between the two reads"), "said: {widest}");
    assert!(!widest.contains("width of the first record"), "said: {widest}");

    // Both name the two numbers, which are what locate the row.
    for message in [&fix, &widest] {
        assert!(message.contains('2') && message.contains('4'),
            "should give both widths, said: {message}");
    }
}

#[test]
fn a_header_wider_than_the_widest_count_gets_the_widest_message() {
    // The header is read by both of widest's passes, so it too can only
    // be wider than the counted width if the input changed between them.
    // Unchecked, that would fall through to the writer's bare
    // record-length error while a data row in the same state gets the
    // tailored one.
    let mut args = default_args();
    args.has_headers = true;
    args.flexible_record = true;
    args.fill_mode = FillMode::Widest;
    let reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(&b"a,b,c\n1,2\n"[..]);
    let mut writer = csv::Writer::from_writer(Vec::new());
    // match rather than expect_err, which would ask Tally to be Debug
    // for the sake of a test alone.
    let error = match process_stream(reader, &Lines::default(), &mut writer, &args, Some(2), None) {
        Err(error) => error.to_string(),
        Ok(_) => panic!("a header wider than the count means the input changed"),
    };
    assert!(error.contains("--flexible=widest"), "said: {error}");
    assert!(error.contains("changed between the two reads"), "said: {error}");

    // A count the header fits under still pads and succeeds, so the check
    // refuses the impossible width rather than the mode.
    let mut args = default_args();
    args.has_headers = true;
    args.output_header = true;
    args.flexible_record = true;
    args.fill_mode = FillMode::Widest;
    let reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(&b"a,b\n1,2,3\n"[..]);
    let mut writer = csv::Writer::from_writer(Vec::new());
    process_stream(reader, &Lines::default(), &mut writer, &args, Some(3), None).unwrap();
    let written = String::from_utf8(writer.into_inner().unwrap()).unwrap();
    assert_eq!(written, "a,b,\n1,2,3\n");
}

// ---- clamp_insert_column / insert_target -------------------------------

#[test]
fn clamp_insert_column_never_leaves_the_record() {
    // -i's column is clamped rather than refused, so it can never be out
    // of bound: too low becomes the first column, too high the last.
    assert_eq!(clamp_insert_column(0, 3), 0);
    assert_eq!(clamp_insert_column(2, 3), 2);
    assert_eq!(clamp_insert_column(3, 3), 2);
    assert_eq!(clamp_insert_column(9999, 3), 2);
    assert_eq!(clamp_insert_column(-1, 3), 0);
    assert_eq!(clamp_insert_column(i64::MIN, 3), 0);
    assert_eq!(clamp_insert_column(i64::MAX, 3), 2);
    // One past u32::MAX: an `as usize` cast on a 32-bit target would
    // truncate this to 0 and insert before the first column instead of
    // clamping to the last. On 64-bit hosts both codings agree, so this
    // line pins the documented answer and would catch the wrap on a
    // 32-bit runner.
    assert_eq!(clamp_insert_column(4_294_967_296, 3), 2);
    // An empty record has no column to clamp to. The actions all refuse a
    // zero-length record before reaching here, so this is the defensive
    // answer rather than a reachable one, but it must not underflow.
    assert_eq!(clamp_insert_column(5, 0), 0);
    assert_eq!(clamp_insert_column(-5, 0), 0);
}

#[test]
fn insert_target_uses_the_actions_own_column_when_i_is_absent() {
    let mut args = default_args();
    args.selected_column = 1;
    assert_eq!(insert_target(&args, 3), 1);
    // The two-column --duration falls back to its second column.
    args.action = Action::Duration { second: Some(2) };
    assert_eq!(insert_target(&args, 3), 2);
    // -i, once given, replaces that fallback and is clamped to the row.
    args.insert_column = Some(0);
    assert_eq!(insert_target(&args, 3), 0);
    args.insert_column = Some(99);
    assert_eq!(insert_target(&args, 3), 2);
}

// ---- insert_fields -----------------------------------------------------

#[test]
fn insert_fields_places_one_value_by_position() {
    for (position, expected) in [
        (InsertPosition::Before, vec!["a", "new", "b", "c"]),
        (InsertPosition::Replace, vec!["a", "new", "c"]),
        (InsertPosition::After, vec!["a", "b", "new", "c"]),
    ] {
        let mut fields = vec!["a", "b", "c"];
        insert_fields(&mut fields, 1, position, &["new"]);
        assert_eq!(fields, expected, "position={position:?}");
    }
    // "after" the last column appends rather than running past the end.
    let mut fields = vec!["a", "b", "c"];
    insert_fields(&mut fields, 2, InsertPosition::After, &["new"]);
    assert_eq!(fields, vec!["a", "b", "c", "new"]);
}

#[test]
fn insert_fields_keeps_two_values_in_order() {
    // The pair is written left-to-right whichever side of the column it
    // lands on, which two separate inserts make easy to get backwards.
    for (position, expected) in [
        (InsertPosition::Before, vec!["a", "one", "two", "b", "c"]),
        (InsertPosition::Replace, vec!["a", "one", "two", "c"]),
        (InsertPosition::After, vec!["a", "b", "one", "two", "c"]),
    ] {
        let mut fields = vec!["a", "b", "c"];
        insert_fields(&mut fields, 1, position, &["one", "two"]);
        assert_eq!(fields, expected, "position={position:?}");
    }
}

// ---- remove_columns / arranged -----------------------------------------

#[test]
fn remove_columns_takes_its_list_highest_index_first() {
    // The contract --remove's parsing exists to keep: sorted high-to-low,
    // so dropping one never shifts the position of another still to come.
    let mut fields = vec!["a", "b", "c", "d"];
    remove_columns(&mut fields, &[2, 0]);
    assert_eq!(fields, vec!["b", "d"]);

    let mut every = vec!["a", "b", "c"];
    remove_columns(&mut every, &[2, 1, 0]);
    assert!(every.is_empty());
}

/// FIELDS in --arrange's order, as apply_row_edit writes them.
fn arranged<'a>(fields: &[&'a str], order: &[usize], listed: &[usize]) -> Vec<&'a str> {
    let row = csv::StringRecord::from(fields.to_vec());
    let mut out = Vec::new();
    arrange_into(&mut out, &row, order, listed);
    out.into_iter().map(|field| fields[fields.iter().position(|f| *f == field).unwrap()]).collect()
}

#[test]
fn arranged_puts_named_columns_first_and_keeps_the_rest_in_order() {
    let fields = ["a", "b", "c", "d", "e"];
    // The help's own example: -a3,1 on five columns gives 3,1,0,2,4.
    assert_eq!(
        arranged(&fields, &[3, 1], &[1, 3]),
        vec!["d", "b", "a", "c", "e"]
    );
    // Naming one column is how a column is moved to the front.
    assert_eq!(
        arranged(&fields, &[3], &[3]),
        vec!["d", "a", "b", "c", "e"]
    );
    // Naming all of them is a full reorder, and nothing follows.
    assert_eq!(
        arranged(&fields, &[4, 3, 2, 1, 0], &[0, 1, 2, 3, 4]),
        vec!["e", "d", "c", "b", "a"]
    );
    // Naming none leaves the row alone.
    assert_eq!(arranged(&fields, &[], &[]), fields.to_vec());
}

#[test]
fn arranged_always_returns_the_width_it_was_given() {
    // --arrange reorders and never drops or duplicates, whatever subset is
    // named. That is the promise the help makes about the row's width.
    let fields = ["a", "b", "c", "d", "e"];
    for (order, listed) in [
        (vec![], vec![]),
        (vec![0], vec![0]),
        (vec![4, 0], vec![0, 4]),
        (vec![2, 3, 1], vec![1, 2, 3]),
        (vec![1, 0, 4, 3, 2], vec![0, 1, 2, 3, 4]),
    ] {
        let out = arranged(&fields, &order, &listed);
        assert_eq!(out.len(), fields.len(), "order={order:?}");
        let mut sorted = out.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, fields.to_vec(), "order={order:?}");
    }
}

// ---- is_posix_tz_rule ---------------------------------------------------

#[test]
fn posix_tz_rules_are_told_apart_from_zone_names() {
    // An abbreviation followed immediately by an offset is POSIX rules,
    // which the bundled database cannot name and jiff reads itself.
    for value in [
        "EST5EDT,M3.2.0,M11.1.0",
        "UTC+3",
        "GMT0BST,M3.5.0/1,M10.5.0",
        "est5edt",
        "<+04>-4",
        "<-0330>3:30",
    ] {
        assert!(is_posix_tz_rule(value), "{value} spells POSIX rules");
    }
    // A zone name never carries a digit or a sign in that position, which
    // is what keeps a misspelling a refusal rather than a silent fallback.
    for value in [
        "Europe/Oslo",
        "America/Chicagoo",
        "UTC",
        "EST",
        "",
        "Am",
        // An opening bracket with nothing closing it is not a form at all.
        "<+04",
        // Nor is a bracketed abbreviation with no offset after it.
        "<+04>",
    ] {
        assert!(!is_posix_tz_rule(value), "{value} does not spell POSIX rules");
    }
}

// ---- report_tally --------------------------------------------------------

/// A tally of CONVERTED values, FAILED of them parse_err.
fn tally(converted: u64, failed: u64) -> Tally {
    Tally { converted, failed, ..Default::default() }
}

#[test]
fn report_tally_is_quiet_when_nothing_failed() {
    assert!(report_tally(&tally(40, 0), None, ',').is_ok());
    // Nothing attempted at all, a header-only file, or no action, is
    // not a failure either.
    assert!(report_tally(&tally(0, 0), None, ',').is_ok());
}

#[test]
fn report_tally_fails_only_when_nothing_converted() {
    // Some failures among many rows is ordinary: reported on stderr, but
    // still a successful run.
    assert!(report_tally(&tally(40122, 3), None, ',').is_ok());
    // Every value failing is not, and is the one case that is an error,
    // the column is probably not the one holding the timestamps.
    let error = report_tally(&tally(2, 2), None, ',')
        .expect_err("nothing converted is an error")
        .to_string();
    assert!(error.contains("nothing converted"), "said: {error}");
    assert!(error.contains(PARSE_ERR), "should name the marker, said: {error}");
    // Down to a single row.
    assert!(report_tally(&tally(1, 1), None, ',').is_err());
}

// ---- Tally::record_written -----------------------------------------------

/// The options for a run with COMMENT as the comment character and STYLE as
/// --quote.
fn commented_args(comment: Option<char>, style: &str) -> ArgumentFlags {
    ArgumentFlags {
        read_as_comment: comment,
        output_quotes: resolve_value(&QUOTINGS, style).expect("a quoting style"),
        ..default_args()
    }
}

#[test]
fn record_written_counts_only_a_first_field_the_writer_leaves_bare() {
    // 'never' is the style that leaves a leading comment character
    // bare; under the default the writer quotes it itself.
    let args = commented_args(Some('#'), "never");
    let mut tally = Tally::default();

    // The first field is the only one that can begin a line.
    tally.record_written(&["#x", "2"], &args, false);
    tally.record_written(&["1", "#x"], &args, false);
    assert_eq!(tally.commented_out, 1);

    // An empty record has no first field to ask about.
    tally.record_written(&[], &args, false);
    assert_eq!(tally.commented_out, 1);

    // Without --comment nothing is a comment, so nothing is counted.
    let mut none = Tally::default();
    none.record_written(&["#x", "2"], &commented_args(None, "never"), false);
    assert_eq!(none.commented_out, 0);

    // Under the default style the writer quotes the field itself, so
    // there is nothing to count, and the same for 'always'.
    for style in ["necessary", "always"] {
        let mut quoted = Tally::default();
        quoted.record_written(&["#x", "2"], &commented_args(Some('#'), style), false);
        assert_eq!(quoted.commented_out, 0, "style={style}");
    }

    // The one gap besides 'never': 'nonnumeric' writes a numeric field
    // bare whatever it starts with.
    let mut numeric = Tally::default();
    numeric.record_written(&["-5", "2"], &commented_args(Some('-'), "nonnumeric"), false);
    assert_eq!(numeric.commented_out, 1);
}

#[test]
fn record_written_remembers_when_the_header_is_one_of_them() {
    let args = commented_args(Some('#'), "never");

    // A data row is counted without setting the flag: it goes missing,
    // which is bad, but nothing is promoted in its place.
    let mut rows_only = Tally::default();
    rows_only.record_written(&["#x", "2"], &args, false);
    assert_eq!(rows_only.commented_out, 1);
    assert!(!rows_only.commented_header);

    // The header sets it, and still counts towards the total.
    let mut with_header = Tally::default();
    with_header.record_written(&["#id", "ts"], &args, true);
    with_header.record_written(&["#x", "2"], &args, false);
    assert_eq!(with_header.commented_out, 2);
    assert!(with_header.commented_header);

    // A header the writer quotes is not counted, so the flag stays clear,
    // the flag follows the count rather than merely being the header.
    let mut safe = Tally::default();
    safe.record_written(&["#id", "ts"], &commented_args(Some('#'), "necessary"), true);
    assert_eq!(safe.commented_out, 0);
    assert!(!safe.commented_header);
}

/// The options for a run writing DELIMITER with STYLE as --quote.
fn delimited_args(delimiter: Option<char>, style: &str) -> ArgumentFlags {
    ArgumentFlags {
        output_delimiter: delimiter,
        output_quotes: resolve_value(&QUOTINGS, style).expect("a quoting style"),
        ..default_args()
    }
}

#[test]
fn record_written_counts_a_bare_number_holding_the_output_delimiter() {
    // '.' can sit inside a number, so 'nonnumeric' writes 5.5 bare and a
    // second read splits it. Any field of the row, not only the first: a
    // boundary invented in the middle shifts the columns after it.
    let args = delimited_args(Some('.'), "nonnumeric");
    let mut tally = Tally::default();
    tally.record_written(&["5.5", "x"], &args, false);
    tally.record_written(&["x", "5.5"], &args, false);
    assert_eq!(tally.delimiter_split, 2);

    // Counted once per row however many fields of it split.
    let mut once = Tally::default();
    once.record_written(&["5.5", "1.5"], &args, false);
    assert_eq!(once.delimiter_split, 1);

    // The field must hold the delimiter and be a number. A non-number
    // holding it is quoted, and a number not holding it is safe bare.
    let mut safe = Tally::default();
    safe.record_written(&["a.b", "12"], &args, false);
    assert_eq!(safe.delimiter_split, 0);

    // The default delimiter is not one a number can hold, which is why
    // this needs --print-delimiter to be reachable at all.
    let mut comma = Tally::default();
    comma.record_written(&["5.5", "x"], &delimited_args(None, "nonnumeric"), false);
    assert_eq!(comma.delimiter_split, 0);

    // The styles that quote such a field, and 'never', which the help
    // warns writes invalid CSV rather than counting it afterwards.
    for style in ["necessary", "always", "never"] {
        let mut other = Tally::default();
        other.record_written(&["5.5", "x"], &delimited_args(Some('.'), style), false);
        assert_eq!(other.delimiter_split, 0, "style={style}");
    }
}

#[test]
fn report_tally_still_reports_rows_that_would_read_back_as_comments() {
    // Said whatever else the run did: it is about the output rather than
    // about a conversion, so a --remove or a plain reformat reaches it,
    // and a run that converted everything perfectly can still have it.
    let counted = Tally { converted: 5, failed: 0, commented_out: 2, ..Default::default() };
    assert!(report_tally(&counted, Some('#'), ',').is_ok(), "not fatal on its own");
    // And it does not fire without --comment, where nothing is a comment.
    assert!(report_tally(&counted, None, ',').is_ok());
}

// ---- writer_leaves_comment_bare ------------------------------------------

#[test]
fn only_two_styles_leave_a_leading_comment_character_bare() {
    // The writer is told the comment character, so 'necessary' quotes a
    // field holding one exactly as it quotes a field holding the
    // delimiter, and 'always' always did. The two gaps are the styles the
    // writer cannot quote under: 'never' by definition, and 'nonnumeric'
    // when the field is a number, since that style consults nothing else.
    assert!(!writer_leaves_comment_bare("#x", Quoting::Necessary));
    assert!(!writer_leaves_comment_bare("#x", Quoting::Always));
    assert!(writer_leaves_comment_bare("#x", Quoting::Never));

    // --comment '-' over numeric data: -5 is a number, so 'nonnumeric'
    // writes it bare; -5x is not, so it is quoted and safe.
    assert!(writer_leaves_comment_bare("-5", Quoting::NonNumeric));
    assert!(!writer_leaves_comment_bare("-5x", Quoting::NonNumeric));
    assert!(!writer_leaves_comment_bare("#x", Quoting::NonNumeric));

    // "Number" is csv-core's own is_non_numeric, asked directly, and its
    // reading is a float parse, so scientific notation and even the
    // infinities count as numbers and are written bare. Pinned because
    // these are the cases where a hand-rolled "is it a number" would
    // guess differently.
    assert!(writer_leaves_comment_bare("-1e5", Quoting::NonNumeric));
    assert!(writer_leaves_comment_bare("-inf", Quoting::NonNumeric));
    assert!(writer_leaves_comment_bare("-5.", Quoting::NonNumeric));
}

// ---- the cost of one more row --------------------------------------------

/// Counts the allocations made on the calling thread, so a test can ask
/// what a row costs without timing anything. Per thread because the test
/// harness runs tests side by side, and a shared count would take in
/// whatever the others allocate.
struct CountingAllocator;

thread_local! {
    /// The count, for this thread.
    static ALLOCATIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// SAFETY: every call is handed straight to the system allocator; the count
/// beside it touches a const-initialised thread-local, which allocates
/// nothing itself.
unsafe impl std::alloc::GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        unsafe { std::alloc::System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: std::alloc::Layout) {
        unsafe { std::alloc::System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static COUNTING: CountingAllocator = CountingAllocator;

/// The allocations process_stream makes over ROWS rows of twelve fields.
fn allocations_over(rows: usize, args: &ArgumentFlags) -> u64 {
    let input = "1,2,3,4,5,6,7,8,9,10,11,12\n".repeat(rows);
    let (counted, lines) = count_lines(input.as_bytes());
    let reader = configure_reader(args).from_reader(counted);
    let mut writer = csv::Writer::from_writer(std::io::sink());
    let before = ALLOCATIONS.with(std::cell::Cell::get);
    process_stream(reader, &lines, &mut writer, args, None, None).expect("the run succeeds");
    ALLOCATIONS.with(std::cell::Cell::get) - before
}

/// A row costs a fixed few allocations, whatever the quoting style and
/// however many fields it has: the checks made on each written field
/// compare settings resolved at startup rather than work them out again.
/// Measured as the difference between two input sizes, so what a run
/// costs once, the buffers and the reader, cancels out.
#[test]
fn one_more_row_costs_a_fixed_few_allocations() {
    for (_, style) in QUOTINGS {
        let args = ArgumentFlags {
            output_quotes: style,
            input_quotes: true,
            ..default_args()
        };
        let per_row = (allocations_over(2000, &args) - allocations_over(1000, &args)) / 1000;
        assert!(per_row <= 2, "{style:?}: {per_row} allocations per row of twelve fields");
    }
}
// ---- reading and writing as the input arrives ----------------------------

/// A reader that hands over CHUNK once and then fails the test if it is
/// asked again, standing for a pipe with nothing more to give yet.
struct OneChunk(Option<&'static [u8]>);

impl std::io::Read for OneChunk {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let chunk = self.0.take().expect("read again from a pipe with nothing more yet");
        buffer[..chunk.len()].copy_from_slice(chunk);
        Ok(chunk.len())
    }
}

/// The UTF-16 probe settles for the first read that gives it eight bytes,
/// rather than waiting on a pipe for all 512.
#[test]
fn the_encoding_probe_does_not_wait_for_more_than_it_needs() {
    assert!(reject_utf16(OneChunk(Some(b"2024-01-01T00:00:00Z\n"))).is_ok());
}

/// A writer that counts how many times it is handed bytes.
struct CountedWrites(std::rc::Rc<std::cell::Cell<usize>>);

impl std::io::Write for CountedWrites {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.0.set(self.0.get() + 1);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// On a terminal each row is written as it is made; elsewhere the rows
/// wait in the writer's buffer.
#[test]
fn rows_reach_a_terminal_one_at_a_time() {
    for (flush_each_record, expected) in [(true, 3), (false, 0)] {
        let args = ArgumentFlags { flush_each_record, ..default_args() };
        let writes = std::rc::Rc::new(std::cell::Cell::new(0));
        let reader = configure_reader(&args).from_reader("1\n2\n3\n".as_bytes());
        let mut writer = csv::Writer::from_writer(CountedWrites(writes.clone()));
        process_stream(reader, &Lines::default(), &mut writer, &args, None, None).unwrap();
        assert_eq!(writes.get(), expected, "flush_each_record={flush_each_record}");
    }
}

// ---- counting lines ----------------------------------------------------------

/// The newline search finds the first newline wherever it falls against the
/// eight-byte steps it takes, and nothing where there is none.
#[test]
fn find_newline_finds_the_first_newline_anywhere() {
    for length in 0..40 {
        let mut bytes = vec![b'x'; length];
        assert_eq!(find_newline(&bytes), None, "length {length}");
        for at in 0..length {
            bytes[at] = b'\n';
            assert_eq!(find_newline(&bytes), Some(at), "length {length}, at {at}");
            bytes[at] = 0x8a;
            assert_eq!(find_newline(&bytes), None, "a byte one bit from a newline, length {length}");
            bytes[at] = b'x';
        }
    }
}
