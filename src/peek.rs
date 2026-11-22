// --peek: the top of the input as a numbered table.

use std::error::Error;
use crate::cli::*;
use crate::note;

// A field as --peek shows it: one line, however many the file gave it.
//
// A quoted field may hold a newline, a carriage return or a tab, and each
// would put the table's own rows and columns somewhere they do not belong,
// a three-row table arriving as five, with the numbering above nothing. Shown
// as the two characters instead, which is what the field holds as far as a
// column number is concerned.
//
// Every other control character, and the marks that reorder text drawn
// right to left, is shown by its code point, as \u{1b}. The table goes to a
// terminal, and a field holding an escape sequence would otherwise be run by
// it: a file can recolour the screen, retitle the window, or, with a bidi
// override, show a field in an order other than the one it holds.
pub(crate) fn peek_field(field: &str) -> String {
    let mut shown = String::with_capacity(field.len());
    for character in field.chars() {
        match character {
            '\\' => shown.push_str("\\\\"),
            '\t' => shown.push_str("\\t"),
            '\r' => shown.push_str("\\r"),
            '\n' => shown.push_str("\\n"),
            _ if character.is_control() || is_bidi_control(character) => {
                shown.push_str(&format!("\\u{{{:x}}}", u32::from(character)));
            },
            _ => shown.push(character),
        }
    }
    shown
}

// Whether a character is one of Unicode's bidirectional formatting
// characters: the embeddings and overrides, the isolates, and the three
// marks. None of them draws anything; they change the order the characters
// around them are drawn in.
pub(crate) fn is_bidi_control(character: char) -> bool {
    matches!(character, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

// Numbers the columns above the rows, padded so each number sits over its
// own field.
//
// Padded by character count rather than by rendered width: a terminal draws
// an emoji two cells wide and a combining accent none at all, and no count
// available here is right for every terminal. Characters are at least a rule
// a reader can predict, and the help says which rule it is.
pub(crate) fn peek_table(rows: &[Vec<String>]) -> String {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    let numbers: Vec<String> = (0..width).map(|column| column.to_string())
                                         .collect();
    let shown: Vec<&Vec<String>> = std::iter::once(&numbers)
        .chain(rows.iter())
        .collect();
    // How wide each column has to be to hold everything in it. Rust's own
    // padding counts characters, which is the rule the help states.
    let held: Vec<usize> = (0..width)
        .map(|column| shown.iter()
             .filter_map(|row| row.get(column))
             .map(|field| field.chars().count())
             .max()
             .unwrap_or(0))
        .collect();

    let mut out = String::new();
    for row in &shown {
        let mut line = String::new();
        for (column, wanted) in held.iter().enumerate() {
            let field = row.get(column).map(String::as_str).unwrap_or("");
            line.push_str(&format!("{field:wanted$}  "));
        }
        // Trailing spaces are invisible until something copies them out.
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

// Reads what --peek shows: the header when there is one, and the first data
// row. Nothing further is read, so a huge file costs the first record of it.
pub(crate) fn peek_stream<R: std::io::Read>(
    mut reader: csv::Reader<R>,
    args: &ArgumentFlags,
) -> Result<String, Box<dyn Error>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    // -H and -p mean here what they mean everywhere else: -H says the file
    // has a header, which the reader then takes as one rather than as data,
    // and -p asks for it to be written. So -H alone shows the first *data*
    // row, which is the file's second line, and claiming -H over a file
    // that has no header shows the second line for the same reason it would
    // in a run, rather than inventing a rule for this mode alone.
    if args.has_headers && args.output_header {
        let headers = reader.headers()?;
        if !headers.is_empty() {
            rows.push(headers.iter().map(peek_field).collect());
        }
    }
    let mut record = csv::StringRecord::new();
    if reader.read_record(&mut record)? {
        rows.push(record.iter().map(peek_field).collect());
    }
    if rows.is_empty() {
        // Not an error: an empty input is not one anywhere else here, and a
        // status saying otherwise would be the surprise. Said rather than
        // left as an empty screen, which reads as a broken run.
        note("csvdt: nothing to peek at: the input holds no records");
        return Ok(String::new());
    }
    Ok(peek_table(&rows))
}
