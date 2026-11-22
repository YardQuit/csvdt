//! --fill-log: the record-by-record account of what a filling mode did.

use std::error::Error;
use crate::platform;
use crate::cli::*;
use crate::note;

/// Refuses a '--fill-log' destination that is the input, or where the output
/// is going.
///
/// The log is created, and creating truncates, so this is asked before the
/// file is opened rather than after. 'csvdt -f=widest --fill-log data.csv
/// data.csv' would empty the input, read the empty file it had just made, write
/// nothing and exit 0: the same loss refuse_input_that_is_also_the_output
/// guards against, by a route that guard does not watch.
///
/// Asked by identity rather than by path, so a symbolic link to the input is
/// caught with it, and so is a redirection: 'csvdt --fill-log data.csv <
/// data.csv' names no input path at all, and '--fill-log /dev/stdout' names
/// no file but is one.
///
/// Nothing is opened to ask: a fifo opened to read waits for a writer, and
/// csvdt is the writer it would be waiting for. A destination that does not
/// exist yet cannot be either, and is the ordinary case: nothing is refused.
pub(crate) fn refuse_fill_log_that_is_the_input_or_the_output(
    destination: &std::path::Path,
    input_file: Option<&std::path::Path>,
) -> Result<(), Box<dyn Error>> {
    if is_stderr(destination) {
        return Ok(());
    }
    let Some(log) = platform::path_identity(destination) else { return Ok(()) };
    let destination = destination.display();

    let input = match input_file {
        None => platform::stdin_identity(),
        Some(path) => platform::path_identity(path),
    };
    if input == Some(log) {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("'{destination}' is both the input and where \
                     '--fill-log' would write. The log is created before the \
                     input is read, so this would empty the file csvdt is \
                     about to convert. Name another file, or '-' for standard \
                     error"))));
    }

    let output = platform::stdout_identity();
    if output == Some(log) {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("'{destination}' is where both the output and \
                     '--fill-log' would go. The log is CSV of its own, and \
                     mixed into the conversion it becomes rows of it. Name \
                     another file, or '-' for standard error"))));
    }
    Ok(())
}

/// Whether a --fill-log destination is '-', which means standard error.
pub(crate) fn is_stderr(destination: &std::path::Path) -> bool {
    destination.as_os_str() == "-"
}

/// The record-by-record account --fill-log writes.
///
/// A filling mode changes the input, and the WARNING under --flexible says so:
/// the empty fields it adds cannot be told from empty fields the source really
/// held. This says exactly where, one row per record, so the change can be
/// reviewed against the file it was made to.
///
/// CSV, so it loads in whatever the data loads in and joins to the output on
/// the row number. Written as it goes rather than collected and written at the
/// end: a file where every row is short would otherwise be held in memory
/// entire, against a design whose memory grows with the widest record and not
/// with the file.
pub(crate) struct FillLog {
    /// The log, as CSV, to its file or standard error.
    writer: csv::Writer<Box<dyn std::io::Write>>,
    /// How many rows have been logged, for the line said at the end.
    rows: u64,
}

impl FillLog {
    /// '-' means stderr, for looking at a run rather than keeping it. Anything
    /// else is a path, created rather than appended to: a log that grew across
    /// runs would describe rows from an input that has since changed.
    pub(crate) fn open(destination: &std::path::Path) -> Result<Self, Box<dyn Error>> {
        let sink: Box<dyn std::io::Write> = if is_stderr(destination) {
            Box::new(std::io::stderr())
        } else {
            Box::new(std::fs::File::create(destination).map_err(|error| {
                std::io::Error::new(
                    error.kind(),
                    format!(
                        "could not open '{}' for '--fill-log': \
                         {error}. Give a path csvdt can write, or '-' for \
                         standard error",
                        destination.display(),
                    ),
                )
            })?)
        };
        let mut writer = csv::Writer::from_writer(sink);
        writer.write_record(
            ["line", "record", "fields", "width", "delta", "status"])?;
        Ok(FillLog { writer, rows: 0 })
    }

    /// Logs one record, AT its physical line and its record number: 'line' is
    /// the line an editor jumps to and 'record' counts records, which drift
    /// apart the moment a field holds a newline, and the pair is what lets a
    /// reader find the row either way.
    pub(crate) fn note(
        &mut self,
        at: (u64, u64),
        fields: usize,
        width: usize,
        action: &str,
    ) -> Result<(), Box<dyn Error>> {
        let (line, record) = at;
        self.writer.write_record([
            &line.to_string(),
            &record.to_string(),
            &fields.to_string(),
            &width.to_string(),
            // Fields minus width: how far the row is from the shape it was
            // measured against, negative where it falls short and positive
            // where it runs over. The row's count comes first because the
            // deviation is the row's; the other way round would put a minus
            // sign beside the word 'over'.
            //
            // It describes the row, not what was done to it, so it is not
            // called 'added': beside an untouched row, "added 2" would read
            // as something done to it.
            //
            // Signed rather than saturating, so it says something on every
            // row, including how far over a wide row runs. It is never zero:
            // a row matching the width is not logged at all. 'delta' names
            // neither direction because it has two.
            &(fields as i64 - width as i64).to_string(),
            action,
        ])?;
        self.rows += 1;
        Ok(())
    }

    /// Flushed here rather than left to Drop, which swallows the error: a log
    /// that could not be written is worth an exit status, since the whole
    /// point of it is to be read afterwards.
    pub(crate) fn finish(mut self) -> Result<u64, Box<dyn Error>> {
        self.writer.flush()?;
        Ok(self.rows)
    }
}

/// Says on stderr that the log has something in it, and what it measured
/// against, since the three modes do not agree on that and a reader comparing
/// two logs of the same file would otherwise have to guess.
///
/// Only when a log was asked for: a run that did not ask has not changed how
/// it reports. And said even when the count is zero, because "no rows" is the
/// answer a reader wanted, a log that is only a header line reads like one
/// that failed to be written.
pub(crate) fn report_fill_log(rows: u64, args: &ArgumentFlags) {
    let destination = match args.fill_log.as_deref() {
        Some(path) if is_stderr(path) => "standard error".to_string(),
        Some(path) => format!("'{}'", path.display()),
        None => return,
    };
    let reference = match args.fill_mode {
        FillMode::Widest => "the widest record",
        _ => "the first record",
    };
    note(&format!(
        "csvdt: {rows} row(s) written to the fill log in {destination}, \
         measured against {reference}"));
}
