//! Opening the input and setting up the CSV reader over it.

use std::error::Error;
use crate::platform;
use crate::cli::*;

/// How much of the input to look at when deciding whether it is UTF-16.
pub(crate) const ENCODING_PROBE_BYTES: usize = 512;

/// Refuses UTF-16 input up front, returning a reader over the untouched bytes
/// when it looks fine.
///
/// A byte-order mark settles it outright. Without one, pure-ASCII UTF-16 is
/// still recognisable because every second byte is NUL, and that case matters
/// more than it looks: a NUL is valid UTF-8, so nothing downstream objects to
/// it. Such a file read with --flexible would produce NUL-interleaved output
/// and exit 0, which this tool must not do.
///
/// The test is the full alternating pattern rather than merely "contains a
/// NUL", so a field that holds NUL bytes still passes through.
///
/// The probe takes what the first read that brings it to eight bytes gives,
/// rather than waiting for all of ENCODING_PROBE_BYTES: from a pipe fed a line
/// at a time, `tail -f log | csvdt -u0`, waiting would hold back every row
/// until 512 bytes had arrived. A file fills the whole probe in one read.
pub(crate) fn reject_utf16<R: std::io::Read>(mut input: R) -> Result<impl std::io::Read, Box<dyn Error>> {
    let mut probe = vec![0u8; ENCODING_PROBE_BYTES];
    let mut filled = 0;
    while filled < 8 {
        match input.read(&mut probe[filled..])? {
            0 => break,
            read => filled += read,
        }
    }
    probe.truncate(filled);

    let refuse = |label: &str| -> Box<dyn Error> {
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("input looks like {}, which this tool reads as text and \
                     cannot decode. Convert it first, e.g. \
                     `iconv -f UTF-16 -t UTF-8 in.csv > out.csv`", label)))
    };

    if probe.starts_with(&[0xFF, 0xFE]) {
        return Err(refuse("UTF-16LE (it starts with a byte-order mark)"));
    }
    if probe.starts_with(&[0xFE, 0xFF]) {
        return Err(refuse("UTF-16BE (it starts with a byte-order mark)"));
    }

    // Needs a few characters' worth before the pattern means anything.
    if probe.len() >= 8 {
        let paired = probe.len() / 2 * 2;
        if (1..paired).step_by(2).all(|i| probe[i] == 0) {
            return Err(refuse("UTF-16LE (every second byte is NUL, with no \
                               byte-order mark)"));
        }
        if (0..paired).step_by(2).all(|i| probe[i] == 0) {
            return Err(refuse("UTF-16BE (every second byte is NUL, with no \
                               byte-order mark)"));
        }
    }

    // Read::chain, so the bytes already consumed by the probe are put back.
    Ok(std::io::Read::chain(std::io::Cursor::new(probe), input))
}

/// Guarantees the input hands the CSV reader a final newline when '--comment'
/// is in use.
///
/// csv-core mishandles end-of-input inside a comment: its DFA treats
/// "in a comment" like "mid-record" and fabricates a record of one empty
/// field. A file whose last line is a comment with no trailing newline was
/// refused as ragged, or grew a phantom empty row under '--flexible', and a
/// comment ending in a lone \r stays in the comment the same way, since only
/// \n leaves that state. Feeding the reader one extra \n when the input did
/// not end with one closes the comment and costs an empty line, which the
/// reader skips. Active only when a comment character is set: no other
/// well-formed input misparses for want of a final newline. One malformed
/// shape does pay: an input that ends inside an unclosed quote takes the
/// fabricated \n into that field as data, so its parse differs from the
/// same input without '--comment'. Quote state is not visible from a Read
/// wrapper, and the input is already in the unclosed-quote territory KNOWN
/// LIMITS describes as mangled, so this is documented there, not defended.
///
/// The same only-\n rule bites harder mid-file: in a CR-only file (classic
/// Mac line endings), \r ends every data record but never a comment, so the
/// first comment line runs on through all the rows that follow. This shim
/// cannot help there: it acts at EOF, and rewriting \r mid-stream needs
/// the quote state a Read wrapper cannot see (a \r inside a quoted field is
/// data). Documented in KNOWN LIMITS, fixable only in csv-core's DFA.
pub(crate) struct EndInNewline<R> {
    /// The input being read.
    input: R,
    /// Whether a comment character is set, which is when the newline is owed.
    active: bool,
    /// Whether any byte has been read at all: empty input must stay empty
    /// rather than become one blank line.
    seen: bool,
    /// The last byte read, which says whether the input ended on a newline.
    last: u8,
    /// Whether the newline has been handed over already.
    appended: bool,
}

/// Wraps INPUT so it ends in a newline when a comment character is set.
pub(crate) fn end_in_newline<R: std::io::Read>(input: R, args: &ArgumentFlags) -> EndInNewline<R> {
    EndInNewline {
        input,
        active: args.read_as_comment.is_some(),
        seen: false,
        last: 0,
        appended: false,
    }
}

impl<R: std::io::Read> std::io::Read for EndInNewline<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = self.input.read(buffer)?;
        if count > 0 {
            self.seen = true;
            self.last = buffer[count - 1];
            return Ok(count);
        }
        if self.active && self.seen && self.last != b'\n'
            && !self.appended && !buffer.is_empty()
        {
            self.appended = true;
            buffer[0] = b'\n';
            return Ok(1);
        }
        Ok(0)
    }
}

/// The CSV reader as the options describe it: header, width, trimming,
/// delimiter, quote and comment character.
pub(crate) fn configure_reader(args: &ArgumentFlags) -> csv::ReaderBuilder {
    let mut reader_builder = csv::ReaderBuilder::new();
    // --peek reads flexibly whatever was asked for: it writes no CSV, so a
    // record of a different width is a row of a different width in a table
    // and nothing more. Refusing it would turn away the reader most likely
    // to be looking, somebody whose file is ragged and who wants to see
    // where. The refusal exists to stop uneven records reaching an output
    // that claims to be CSV, and there is no such output here.
    reader_builder
        .has_headers(args.has_headers)
        .flexible(args.flexible_record || args.peek);

    reader_builder.trim(args.whitespace_trim);

    if let Some(value) = args.input_delimiter {
        reader_builder.delimiter(value as u8);
    } else {
        reader_builder.delimiter(b',');
    }

    if args.input_quotes {
        reader_builder.quote(b'\'');
    } else {
        reader_builder.quote(b'"');
    }

    if let Some(value) = args.read_as_comment {
        reader_builder.comment(Some(value as u8));
    }

    reader_builder
}

/// Whether a path can be opened and read a second time and give the same thing
/// again. A regular file can; a fifo cannot, and neither can the pipe that
/// process substitution puts behind a filename. Reading one of those twice
/// drains it on the counting pass and leaves the real pass with nothing, or,
/// for a fifo with no further writer, waits for one that never comes.
pub(crate) fn is_rereadable(path: &std::path::Path) -> bool {
    // Follows symlinks deliberately: /dev/stdin naming a redirected regular
    // file is as re-readable as the file itself.
    std::fs::metadata(path)
        .map(|data| data.file_type().is_file())
        .unwrap_or(false)
}

/// Opens a named input, saying which path failed and why. The bare io error says
/// neither: "No such file or directory (os error 2)" does not say which of a
/// script's paths that was. A directory is refused here too, rather than at
/// the first read, where it would surface as "Is a directory" from somewhere
/// inside the CSV reader, also without naming it.
pub(crate) fn open_input(path: &std::path::Path) -> Result<std::fs::File, Box<dyn Error>> {
    let named = |error: std::io::Error| -> Box<dyn Error> {
        Box::new(std::io::Error::new(
            error.kind(), format!("cannot read '{}': {error}", path.display())))
    };

    let a_directory = || -> Box<dyn Error> {
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("cannot read '{}': it is a directory, not a file", path.display())))
    };

    match std::fs::File::open(path) {
        // Unix opens a directory quite happily and fails at the first read
        // instead, as "Is a directory" from wherever that read is made.
        Ok(file) => match file.metadata().map_err(named)?.is_dir() {
            true => Err(a_directory()),
            // Asked here rather than at the one call site that reads a file,
            // so the counting pass of '--flexible=widest' and the copy that
            // feeds its spool are covered by the same question.
            false => platform::refuse_input_that_is_also_the_output(path, &file).map(|()| file),
        },
        // Windows refuses to open one at all, and says only that access was
        // denied, which does not mention the reason. Asked here rather than
        // before the open, so the ordinary case still costs one call.
        Err(error) => match std::fs::metadata(path).map(|data| data.is_dir()) {
            Ok(true) => Err(a_directory()),
            _ => Err(named(error)),
        },
    }
}

/// Counts the fields of the widest record in the file, so a following pass can
/// pad every row out to it. Reads with has_headers(false) so the header line is
/// measured too, it may itself be the short one.
///
/// Uses ByteRecord rather than StringRecord: counting fields needs no text, so
/// this pass does no UTF-8 validation and leaves reporting a bad encoding to the
/// real pass, which can point at it properly.
pub(crate) fn scan_widest_record(args: &ArgumentFlags, path: &std::path::Path) -> Result<usize, Box<dyn Error>> {
    let mut builder = configure_reader(args);
    builder.has_headers(false).flexible(true);
    let mut reader = builder.from_reader(
        end_in_newline(reject_utf16(open_input(path)?)?, args));

    let mut record = csv::ByteRecord::new();
    let mut widest = 0;
    while reader.read_byte_record(&mut record)? {
        widest = widest.max(record.len());
    }
    Ok(widest)
}

/// The CSV reader has no error for a quote that is opened and never closed: it
/// reads everything after it, newlines included, as one field, so the file
/// arrives as a record with too few fields. That is the same error a ragged
/// file gives, and the documented remedy for raggedness, --flexible,
/// accepts the mangled record rather than reporting it, so the message that
/// misdirects there is the one to say what else to look for.
pub(crate) fn explain_unequal_lengths(error: csv::Error) -> Box<dyn Error> {
    if !matches!(error.kind(), csv::ErrorKind::UnequalLengths { .. }) {
        return error.into();
    }

    Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("{error}\nIf the file does not look ragged, check for a quote \
                 that is opened and never closed: everything after it, newlines \
                 included, is read as one field, which arrives here as a record \
                 with too few fields. '--flexible' would accept that record \
                 rather than report it."),
    ))
}
