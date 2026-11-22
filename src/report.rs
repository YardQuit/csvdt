//! What a run says on stderr when it ends: the tally of what it wrote.

use std::error::Error;
use crate::cli::*;
use crate::timestamp::*;
use crate::edit::*;
use crate::note;

/// Whether the writer, told the comment character, still writes a field that
/// begins with it bare, which is what decides whether the field survives as
/// data or starts a comment when the output is read back.
///
/// The writer quotes it itself under 'necessary' (the comment character is a
/// reason to quote, exactly like the delimiter) and under 'always'. The two
/// styles left are the two real gaps: 'never' quotes nothing by definition, and
/// 'nonnumeric' decides by whether the field is a number and consults nothing
/// else, so --comment '-' over a field like -5 is written bare. That numeric
/// test is csv-core's own is_non_numeric, asked directly rather than mirrored,
/// so this answer cannot drift from what the writer actually does.
pub(crate) fn writer_leaves_comment_bare(field: &str, style: Quoting) -> bool {
    match style {
        Quoting::Never => true,
        Quoting::NonNumeric => !csv_core::is_non_numeric(field.as_bytes()),
        Quoting::Always | Quoting::Necessary => false,
    }
}

/// Whether the writer leaves this field bare although it holds the delimiter
/// being written, which is what turns one field into two when the output is
/// read back, and every column number after it into a different one.
///
/// Only 'nonnumeric' is asked. 'necessary' quotes a field holding the delimiter
/// and 'always' quotes every field, so under those two it cannot happen.
/// 'never' can do it, and often does, but the help says so in as many words,
/// it "never writes quotes, even where that produces invalid CSV data", and
/// this is that, announced in advance. 'nonnumeric' says the opposite: quotes
/// go on fields that do not need them, which no reader takes to mean a field
/// that does need them goes without. So it is the one style that can lose a
/// field boundary without having warned anybody.
///
/// It takes a delimiter a number can hold, --print-delimiter '.' over the
/// field 5.5, or '-' over -5, or 'e' over 1e5. The numeric test is csv-core's
/// own is_non_numeric, asked directly rather than mirrored, so the answer
/// cannot drift from what the writer actually does.
pub(crate) fn writer_splits_on_delimiter(field: &str, args: &ArgumentFlags) -> bool {
    if args.output_quotes != Quoting::NonNumeric {
        return false;
    }
    field.contains(args.output_delimiter.unwrap_or(','))
        && !csv_core::is_non_numeric(field.as_bytes())
}

/// Whether a field, as this run writes it, comes back changed when the output
/// is read again with '--single-quote'. The writer always quotes with '"',
/// the output is ordinary CSV, but that reader takes only ''' as a quote, so
/// a field the writer quoted keeps its '"' as data and splits on any delimiter
/// inside it; and a bare field that itself begins with ''' is taken for a
/// quoted one. Reading the output back without '--single-quote' is exact.
///
/// The quoted-or-bare question is the writer's, answered per style: 'always'
/// quotes everything, 'never' nothing, 'nonnumeric' asks csv-core's own
/// is_non_numeric, and 'necessary' quotes a field holding the delimiter, the
/// quote, a record boundary, or the comment character, the same set the
/// writer is configured with above, restated here because csv-core does not
/// expose its decision.
pub(crate) fn single_quote_misreads(field: &str, args: &ArgumentFlags) -> bool {
    let quoted = match args.output_quotes {
        Quoting::Always => true,
        Quoting::Never => false,
        Quoting::NonNumeric => csv_core::is_non_numeric(field.as_bytes()),
        Quoting::Necessary => field.contains(args.output_delimiter.unwrap_or(','))
            || field.contains('"')
            || field.contains('\r')
            || field.contains('\n')
            || args.read_as_comment.is_some_and(|c| field.contains(c)),
    };
    quoted || field.starts_with('\'')
}

/// How many rows a conversion was asked for a value for, how many it could not
/// produce one for, and how many rows the output starts with the comment
/// character in a field the writer left unquoted.
#[derive(Default)]
pub(crate) struct Tally {
    /// Rows a conversion was asked to give a value for.
    pub(crate) converted: u64,
    /// Rows among those it could not give one for.
    pub(crate) failed: u64,
    /// Rows written with a bare first field that starts with the comment
    /// character.
    pub(crate) commented_out: u64,
    /// Whether the header is one of them, which is a worse outcome than a data
    /// row and so is said separately.
    pub(crate) commented_header: bool,
    /// Rows that a '--single-quote' re-read would take apart, see
    /// single_quote_misreads. Only counted when that flag is on.
    pub(crate) single_quote_misread: u64,
    /// Rows holding a bare field that itself holds the delimiter being written,
    /// so a second read finds more fields in the row than were written, see
    /// writer_splits_on_delimiter. Only reachable under '--quote nonnumeric'.
    pub(crate) delimiter_split: u64,
}

impl Tally {
    /// Counts a row's conversion, and whether it failed.
    pub(crate) fn record(&mut self, edit: &RowEdit) {
        if edit.is_conversion() {
            self.converted += 1;
            if edit.is_parse_error() {
                self.failed += 1;
            }
        }
    }

    /// Counted from the fields as they go to the writer, since that is the only
    /// point at which what the output will actually look like is known: which
    /// field is first depends on --remove, -a/--arrange and -i, and whether it
    /// is quoted depends on --quote and on the field's own contents.
    ///
    /// HEADER says this is the header record. It is asked the same question as
    /// any other row, a column name may start with the comment character as
    /// readily as a value can, but what follows from losing it is different,
    /// so it is remembered rather than only counted.
    pub(crate) fn record_written(&mut self, fields: &[&str], args: &ArgumentFlags, header: bool) {
        // Any field of the row can break a '--single-quote' re-read, not only
        // the first: a quoted field splits mid-row wherever it sits. One
        // quoting rule is the record's rather than any field's: a row that is
        // a single empty field is written quoted ("") so the record does not
        // vanish into a blank line, under every style, 'never' included.
        // The per-field mirror in single_quote_misreads cannot see that rule,
        // so it is asked here, where the whole row is. A record --remove left
        // with no fields at all is written as the same "", and misreads the
        // same way, so it counts the same way.
        let lone_empty = match fields {
            [] => true,
            [only] => only.is_empty(),
            _ => false,
        };
        if args.input_quotes
            && (lone_empty
                || fields.iter().any(|field| single_quote_misreads(field, args)))
        {
            self.single_quote_misread += 1;
        }

        // Asked of every field, not only the first: a field boundary invented
        // anywhere in the row shifts every column number after it, and the
        // header is no worse off than a data row here: both come back one
        // field wider, and the numbering they are read by comes back with them.
        if fields.iter().any(|field| writer_splits_on_delimiter(field, args)) {
            self.delimiter_split += 1;
        }

        let Some(comment) = args.read_as_comment else { return };
        let Some(first) = fields.first() else { return };
        if !first.starts_with(comment) {
            return;
        }
        if writer_leaves_comment_bare(first, args.output_quotes) {
            self.commented_out += 1;
            self.commented_header |= header;
        }
    }
}

/// Says at the end of a run how much of it did not convert.
///
/// A few failures among many rows is ordinary: one malformed line should not
/// throw away a large log, so that stays exit 0 and is reported on stderr,
/// where it does not disturb the output but a person can see it. Nothing
/// converting at all is a different thing: the column given is not the one
/// holding the timestamps, or they are not in a format csvdt reads. That is an
/// error exit, because exit 0 is indistinguishable from success and a script
/// would carry on and analyse a file of markers.
pub(crate) fn report_tally(tally: &Tally, comment: Option<char>, delimiter: char)
    -> Result<(), Box<dyn Error>> {
    // Said whatever else the run did, since it is about the output rather than
    // about any conversion, a --remove or a plain reformat reaches it too.
    if let (Some(comment), true) = (comment, tally.commented_out > 0) {
        note(&format!(
            "csvdt: {} row(s) begin with {}, which '--comment' makes the start of \
             a comment. The quoting style asked for leaves the field bare, so \
             reading this output back with the same '--comment' would drop those \
             rows without saying so.{}{}",
            tally.commented_out,
            describe_character(comment),
            // Worth its own sentence: a dropped data row is a row short, but a
            // dropped header leaves '-H' taking the first data row for the
            // column names. That row then stops being data, a header is
            // exempt from every conversion, so it is not merely lost, it is
            // silently promoted, and every column number still lines up.
            if tally.commented_header {
                " The header is one of them, so a second read takes the first \
                 data row for the column names, and that row is then exempt \
                 from the conversion rather than missing from it."
            } else {
                ""
            },
            // The usual advice recommends the styles that quote the field,
            // unless the comment character IS the quote character, where those
            // styles are refused outright (a quoted field would start a
            // comment just the same) and 'never' is the only style accepted.
            // Recommending the refused ones would send the reader in a circle.
            if comment == '"' {
                " No quoting style can keep such a field out of a comment, \
                 since quoting is done with that same character; pick a \
                 different one for '--comment'"
            } else {
                " 'necessary', the default, and 'always' both quote such a \
                 field, so its row survives a second read"
            }));
    }

    if tally.single_quote_misread > 0 {
        note(&format!(
            "csvdt: {} row(s) hold a field that '--single-quote' would \
             misread. The writer quotes with '\"' whatever the reader was \
             told, so this output is ordinary double-quoted CSV: read with \
             '--single-quote' again, a quoted field keeps its quotes as data \
             and splits on any delimiter inside it. Read the output back \
             without '--single-quote'",
            tally.single_quote_misread));
    }

    // Said for the same reason the comment count is: the rows are all there in
    // the output, and it is only the next read of it that loses their shape, so
    // nothing about this run looks wrong. Unlike '--quote never', which the
    // help warns writes invalid CSV where the data needs quoting, 'nonnumeric'
    // promises quotes a field does not need, so this is the one place the
    // output stops being the table that was written and nothing had said it
    // might.
    if tally.delimiter_split > 0 {
        note(&format!(
            "csvdt: {} row(s) hold a number written bare that itself contains \
             {}, the delimiter being written, so reading this output back would \
             find more fields in those rows than were written. '--quote \
             nonnumeric' decides by the numeric test alone and a number is \
             never quoted; 'necessary', the default, and 'always' both quote a \
             field holding the delimiter, and a delimiter no number can hold \
             avoids it under any style",
            tally.delimiter_split,
            describe_character(delimiter)));
    }

    if tally.failed == 0 {
        return Ok(());
    }

    if tally.failed == tally.converted {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "nothing converted: all {} values were written as {}. Check the \
                 column number, and that the timestamps are in a format csvdt \
                 reads -- '--help' ends with TIMESTAMP FORMATS. The output was \
                 written either way; this status is what says it holds no \
                 converted value.",
                tally.converted, PARSE_ERR),
        )));
    }

    note(&format!(
        "csvdt: {} of {} values could not be converted, and were written as {}",
        tally.failed, tally.converted, PARSE_ERR));
    Ok(())
}
