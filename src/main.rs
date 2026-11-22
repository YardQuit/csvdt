mod cli;
mod edit;
mod fill_log;
mod input;
mod man;
mod peek;
mod platform;
mod report;
mod spool;
mod timestamp;
mod zone;
#[cfg(test)]
mod tests;

use std::error::Error;
use chrono::{DateTime, FixedOffset};
use cli::*;
use spool::*;
use input::*;
use peek::*;
use fill_log::*;
use edit::*;
use report::*;

// Writes one line to stderr, accepting that it may not arrive. Everything
// said on stderr is commentary on a result that already stands: a warning
// beside a successful run, or the reason for an exit status the caller reads
// either way. eprintln! panics when stderr is closed or full, which would turn
// those runs into exit 101 with the message lost too.
pub(crate) fn note(message: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "{message}");
}

// The last thing a failing run says, named the way every other diagnostic
// here is. Anonymity costs nothing on a terminal, where the command just
// typed is the line above; everywhere else it costs the only thing that
// makes a line actionable. `csvdt big.csv | sort` with stderr in a log would
// otherwise leave "No space left on device (os error 28)" with nothing saying
// which half of the pipeline ran out of room.
pub(crate) fn fatal(message: &str) {
    note(&format!("csvdt: {message}"));
}

// Writes one of the answers csvdt gives about itself (the option list, the
// manual page) and ends the run where it could not be written.
//
// A closed pipe stays the caller's business: `csvdt --list-options | head -1`
// is a reasonable thing to do and nothing has gone wrong. Every other failure
// is reported, so that `csvdt --generate-man > page.1` on a full disk does not
// succeed for the empty file it leaves behind. A package build reads that
// status and installs what is there.
pub(crate) fn answer(bytes: &[u8]) {
    use std::io::Write;
    let mut stdout = std::io::stdout();
    answered(stdout.write_all(bytes).and_then(|()| stdout.flush()));
}

// Takes the padding off the end of a line and leaves the line ending alone.
//
// A carriage return is part of the ending, not padding. On Windows the help
// corpus is checked out with CRLF unless something says otherwise, so a blank
// line inside an option's long help arrives as ten spaces and a CR. A trim
// that stopped at the CR would leave all ten spaces in the output.
pub(crate) fn without_trailing_padding(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let (body, ending) = match line.strip_suffix('\r') {
            Some(body) => (body, "\r"),
            None => (line, ""),
        };
        out.push_str(body.trim_end_matches([' ', '\t']));
        out.push_str(ending);
    }
    out
}

// Writes what clap renders for --help, -h and --version.
//
// clap's own printer leaves trailing spaces on help lines. Every line of an
// option's long help is indented ten columns, blank lines included, so a
// blank line inside one arrives as ten spaces and a newline. That is
// invisible on a terminal and litter in a file, a diff or a paste.
//
// The bytes are still clap's. StyledStr::ansi() hands back the same string
// clap's own printer writes, and it goes out through the same
// anstream::AutoStream at the same ColorChoice, the default, Auto, which
// is what this Command asks for by never asking. So colour is decided by
// clap's rules exactly as before, on a terminal and not through a pipe, and
// the stream is the one clap would have picked: help and version to standard
// output, and anything else that renders like help to standard error.
//
// Only the trailing spaces differ.
pub(crate) fn say_as_clap_would(error: &clap::Error) -> std::io::Result<()> {
    use std::io::Write;
    let rendered = error.render();
    let trimmed = without_trailing_padding(&rendered.ansi().to_string());

    let choice = anstream::ColorChoice::Auto;
    let write = |stream: &mut dyn Write| {
        stream.write_all(trimmed.as_bytes()).and_then(|()| stream.flush())
    };
    match error.kind() {
        clap::error::ErrorKind::DisplayHelp
        | clap::error::ErrorKind::DisplayVersion => {
            write(&mut anstream::AutoStream::new(std::io::stdout().lock(), choice))
        },
        // DisplayHelpOnMissingArgumentOrSubcommand, which clap sends the
        // other way. csvdt has no subcommands and no argument it demands, so
        // nothing reaches this today; it is here so the stream stays clap's
        // choice rather than becoming a guess if that ever changes.
        _ => write(&mut anstream::AutoStream::new(std::io::stderr().lock(), choice)),
    }
}

// The same policy as answer(), for an answer written by the function above.
// What is left to decide is what a failed write means, and that is this, the
// same as for the answers csvdt writes directly.
pub(crate) fn answered(written: std::io::Result<()>) {
    match written {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => (),
        Err(error) => {
            fatal(&error.to_string());
            std::process::exit(1);
        },
    }
}

fn main() {
    // Asked before anything is parsed, so that every route to standard output
    // is covered by one question rather than each having to remember it.
    // --help and --version are printed by clap on its way out of get_matches,
    // where there is nothing of csvdt's left to ask, so they are handled
    // before clap gets that far.
    if let Err(error) = platform::require_writable_stdout() {
        fatal(&error.to_string());
        std::process::exit(1);
    }

    let args = match arguments() {
        Ok(args) => args,
        Err(err) => {
            fatal(&err.to_string());
            std::process::exit(100);
        },
    };

    match reader_writer(args) {
        Ok(_) => (),
        // A reader that stops reading is not an error to report. `csvdt file
        // | head` closes the pipe as soon as it has what it wants, and Rust
        // leaves SIGPIPE ignored, so the write comes back as an ordinary
        // broken-pipe error rather than ending the process. Reporting it puts
        // a failure on the screen, and a non-zero status into any script using
        // `set -o pipefail`, for one of the most ordinary things anyone does
        // with a command-line tool.
        Err(err) if is_broken_pipe(err.as_ref()) => (),
        Err(err) => {
            fatal(&err.to_string());
            std::process::exit(1);
        }
    }
}

// Whether an error is, or wraps, the far end of the output having closed.
// The csv crate wraps io errors of its own, and the writer is flushed through
// it, so the cause chain has to be walked rather than the top-level type
// inspected.
pub(crate) fn is_broken_pipe(error: &(dyn Error + 'static)) -> bool {
    let mut cause = Some(error);
    while let Some(err) = cause {
        let io = err.downcast_ref::<std::io::Error>().or_else(|| {
            match err.downcast_ref::<csv::Error>().map(csv::Error::kind) {
                Some(csv::ErrorKind::Io(io)) => Some(io),
                _ => None,
            }
        });
        if io.is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe) {
            return true;
        }
        cause = err.source();
    }
    false
}

pub(crate) fn reader_writer(args: ArgumentFlags) -> Result<(), Box<dyn Error>> {
    // Rows are written as they are made when somebody is watching, the way
    // the C library line-buffers a terminal, so a slow input shows up as it
    // arrives. Anywhere else the output is written in large blocks.
    let args = ArgumentFlags {
        flush_each_record: std::io::IsTerminal::is_terminal(&std::io::stdout()),
        ..args
    };
    let reader_builder = configure_reader(&args);

    let mut writer_builder = csv::WriterBuilder::new();
    // Left strict when filling: every row is padded to one width, so the
    // writer refusing a ragged record is a free check that it worked.
    writer_builder
        .has_headers(args.has_headers)
        .flexible(args.flexible_record && args.fill_mode == FillMode::Off)
        // Bigger than the 8 KiB default because of how csv-core quotes: it
        // rescans the whole remaining field for a quote byte on every call,
        // and it is called once per buffer refill, so writing one enormous
        // quoted field costs field_size/buffer_size scans of it, time
        // quadratic in the field. That is not a corner: an unclosed quote
        // turns the rest of the file into one field, which is the damaged
        // input KNOWN LIMITS already warns about, and a multi-megabyte
        // field holding a delimiter or a newline (embedded JSON, a notes
        // column) is quoted for the same reason. A 25 MB field took 2.1s to
        // write and a 200 MB one several minutes; at 1 MiB the constant
        // falls by 128, which is the difference between slow and hung. The
        // asymptote is csv-core's to fix; this buys back the constant for
        // one MiB of a ten MiB floor.
        .buffer_capacity(1 << 20);

    // Told the comment character, so a field holding it is quoted and the
    // output reads back with the same --comment instead of losing the row.
    // Two gaps remain, and the counting covers exactly those:
    // 'never' quotes nothing by definition, and 'nonnumeric' decides by
    // whether the field is a number, so --comment '-' over a field like -5 is
    // still written bare.
    writer_builder.comment(args.read_as_comment.map(|c| c as u8));

    if let Some(value) = args.output_delimiter {
        writer_builder.delimiter(value as u8);
    } else {
        writer_builder.delimiter(b',');
    }

    writer_builder.quote_style(args.output_quotes.style());

    // Standard output was asked about in main, before any of this was
    // parsed. Standard input is asked here, where it is known whether the run
    // is going to read from it at all.
    if args.input_file.is_none() {
        platform::require_readable_stdin()?;
    }
    let mut writer = writer_builder.from_writer(std::io::stdout());

    // --flexible=widest reads the input twice: once to find the widest record,
    // then again to pad every row out to it. A named file can simply be opened
    // again; standard input cannot be rewound, so it is copied to a temporary
    // file first. Either way the counting pass finishes before anything is
    // written, so stdout only ever sees fully padded rows.
    // Opened before the input is copied to a spool or counted, so a
    // destination that cannot be written is reported before the run does its
    // work rather than after: a widest run over a large pipe would otherwise
    // copy the whole of it before finding out it had nowhere to report.
    let mut fill_log = match &args.fill_log {
        Some(destination) => {
            refuse_fill_log_that_is_the_input_or_the_output(
                destination, args.input_file.as_deref())?;
            Some(FillLog::open(destination)?)
        },
        None => None,
    };

    // A named input that cannot be read twice is spooled just as standard
    // input is. Only Widest reads twice, so nothing else pays for this. Held
    // until the end of the run, when its Drop removes the temporary file; the
    // run reads the spool by its path, which is kept as the operating system
    // gave it rather than made into text, so a temporary directory whose name
    // is not UTF-8 works as well as any other.
    let spooled = match (&args.input_file, args.fill_mode) {
        (Some(path), FillMode::Widest) if !is_rereadable(path) => Some(spool(open_input(path)?)?),
        (None, FillMode::Widest) => Some(spool(std::io::stdin())?),
        _ => None,
    };
    let source = spooled
        .as_ref()
        .map(|spool| spool.path.as_path())
        .or(args.input_file.as_deref());

    let scanned_width = match (source, args.fill_mode) {
        (Some(path), FillMode::Widest) => Some(scan_widest_record(&args, path)?),
        _ => None,
    };

    let input: Box<dyn std::io::Read> = match source {
        None => Box::new(std::io::stdin()),
        Some(path) => {
            let file = match open_input(path) {
                Ok(file) => file,
                Err(err) => {
                    // --flexible's mode has to be attached with '=', so writing
                    // "-f fix" leaves "fix" sitting where the file goes. Say
                    // that, rather than only reporting a file nobody named.
                    let mode = path.to_str().filter(|name| matches!(*name, "fix" | "keep" | "widest"));
                    if let (true, Some(mode)) = (args.flexible_record, mode) {
                        return Err(Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            format!("no file named '{mode}'. If you meant the \
                                     --flexible mode, attach it: -f={mode}"))));
                    }
                    return Err(err);
                },
            };
            Box::new(file)
        },
    };
    let reader = reader_builder.from_reader(end_in_newline(reject_utf16(input)?, &args));
    if args.peek {
        answer(peek_stream(reader, &args)?.as_bytes());
        return Ok(());
    }
    let tally = process_stream(reader, &mut writer, &args, scanned_width, fill_log.as_mut())?;

    if let Some(log) = fill_log.take() {
        let rows = log.finish()?;
        report_fill_log(rows, &args);
    }
    writer.flush()?;
    // After the flush: a closed output pipe has already returned by here, and
    // saying how much did not convert is pointless when the reader has stopped
    // listening anyway.
    report_tally(&tally, args.read_as_comment,
                 args.output_delimiter.unwrap_or(','))?;
    Ok(())
}

pub(crate) fn process_stream<R: std::io::Read, W: std::io::Write>(
    mut reader: csv::Reader<R>,
    writer: &mut csv::Writer<W>,
    args: &ArgumentFlags,
    scanned_width: Option<usize>,
    mut fill_log: Option<&mut FillLog>,
) -> Result<Tally, Box<dyn Error>> {
    let mut tally = Tally::default();
    let mut dt_cache: Option<DateTime<FixedOffset>> = None;

    // The width --flexible=fix pads out to. Streaming with flat memory means
    // the widest row can't be known without reading the whole file first, so
    // the first record sets it: the header when there is one, otherwise the
    // first data row. Widest mode brings its width in from the counting pass.
    let mut fill_width: Option<usize> = scanned_width;

    if args.has_headers {
        // Asked before anything pads or edits the record: empty input, or input
        // that is nothing but blank lines, which the CSV reader skips,
        // arrives here as a header of no fields.
        let mut headers = reader.headers()?.clone();

        // Nothing follows a header that was not there, so there is nothing to
        // process and the run ends having written nothing. That is already what
        // the same input does without -H, and the writer renders a record of no
        // fields as `""`, so writing this one would put a field in the output
        // the input never had. Processing it also measured every column option
        // against a record of no fields and reported the column as out of
        // bound, blaming the argument for the input being empty.
        //
        // A header holding one empty field has one field rather than none, so
        // it still round-trips, and one emptied by --remove is not this case
        // either: it was read, and its data rows print `""` as well, so it
        // stays with them.
        if !headers.is_empty() {
            if args.fill_mode == FillMode::FirstRecord {
                fill_width = Some(headers.len());
            }
            // Plain --flexible fills nothing and so has no width of its own.
            // The log still wants one to measure against, and a single pass
            // has only the first record to offer, the header, when there is
            // one, exactly as fix would take it.
            if args.fill_mode == FillMode::Off && fill_log.is_some() {
                fill_width = Some(headers.len());
            }
            // The header is padded too under Widest, since the widest row may
            // be wider than it. The added names are empty rather than invented.
            if let Some(width) = fill_width {
                // Under Widest no record can be wider than the counted width,
                // the header included, the counting pass read it too. A
                // header that is means the input changed between the two
                // reads, which the data rows already report in those terms;
                // unchecked here, it would fall through to the writer's bare
                // record-length error instead.
                if headers.len() > width {
                    if let Some(log) = fill_log.as_deref_mut() {
                        log.note(None, headers.len(), width, "over")?;
                    }
                    return Err(row_wider_than_reference(
                        headers.len(), width, args.fill_mode));
                }
                if let (true, Some(log)) = (headers.len() < width, fill_log.as_deref_mut()) {
                    log.note(None, headers.len(), width, "padded")?;
                }
                // Under FillMode::Off the width above is the header's own, so
                // neither branch can fire and nothing pads: the header is the
                // reference rather than a row measured against it.
                while headers.len() < width {
                    headers.push_field("");
                }
            }
            // Not stored back into the reader: nothing reads its header state
            // after this point, the loop below only calls read_record, and
            // the writer is handed the record directly.
            let new_headers = process_headers(&headers, args)
                .map_err(|error| locate_out_of_bound(error, &headers, true))?;
            if args.output_header {
                // Asked of the header too, and only when one is actually
                // written: a run without -p/--print-header puts no header in
                // the output, so there is none there to be read back as a
                // comment. It is the worst of the three cases, see
                // report_tally.
                let names: Vec<&str> = new_headers.iter().collect();
                tally.record_written(&names, args, true);
                writer.write_record(&new_headers)?;
            }
        }
    }

    // Reuses one StringRecord buffer across every row instead of the
    // `records()` iterator, which clones a fresh StringRecord per row.
    //
    // The row is then planned and written without ever materialising a second
    // StringRecord: `fields` holds &str borrowed from `record` and from the
    // edit's owned strings, and goes straight to the writer. Building an
    // output StringRecord here instead would mean allocating and copying
    // every field of every row, which measured ~35% slower on a 2M-row file.
    let mut record = csv::StringRecord::new();
    while reader.read_record(&mut record).map_err(explain_unequal_lengths)? {
        if args.fill_mode != FillMode::Off {
            // Without a header to measure, the first data row sets the width.
            let width = *fill_width.get_or_insert(record.len());
            if record.len() > width {
                // Logged before the error returns, so the log ends on the row
                // that ended the run rather than stopping just short of the
                // one a reader most wants to see.
                if let Some(log) = fill_log.as_deref_mut() {
                    log.note(record.position(), record.len(), width, "over")?;
                }
                return Err(row_wider_than_reference(
                    record.len(), width, args.fill_mode));
            }
            if let (true, Some(log)) = (record.len() < width, fill_log.as_deref_mut()) {
                log.note(record.position(), record.len(), width, "padded")?;
            }
            // Padded before the action runs, so a column index refers to the
            // rectangular row: an action pointed at a field this row was
            // missing sees an empty value rather than an out-of-bound error.
            while record.len() < width {
                record.push_field("");
            }
        } else if let Some(log) = fill_log.as_deref_mut() {
            // Plain --flexible passes every row through untouched, so the
            // log records where each row stands rather than what was done to
            // it, 'short' where fix or widest would have padded. Nothing in
            // this mode's output marks a short row, which makes the log the
            // only account of one there is.
            //
            // Measured against the first record, which is the only reference
            // a single pass has, and so a narrower one than widest would use:
            // a row wider than the first is 'over' here, where widest would
            // merely have counted it and padded the rest out to it.
            let width = *fill_width.get_or_insert(record.len());
            match record.len().cmp(&width) {
                std::cmp::Ordering::Less => {
                    log.note(record.position(), record.len(), width, "short")?
                },
                std::cmp::Ordering::Greater => {
                    log.note(record.position(), record.len(), width, "over")?
                },
                std::cmp::Ordering::Equal => (),
            }
        }
        let (edit, new_dt_cache) = plan_row_edit(&record, args, dt_cache)
            .map_err(|error| locate_out_of_bound(error, &record, false))?;
        tally.record(&edit);
        // Must be built per row: it borrows `record`, which the next
        // read_record() overwrites, so it cannot be hoisted out of the loop.
        let mut fields: Vec<&str> = Vec::with_capacity(record.len() + 2);
        apply_row_edit(&record, args, &edit, &mut fields);
        tally.record_written(&fields, args, false);
        writer.write_record(&fields)?;
        if args.flush_each_record {
            writer.flush()?;
        }
        dt_cache = new_dt_cache;
    }

    Ok(tally)
}

// Rows longer than the reference width cannot be made rectangular by padding,
// and shortening them would throw fields away.
//
// Which mode is running decides what to say, because the two reach this for
// opposite reasons. Under 'fix' the width came from the first record and a
// wider row later is ordinary: that is the documented limit of a single
// pass. Under 'widest' the width came from a pass over the whole input, so no
// row can be too wide for it: getting here at all means the file is not the
// one that was counted, which the message has to say, since 'widest' promises
// exactly the thing that appears to have failed.
pub(crate) fn row_wider_than_reference(row_len: usize, width: usize, mode: FillMode) -> Box<dyn Error> {
    let message = match mode {
        FillMode::Widest => format!(
            "'--flexible=widest' counted {} fields as the widest record in a \
             first pass over the whole input, but the second pass has reached \
             a row of {}. No row can be wider than that count, so the input \
             changed between the two reads -- a file still being appended to, \
             or replaced. Copy it first, or use '--flexible=fix' or \
             '--flexible' on its own, which read once and cannot disagree \
             with themselves", width, row_len),
        _ => format!(
            "'--flexible=fix' pads short rows out to the width of the \
             first record ({} fields), but this row has {}. Shortening it \
             would discard data, so fix the input or use '--flexible' on \
             its own to pass rows through as they are", width, row_len),
    };
    Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, message))
}
