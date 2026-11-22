//! What an action makes of each row, and of the header.

use std::error::Error;
use crate::cli::*;
use crate::timestamp::*;

/// A column an action names that the record it is measured against does not
/// have. A type of its own, so the caller that knows which record that was
/// can recognise it and say so; see locate_out_of_bound.
#[derive(Debug)]
pub(crate) struct OutOfBound;

impl std::fmt::Display for OutOfBound {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("Specified column is out of bound")
    }
}

impl Error for OutOfBound {}

/// The error a column past the end of a record gives.
pub(crate) fn out_of_bound() -> Box<dyn Error> {
    Box::new(OutOfBound)
}

/// Attaches which record an out-of-bound column was measured against, AT its
/// line and record number, or the header where AT is None. Said in the
/// same terms the CSV reader's own errors use: "record 3 (line: 4)" for a data
/// row, "the header" for the header. The bare message names neither the
/// record nor its width, which matters most in a ragged file, where finding
/// the one short row is the task. Any other error is handed back as it was.
pub(crate) fn locate_out_of_bound(
    error: Box<dyn Error>, record: &csv::StringRecord, at: Option<(u64, u64)>,
) -> Box<dyn Error> {
    if !error.is::<OutOfBound>() {
        return error;
    }
    let location = match at {
        None => "the header".to_string(),
        Some((line, number)) => format!("record {number} (line: {line})"),
    };
    Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("{OutOfBound}: {} has {} field(s), counted from 0", location, record.len())))
}

/// Clamps -i's column into a valid index for this record instead of
/// producing an out-of-bound error: negative (or otherwise too low) becomes
/// the first column, too high becomes the last, -i's target can never be
/// out of bound.
pub(crate) fn clamp_insert_column(value: i64, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    if value < 0 {
        return 0;
    }
    // try_from rather than `as`: on a 32-bit target `as usize` truncates, so
    // a column past 2^32 would wrap to a small index instead of clamping to
    // the last column as documented. A value usize cannot hold is over any
    // record's length by construction, which is the Err arm.
    match usize::try_from(value) {
        Ok(column) if column < len => column,
        _ => len - 1,
    }
}

/// Puts VALUES into `fields` around `column`: ahead of it, over it, or behind
/// it, as `position` says, in the order given. Shared by every action that
/// writes one or two fields, so the before/replace/after dispatch exists once.
pub(crate) fn insert_fields<'a>(
    fields: &mut Vec<&'a str>,
    column: usize,
    position: InsertPosition,
    values: &[&'a str],
) {
    let range = match position {
        InsertPosition::Before => column..column,
        InsertPosition::Replace => column..column + 1,
        InsertPosition::After => column + 1..column + 1,
    };
    fields.splice(range, values.iter().copied());
}

/// Drops every column in `columns` from `fields`. `columns` must be ordered
/// highest index first (as --remove's parsing guarantees), so each removal
/// leaves the positions of those still to come untouched.
pub(crate) fn remove_columns(fields: &mut Vec<&str>, columns: &[usize]) {
    for &column in columns {
        fields.remove(column);
    }
}

/// Fills `fields` with ROW in --arrange's order: the columns it names first,
/// in the order given, then every column it didn't name, keeping their
/// original relative order. Nothing is dropped and nothing is duplicated, so
/// the row always comes out the same width it went in. Built straight from the
/// record rather than filled in input order and then permuted, which would
/// need a second buffer to read from.
pub(crate) fn arrange_into<'a>(
    fields: &mut Vec<&'a str>,
    row: &'a csv::StringRecord,
    order: &[usize],
    listed: &[usize],
) {
    for &column in order {
        fields.push(&row[column]);
    }
    for (index, field) in row.iter().enumerate() {
        if listed.binary_search(&index).is_err() {
            fields.push(field);
        }
    }
}

/// The header as the output carries it, with the action's new columns named.
pub(crate) fn process_headers(
    headers: &csv::StringRecord,
    args: &ArgumentFlags,
) -> Result<csv::StringRecord, Box<dyn Error>> {
    let edit = plan_header_edit(headers.len(), args)?;
    let mut fields = Vec::with_capacity(headers.len() + 2);
    apply_row_edit(headers, args, &edit, &mut fields);
    Ok(csv::StringRecord::from(fields))
}

/// What the action makes of the header: the same edit a data row gets, with
/// the new columns' names in place of their values. A name carries a '>' or
/// '<' pointing at the column it was made from, unless it replaces it.
pub(crate) fn plan_header_edit(len: usize, args: &ArgumentFlags) -> Result<RowEdit, Box<dyn Error>> {
    check_bounds(args, len)?;
    let target = insert_target(args, len);
    let name = |base: &str| match args.insert_position {
        InsertPosition::Before => format!("{base}>"),
        InsertPosition::Replace => base.to_string(),
        InsertPosition::After => format!("<{base}"),
    };
    let single = |base: &str| RowEdit::Single { target, value: name(base) };
    Ok(match &args.action {
        Action::Passthrough => RowEdit::Passthrough,
        Action::Remove(_) => RowEdit::Remove,
        Action::Arrange { .. } => RowEdit::Arrange,
        Action::Split => RowEdit::Pair {
            target, first: name("split_date"), second: name("split_time"),
        },
        Action::ToRfc3339 => single("to_rfc3339"),
        Action::ToUtc => single("to_utc"),
        Action::ToLocal(_) => single("to_local"),
        Action::ToOffset(_) => single("to_offset"),
        Action::Duration { second: Some(_) } => single("row_duration"),
        Action::Duration { second: None } => single("duration"),
    })
}

/// Refuses a record too narrow for a column the action names. Asked of the
/// header and of every data row alike, and of every column before any is
/// touched, so a row is either edited fully or reported, never left half done.
pub(crate) fn check_bounds(args: &ArgumentFlags, len: usize) -> Result<(), Box<dyn Error>> {
    let within = |column: usize| match column < len {
        true => Ok(()),
        false => Err(out_of_bound()),
    };
    match &args.action {
        Action::Passthrough => Ok(()),
        Action::Remove(columns) | Action::Arrange { order: columns, .. } => {
            columns.iter().try_for_each(|&column| within(column))
        },
        Action::Duration { second } => {
            within(args.selected_column)?;
            second.map_or(Ok(()), within)
        },
        _ => within(args.selected_column),
    }
}

/// The one edit an action makes to a row: which column it targets and the
/// value(s) written there. Computed separately from applying it, because the
/// owned Strings have to live in the *caller's* frame: that's what lets the
/// field list borrow them and go straight to the CSV writer, with no
/// intermediate StringRecord copy per row (see process_stream).
pub(crate) enum RowEdit {
    /// The row as it was read.
    Passthrough,
    /// One value written at a column.
    Single {
        /// Where the value goes.
        target: usize,
        /// The value, or the parse_err marker.
        value: String,
    },
    /// Two values written side by side at a column, as -s writes its date
    /// and time.
    Pair {
        /// Where the pair goes.
        target: usize,
        /// The first value.
        first: String,
        /// The second value.
        second: String,
    },
    /// The --remove columns dropped.
    Remove,
    /// The columns in -a/--arrange's order.
    Arrange,
}

impl RowEdit {
    /// Whether a conversion ran for this row at all. --remove and -a/--arrange
    /// move fields about without reading one, so there is nothing that could
    /// have failed; nor is there with no action given.
    pub(crate) fn is_conversion(&self) -> bool {
        matches!(self, RowEdit::Single { .. } | RowEdit::Pair { .. })
    }

    /// Whether the conversion could not produce a result for this row. Asked of
    /// the edit rather than counted at each place that writes the marker, so a
    /// conversion added later cannot forget to be counted.
    pub(crate) fn is_parse_error(&self) -> bool {
        match self {
            RowEdit::Single { value, .. } => value == PARSE_ERR,
            RowEdit::Pair { first, second, .. } => {
                first == PARSE_ERR || second == PARSE_ERR
            },
            RowEdit::Passthrough | RowEdit::Remove | RowEdit::Arrange => false,
        }
    }
}

/// Resolves where the action's result goes in this row: -i's column, clamped
/// into the row, or without -i the action's own column, which for the
/// two-column --duration is the second.
pub(crate) fn insert_target(args: &ArgumentFlags, len: usize) -> usize {
    let fallback = match args.action {
        Action::Duration { second: Some(second) } => second,
        _ => args.selected_column,
    };
    match args.insert_column {
        Some(value) => clamp_insert_column(value, len),
        None => fallback,
    }
}

/// An edit for one row, and the timestamp the row-over-row --duration carries
/// into the next.
pub(crate) type PlannedEdit = (RowEdit, Option<Stamp>);

/// Parses the row and works out what to write, without touching the field
/// list. PREVIOUS_STAMP is the last timestamp the row-over-row --duration
/// read, if any; the edit comes back with the one to carry into the next row.
pub(crate) fn plan_row_edit(
    record: &csv::StringRecord,
    args: &ArgumentFlags,
    previous_stamp: Option<Stamp>,
) -> Result<PlannedEdit, Box<dyn Error>> {
    check_bounds(args, record.len())?;
    let target = insert_target(args, record.len());
    let parse_err = || String::from(PARSE_ERR);
    let timestamp = || parse_rfc3339_lenient(&record[args.selected_column]);
    let single = |value| RowEdit::Single { target, value };

    let edit = match &args.action {
        Action::Passthrough => RowEdit::Passthrough,
        Action::Remove(_) => RowEdit::Remove,
        Action::Arrange { .. } => RowEdit::Arrange,
        Action::Split => {
            // The time keeps a fraction the source carried and writes
            // nothing when there is none, as RFC3339 output does. Whole
            // seconds would drop it silently, which no other conversion here
            // does: -d keeps it unless --round-seconds asks otherwise, and
            // -u/-l/-o write back whatever came in.
            let (first, second) = match timestamp() {
                Ok(stamp) => stamp.split(),
                Err(_) => (parse_err(), parse_err()),
            };
            RowEdit::Pair { target, first, second }
        },
        // Trimmed for the same reason parse_rfc3339_lenient is.
        Action::ToRfc3339 => single(match record[args.selected_column].trim().parse::<i64>() {
            // A year outside four digits would be ISO 8601's expanded form
            // ("+57534-06-16T...", "-0001-12-31T..."). That is not RFC3339,
            // which this option promises: csvdt cannot read it back, and nor
            // can anything else expecting the format. Epoch milliseconds
            // handed to an option expecting seconds land there, which is a
            // mistake worth reporting rather than answering with a timestamp
            // no one can use.
            Ok(seconds) => to_rfc3339_in_range(Stamp::from_epoch_seconds(seconds)),
            Err(_) => parse_err(),
        }),
        Action::ToUtc => single(match timestamp() {
            Ok(stamp) => to_rfc3339_in_range(stamp.on_offset(jiff::tz::Offset::UTC)),
            Err(_) => parse_err(),
        }),
        Action::ToLocal(zone) => single(match timestamp() {
            Ok(stamp) => to_rfc3339_in_range(stamp.in_zone(zone)),
            Err(_) => parse_err(),
        }),
        Action::ToOffset(offset) => single(match timestamp() {
            Ok(stamp) => to_rfc3339_in_range(stamp.on_offset(*offset)),
            Err(_) => parse_err(),
        }),
        Action::Duration { second: Some(second) } => {
            single(match (timestamp(), parse_rfc3339_lenient(&record[*second])) {
                (Ok(first), Ok(second)) => format_iso8601_duration(second.since(&first), args.round_seconds),
                _ => parse_err(),
            })
        },
        Action::Duration { second: None } => match timestamp() {
            Ok(stamp) => {
                // No previous row yet (first row, or every row so far failed
                // to parse) means there's nothing to measure against, so the
                // duration is zero. Tracked with None rather than a sentinel
                // timestamp, a sentinel would make real data holding that
                // exact instant (e.g. the Unix epoch, which is how a lot of
                // exports write a null date) look like "no previous row" and
                // silently report PT0S.
                let previous = previous_stamp.unwrap_or(stamp);
                let value = format_iso8601_duration(stamp.since(&previous), args.round_seconds);
                return Ok((single(value), Some(stamp)));
            },
            // A row we can't parse leaves the stamp untouched, so the next
            // parseable row still measures against the last real timestamp.
            Err(_) => single(parse_err()),
        },
    };
    Ok((edit, previous_stamp))
}

/// Fills `fields` with the row's final contents: the record's own fields,
/// borrowed, with `edit` applied. Every &str borrows either the record or the
/// edit, so nothing is copied here. The header is built the same way, from
/// the edit plan_header_edit makes of it.
pub(crate) fn apply_row_edit<'a>(
    record: &'a csv::StringRecord,
    args: &ArgumentFlags,
    edit: &'a RowEdit,
    fields: &mut Vec<&'a str>,
) {
    let position = args.insert_position;
    fields.clear();
    match (edit, &args.action) {
        (RowEdit::Arrange, Action::Arrange { order, listed }) => {
            arrange_into(fields, record, order, listed);
        },
        (RowEdit::Remove, Action::Remove(columns)) => {
            fields.extend(record.iter());
            remove_columns(fields, columns);
        },
        (RowEdit::Single { target, value }, _) => {
            fields.extend(record.iter());
            insert_fields(fields, *target, position, &[value.as_str()]);
        },
        (RowEdit::Pair { target, first, second }, _) => {
            fields.extend(record.iter());
            insert_fields(fields, *target, position, &[first.as_str(), second.as_str()]);
        },
        _ => fields.extend(record.iter()),
    }
}
