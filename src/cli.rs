//! The command line: the options, their help, and the flags a run is given.

use std::error::Error;
use jiff::tz::Offset;
use clap::builder::PossibleValue;
use clap::crate_version;
use crate::man::without_marks;
use crate::man;
use crate::zone::*;
use crate::{fatal, answer, answered, say_as_clap_would, without_trailing_padding};

/// What --flexible does about records narrower than the rest.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub(crate) enum FillMode {
    /// Bare -f: let uneven records through untouched.
    #[default]
    Off,
    /// -f=fix: pad out to the first record's width in a single pass, which is
    /// all a stream allows.
    FirstRecord,
    /// -f=widest: read the file once to find the widest record, then pad
    /// everything to that. Needs a file it can read twice.
    Widest,
}

/// How --quote has the writer quote fields. Resolved from the option's word
/// once, at startup, so the checks made on every written field compare an enum
/// rather than lowercasing and matching the word again.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub(crate) enum Quoting {
    /// Every field quoted.
    Always,
    /// A field quoted only where it has to be, the default.
    #[default]
    Necessary,
    /// No field quoted, even where that writes CSV that cannot be read back.
    Never,
    /// Every field quoted that is not a number.
    NonNumeric,
}

impl Quoting {
    /// The csv crate's setting for this style.
    pub(crate) fn style(self) -> csv::QuoteStyle {
        match self {
            Quoting::Always => csv::QuoteStyle::Always,
            Quoting::Necessary => csv::QuoteStyle::Necessary,
            Quoting::Never => csv::QuoteStyle::Never,
            Quoting::NonNumeric => csv::QuoteStyle::NonNumeric,
        }
    }
}

/// The one action a run carries out on each row, with what it needs beyond
/// the column it reads, which is ArgumentFlags::selected_column. clap refuses
/// two actions together, so one value says which it is.
#[derive(Default, Debug, Clone)]
pub(crate) enum Action {
    /// No action: the rows are written as they were read, reformatted.
    #[default]
    Passthrough,
    /// -s/--split: the timestamp's date and time, as two columns.
    Split,
    /// -r/--rfc3339: epoch seconds to RFC3339.
    ToRfc3339,
    /// -u/--utc.
    ToUtc,
    /// -l/--local, in the zone resolved at startup; None leaves the zone to
    /// chrono, which reads a POSIX rule in TZ itself.
    ToLocal(Option<chrono_tz::Tz>),
    /// -o/--offset, to the offset given.
    ToOffset(Offset),
    /// -d/--duration: from the previous row's timestamp, or, with a second
    /// column, between the two columns of the same row.
    Duration {
        /// The second column of the two-column form.
        second: Option<usize>,
    },
    /// --remove: the columns to drop, highest index first, so dropping one
    /// never shifts another still to come.
    Remove(Vec<usize>),
    /// -a/--arrange: the columns to put first, in the order given, and the
    /// same set sorted, so whether a column was named is a binary search.
    Arrange {
        /// The columns named, in the order given.
        order: Vec<usize>,
        /// The same columns, sorted.
        listed: Vec<usize>,
    },
}

/// Where -i puts a converted column relative to the column it is given.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub(crate) enum InsertPosition {
    /// Ahead of the column.
    Before,
    /// Over the column.
    Replace,
    /// Behind the column, the default.
    #[default]
    After,
}

/// Everything a run was asked to do, read from the command line once, at
/// startup, so the row loop works with resolved values only.
#[derive(Default, Debug, Clone)]
pub(crate) struct ArgumentFlags {
    /// The input file, FILE. None reads standard input.
    pub(crate) input_file: Option<std::path::PathBuf>,
    /// -H/--has-header: the first record is a header, not data.
    pub(crate) has_headers: bool,
    /// --peek: show the top of the input as a numbered table instead of a run.
    pub(crate) peek: bool,
    /// -p/--print-header: write the header to the output.
    pub(crate) output_header: bool,
    /// -f/--flexible in any mode: records may differ in width.
    pub(crate) flexible_record: bool,
    /// What --flexible does about a narrow record: nothing, or pad it.
    pub(crate) fill_mode: FillMode,
    /// The one action asked for, or Passthrough when none was.
    pub(crate) action: Action,
    /// The column the action reads; the first of two for -d.
    pub(crate) selected_column: usize,
    /// -i/--insert's column, where the result goes. None puts it at the action's own.
    pub(crate) insert_column: Option<i64>,
    /// -i/--insert's position word: before, replace or, by default, after.
    pub(crate) insert_position: InsertPosition,
    /// --round-seconds: round -d's result to whole seconds.
    pub(crate) round_seconds: bool,
    /// --trim: the whitespace the reader takes off fields and headers.
    pub(crate) whitespace_trim: csv::Trim,
    /// --read-delimiter: the delimiter read, when not a comma.
    pub(crate) input_delimiter: Option<char>,
    /// --single-quote: the reader quotes with ' rather than ".
    pub(crate) input_quotes: bool,
    /// --comment: the character that starts a comment line when reading.
    pub(crate) read_as_comment: Option<char>,
    /// --fill-log's destination; "-" is standard error.
    pub(crate) fill_log: Option<std::path::PathBuf>,
    /// Whether each row is flushed as it is written, which is done when standard output is a terminal.
    pub(crate) flush_each_record: bool,
    /// --print-delimiter: the delimiter written, when not a comma.
    pub(crate) output_delimiter: Option<char>,
    /// --quote: how the writer quotes fields.
    pub(crate) output_quotes: Quoting,
}

/// The words --trim, --quote, --flexible and -i's position accept, each beside
/// what it stands for. All may be abbreviated, and all are matched
/// case-insensitively, from this one table each: clap's list of accepted values
/// and the meaning of each are both read from it, so the two cannot drift
/// apart. A second hand-written copy at the point of use could: if one copy
/// lowercased and the other did not, '--trim ALL' would pass clap, match
/// nothing, and silently mean '--trim none'.
pub(crate) type Choices<T> = [(&'static str, T)];

/// --trim's words.
pub(crate) const TRIMS: [(&str, csv::Trim); 4] = [
    ("all", csv::Trim::All),
    ("fields", csv::Trim::Fields),
    ("headers", csv::Trim::Headers),
    ("none", csv::Trim::None),
];
/// --quote's words.
pub(crate) const QUOTINGS: [(&str, Quoting); 4] = [
    ("always", Quoting::Always),
    ("necessary", Quoting::Necessary),
    ("never", Quoting::Never),
    ("nonnumeric", Quoting::NonNumeric),
];
/// -i/--insert's position word. Not a clap value_parser: -i takes
/// "column,position" as one value, so the word is parsed out of it by hand.
pub(crate) const POSITIONS: [(&str, InsertPosition); 3] = [
    ("before", InsertPosition::Before),
    ("replace", InsertPosition::Replace),
    ("after", InsertPosition::After),
];
/// --flexible's modes.
pub(crate) const FLEXIBLE_MODES: [(&str, FillMode); 3] = [
    ("keep", FillMode::Off),
    ("fix", FillMode::FirstRecord),
    ("widest", FillMode::Widest),
];

/// Every abbreviation of these values clap should accept: each prefix that only
/// one of them has. A prefix two of them share is left out, so 'n' is refused
/// rather than resolved to whichever of never/necessary/nonnumeric comes first.
pub(crate) fn abbreviated_values<T>(choices: &Choices<T>) -> Vec<PossibleValue> {
    let unambiguous = |prefix: &str| {
        choices.iter().filter(|(full, _)| full.starts_with(prefix)).count() == 1
    };

    let mut possible = Vec::new();
    for (value, _) in choices {
        possible.push(PossibleValue::new(*value));
        possible.extend(
            (1..value.len())
                .map(|length| &value[..length])
                .filter(|prefix| unambiguous(prefix))
                .map(|prefix| PossibleValue::new(prefix).hide(true)),
        );
    }
    possible
}

/// What a given value stands for. Only prefixes of exactly one full form are
/// accepted above, so matching on the prefix cannot be ambiguous. Asked once
/// per option at startup, never per row.
pub(crate) fn resolve_value<T: Copy>(choices: &Choices<T>, given: &str) -> Option<T> {
    let given = given.to_lowercase();
    let mut matching = choices.iter().filter(|(full, _)| full.starts_with(&given));
    match (matching.next(), matching.next()) {
        (Some(&(_, meaning)), None) if !given.is_empty() => Some(meaning),
        _ => None,
    }
}

/// Reads the single character an option like --read-delimiter takes.
///
/// The CSV reader and writer work in bytes, so the value has to be one ASCII
/// character, since a multi-byte one cannot be a byte. The check counts
/// characters, so a single non-ASCII one is reported as such. An
/// empty value is refused rather than quietly leaving the default in place: it
/// is far more often an unset shell variable than a request for the default,
/// and silently reading with the wrong delimiter is the kind of thing that is
/// noticed several steps later, if at all.
pub(crate) fn parse_single_ascii_char(option: &str, value: &str) -> Result<char, Box<dyn Error>> {
    let invalid = |message: String| -> Box<dyn Error> {
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, message))
    };
    let mut characters = value.chars();
    match (characters.next(), characters.next()) {
        (Some(character), None) if character.is_ascii() => Ok(character),
        (Some(character), None) => Err(invalid(format!(
            "'{}' expects an ASCII character, but got '{}'; the CSV reader \
             and writer work in bytes, so a multi-byte character cannot be \
             used as a {}", option, character,
            // The message serves three options, and --comment's character
            // is not a delimiter.
            if option == "--comment" { "comment character" }
            else { "delimiter" }))),
        (None, _) => Err(invalid(format!(
            "'{}' was given an empty value, which is usually an unset shell \
             variable. Give the character you meant, or leave the option out \
             to keep the default", option))),
        _ => Err(invalid(format!(
            "'{}' expects a single character, but got \"{}\"", option, value))),
    }
}

/// Explains a column number that will not parse, naming the option and showing
/// what it was given. Rust's own ParseIntError ("invalid digit found in
/// string") names neither the option nor the value, so `-a "1, 2"` would report
/// a problem with a digit without mentioning the space, the list, or --arrange.
pub(crate) fn invalid_column(option: &str, value: &str, signed: bool) -> Box<dyn Error> {
    invalid_column_of(option, value, value, signed)
}

/// The same, for VALUE that failed as one part of WHOLE, which is what the
/// message quotes: -d's "1,0," reported as "0," would show a value nobody
/// typed. The explanation still comes from the part that failed.
pub(crate) fn invalid_column_of(option: &str, value: &str, whole: &str, signed: bool) -> Box<dyn Error> {
    let detail = if value.is_empty() && whole.is_empty() {
        "was given an empty column number. A stray comma leaves one empty, as \
         in \"1,\" or \"0,,2\", and so does an unset shell variable.".to_string()
    } else if value.is_empty() {
        format!("was given \"{whole}\", which leaves a column number empty. A \
                 stray comma does that, and so does an unset shell variable.")
    } else if value.chars().any(char::is_whitespace) {
        format!("was given \"{whole}\", which contains a space. Write it \
                 without one, and a list of columns as 0,2 rather than 0, 2.")
    } else if !signed && value.starts_with('-') {
        format!("was given \"{whole}\". Columns are counted from 0 upwards, so \
                 there is no negative one -- the first column is 0.")
    } else if value.chars().all(|character| character.is_ascii_digit()) {
        format!("was given \"{whole}\", which is too large to be a column \
                 number.")
    } else {
        format!("was given \"{whole}\", which is not a column number. Columns \
                 are named by position, counted from 0.")
    };

    Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData, format!("'{option}' {detail}")))
}

/// Reads one column number for OPTION, explaining a value that is not one.
pub(crate) fn parse_column(option: &str, value: &str) -> Result<usize, Box<dyn Error>> {
    value.parse::<usize>().map_err(|_| invalid_column(option, value, false))
}

/// -i/--insert's column may be negative, since it is clamped into the row rather
/// than refused, so the message for it does not rule a minus sign out.
pub(crate) fn parse_signed_column(option: &str, value: &str) -> Result<i64, Box<dyn Error>> {
    value.parse::<i64>().map_err(|_| invalid_column(option, value, true))
}

/// Each column of a "0,2,3" list, reported one value at a time rather than as
/// the whole list, since one value is what failed.
pub(crate) fn parse_column_list(option: &str, value: &str) -> Result<Vec<usize>, Box<dyn Error>> {
    value.split(',').map(|part| parse_column(option, part)).collect()
}

/// The first column named more than once in a list, if any. Both options that
/// take a list refuse a repeat: for -a the list is the new order, so a repeat
/// would duplicate a field and leave the row wider than it came in, and for
/// --remove it is a miscount, a list that does not say what it looks like.
pub(crate) fn repeated_column(columns: &[usize]) -> Option<usize> {
    let mut sorted = columns.to_vec();
    sorted.sort_unstable();
    sorted
        .windows(2)
        .find(|pair| pair[0] == pair[1])
        .map(|pair| pair[0])
}

/// The refusal of a column OPTION names twice, saying why with BECAUSE.
pub(crate) fn repeated_column_error(option: &str, column: usize, because: &str) -> Box<dyn Error> {
    Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("'{option}' names column {column} more than once. Each column \
                 may appear at most once, {because}"),
    ))
}

/// The range real UTC offsets occupy: -12:00 (Baker Island) to +14:00 (the
/// Line Islands). No place has ever used anything outside it.
pub(crate) const MIN_TARGET_OFFSET_SECONDS: i32 = -12 * 3600;
/// The highest of that range.
pub(crate) const MAX_TARGET_OFFSET_SECONDS: i32 = 14 * 3600;

/// Parses a standalone UTC offset for -o/--offset: Z in either case, or a
/// sign, two digits of hours and two of minutes with or without a colon
/// between them ("+02:00", "-0600"), which is the offset a timestamp carries,
/// with the "+HHMM" leniency the reader allows. Nothing else is taken, not
/// even surrounding blanks: an offset is read by its shape here rather than
/// glued onto a timestamp and parsed with it, which let ".5Z" through as a
/// fraction of a second followed by Z.
///
/// Offsets outside the range real places use are rejected. A timestamp may
/// carry anything up to +/-23:59, so without this a typo like "+15:00" for
/// "+05:00" would be honoured and produce a plausible-looking timestamp
/// carrying an offset that cannot exist, exactly the sort of quiet error
/// this tool otherwise goes out of its way not to make. Only the target
/// that was asked for is checked; offsets found in the data are left alone, since
/// those are the input's business rather than a mistake in the command.
pub(crate) fn parse_offset(value: &str) -> Result<Offset, Box<dyn Error>> {
    let malformed = || -> Box<dyn Error> {
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "'--offset' expects a UTC offset like +02:00, -0600, or Z"))
    };

    let seconds = match value {
        "Z" | "z" => 0,
        _ => {
            let (sign, rest) = match value.split_at_checked(1) {
                Some(("+", rest)) => (1, rest),
                Some(("-", rest)) => (-1, rest),
                _ => return Err(malformed()),
            };
            let (hours, minutes) = match rest.split_at_checked(2) {
                Some((hours, minutes)) => (hours, minutes.strip_prefix(':').unwrap_or(minutes)),
                None => return Err(malformed()),
            };
            let two_digits = |text: &str| -> Option<i32> {
                match text.len() == 2 && text.bytes().all(|byte| byte.is_ascii_digit()) {
                    true => text.parse().ok(),
                    false => None,
                }
            };
            match (two_digits(hours), two_digits(minutes)) {
                (Some(hours), Some(minutes)) if hours < 24 && minutes < 60 => {
                    sign * (hours * 3600 + minutes * 60)
                },
                _ => return Err(malformed()),
            }
        },
    };

    if !(MIN_TARGET_OFFSET_SECONDS..=MAX_TARGET_OFFSET_SECONDS).contains(&seconds) {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("'--offset' target {} is not a real UTC offset; \
                     they run from -12:00 to +14:00", value))));
    }

    Offset::from_seconds(seconds).map_err(|_| malformed())
}

/// Reads the position word from -i's second value, which may be abbreviated.
/// It becomes an InsertPosition here, once, so a typo is a clean startup error
/// and the row loop works with the enum alone.
pub(crate) fn parse_position_word(word: &str) -> Result<InsertPosition, Box<dyn Error>> {
    resolve_value(&POSITIONS, word)
        .ok_or_else(|| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "'--insert' expects a position of before, after, or replace"))
                as Box<dyn Error>
        })
}

/// The parser for an option that names a file: the value as the operating
/// system gave it, empty or not, and whether or not it is UTF-8.
pub(crate) fn path_value() -> impl clap::builder::TypedValueParser<Value = std::path::PathBuf> {
    use clap::builder::TypedValueParser;
    clap::builder::OsStringValueParser::new().map(std::path::PathBuf::from)
}

/// The format version reported on --list-options' first line. Bump it only if
/// the meaning of a field changes; adding a comment line or an option does not,
/// since consumers are told to skip comments and read fields positionally.
pub(crate) const OPTION_LIST_VERSION: u32 = 1;

/// Renders --list-options' output by asking clap what it was given, so the list
/// cannot disagree with the options that actually exist. Nothing here names an
/// option: a new .arg() appears in the output with no change to this function.
pub(crate) fn option_list(command: &clap::Command) -> String {
    // build() is what adds the automatic --help and --version, which are as
    // real to a caller as the ones declared by hand.
    let mut command = command.clone();
    command.build();

    let mut out = format!("# csvdt-option-list {}\n", OPTION_LIST_VERSION);
    out.push_str("# short\tlong\tvalue\tseparator\tvalue-name\n");

    for arg in command.get_arguments() {
        // Positionals have neither form and so cannot be completed as options;
        // hidden arguments are deliberately unadvertised, and stay that way.
        if (arg.get_short().is_none() && arg.get_long().is_none()) || arg.is_hide_set() {
            continue;
        }

        let range = arg.get_num_args().unwrap_or_else(|| 0.into());
        let (value, separator, names) = if !range.takes_values() {
            // A flag. Several of ours carry a value_name anyway, which would
            // be misleading here, so it is dropped rather than reported.
            ("none", "none", String::new())
        } else {
            (
                if range.min_values() == 0 { "optional" } else { "required" },
                if arg.is_require_equals_set() { "equals" } else { "either" },
                arg.get_value_names()
                    .unwrap_or_default()
                    .iter()
                    .map(|name| name.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        };

        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            arg.get_short().map(|c| format!("-{}", c)).unwrap_or_default(),
            arg.get_long().map(|l| format!("--{}", l)).unwrap_or_default(),
            value,
            separator,
            names,
        ));
    }

    out
}

/// The one-line form of a help text: the sentence it opens with.
///
/// -h and --help are one text at two depths rather than two texts. A second
/// corpus of one-liners would be a second thing to keep true, and the first
/// thing to go stale, which is the argument this program already makes for
/// generating its manual page rather than keeping one by hand. Every file in
/// src/help opens with a sentence saying what the option is; the summary is
/// that sentence, taken rather than written again.
///
/// A sentence ends at a full stop outside brackets. Inside them it does not:
/// four entries carry an "(e.g. -o1,-06:00)" whose stop would cut the line in
/// half, and the syntax it shows is the part a reader of the short help most
/// needs. Reflowed, because the corpus is wrapped for the long form and the
/// short one is a single sentence for clap to wrap to the terminal.
pub(crate) fn help_summary(text: &str) -> String {
    let flowed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut depth = 0usize;
    let mut end = flowed.len();
    for (at, character) in flowed.char_indices() {
        match character {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            '.' if depth == 0
                && flowed[at + 1..].chars().next().is_none_or(|c| c == ' ') =>
            {
                end = at + 1;
                break;
            }
            _ => {}
        }
    }
    wrapped(&flowed[..end])
}

/// The column the whole help corpus is written to.
pub(crate) const HELP_COLUMNS: usize = 64;

/// TEXT folded to that column.
///
/// clap only wraps text with its wrap_help feature, which this program does
/// not take: the corpus is folded by hand instead, so that what a reader sees
/// is what was written and a table stays a table. A summary is made rather
/// than written, so it has to be folded here or not at all, left flowing,
/// one ran a single line of -h out to 211 columns.
///
/// A word naming an option is joined to the word before it, and the pair is
/// what gets placed. In the description column a line beginning '--' reads as
/// the start of a new entry, to someone skimming and to anything parsing the
/// output. Mending a bad break after making it, by moving the word before it
/// down until the line no longer starts with '--', would leave '--version or
/// ...' at a line start where two option names fall together, or push lines
/// past 64 columns by adding to a line already full. Joining first means
/// there is nothing to mend afterwards.
pub(crate) fn wrapped(text: &str) -> String {
    let mut atoms: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match atoms.last_mut() {
            Some(atom) if word.starts_with("--") => {
                atom.push(' ');
                atom.push_str(word);
            }
            _ => atoms.push(word.to_string()),
        }
    }

    let mut lines: Vec<String> = Vec::new();
    for atom in atoms {
        let length = atom.chars().count();
        match lines.last_mut() {
            Some(line) if line.chars().count() + 1 + length <= HELP_COLUMNS => {
                line.push(' ');
                line.push_str(&atom);
            }
            _ => lines.push(atom),
        }
    }
    lines.join("\n")
}

/// The opening paragraph of TEXT, for the same reason: the short help wants
/// the introduction, and the paragraphs after it are reference material that
/// a reader asking for a summary did not ask for.
///
/// Keeps the newline the paragraph ends on, which is the blank line the help
/// template puts between the introduction and the usage line.
///
/// A paragraph ends at a blank line, which is not the same as ending at
/// "\n\n": git hands a Windows checkout the corpus with CRLF endings, where
/// the blank line is "\r\n\r\n" and the shorter pattern is nowhere in the
/// file, so -h would print the whole introduction, the durations paragraph
/// included.
pub(crate) fn first_paragraph(text: &str) -> &str {
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if line.trim().is_empty() {
            return &text[..at];
        }
        at += line.len();
    }
    text
}

/// The layout clap fills to print the help.
const TEMPLATE: &str = include_str!("./help/help_template.txt");
/// What csvdt is for: the opening of -h, all of it in --help and the manual page.
const ABOUT: &str = include_str!("./help/about.txt");
/// The help for the FILE argument.
const FILE: &str = include_str!("./help/file.txt");
/// The help for -H/--has-header.
const HAS_HEADER: &str = include_str!("./help/has_header.txt");
/// The help for --print-header.
const PRINT_HEADER: &str = include_str!("./help/print_header.txt");
/// The help for --flexible.
const FLEXIBLE: &str = include_str!("./help/flexible.txt");
/// The help for -i/--insert.
const POSITION: &str = include_str!("./help/position.txt");
/// The help for --trim.
const TRIM: &str = include_str!("./help/trim.txt");
/// The help for --read-delimiter.
const READ_DELIMITER: &str = include_str!("./help/read_delimiter.txt");
/// The help for --single-quote.
const SINGLE_QUOTES: &str = include_str!("./help/single_quotes.txt");
/// The help for --comment.
const COMMENT: &str = include_str!("./help/comment.txt");
/// The help for --fill-log.
const FILL_LOG: &str = include_str!("./help/fill_log.txt");
/// The help for --print-delimiter.
const PRINT_DELIMITER: &str = include_str!("./help/print_delimiter.txt");
/// The help for --quote.
const QUOTES: &str = include_str!("./help/quotes.txt");
/// The help for --split.
const SPLIT: &str = include_str!("./help/split.txt");
/// The help for -r/--rfc3339.
const RFC3339: &str = include_str!("./help/rfc3339.txt");
/// The help for -u/--utc.
const UTC: &str = include_str!("./help/utc.txt");
/// The help for --local.
const LOCAL: &str = include_str!("./help/local.txt");
/// The help for --duration.
const DURATION: &str = include_str!("./help/duration.txt");
/// The help for --round-seconds.
const ROUND_SECONDS: &str = include_str!("./help/round_seconds.txt");
/// The help for --offset.
const OFFSET: &str = include_str!("./help/offset.txt");
/// The help for --remove.
const REMOVE: &str = include_str!("./help/remove.txt");
/// The help for --arrange.
const ARRANGE: &str = include_str!("./help/arrange.txt");
/// The help for --list-options.
const LIST_OPTIONS: &str = include_str!("./help/list_options.txt");
/// The help for --peek.
const PEEK: &str = include_str!("./help/peek.txt");
/// The help for --generate-man.
const GENERATE_MAN: &str = include_str!("./help/generate_man.txt");
/// Not help text: roff, for the page only. See insert_man_sections.
const MAN_SUPPLEMENT: &str = include_str!("./man/supplement.roff");
// The one trailing section clap allows, composed from the four files.
// A macro rather than a const, since concat! wants literals, and
// include_str! and this both expand to one. EXAMPLES comes first, where
// a man page puts it: after the options, before the reference material.
/// The four trailing help files joined, as one literal.
macro_rules! trailing {
    () => {
        concat!(
            include_str!("./help/examples.txt"),
            include_str!("./help/formats.txt"),
            include_str!("./help/limits.txt"),
            include_str!("./help/exit_status.txt"),
        )
    };
}
/// The trailing sections of --help as the manual page takes them.
const TRAILING: &str = trailing!();
/// The same, plus where to go next. The page does not get this one: it
/// ends in a SEE ALSO that says it already, and better, since a page is
/// read where the other pages are.
const TRAILING_LONG: &str = concat!(trailing!(), include_str!("./help/documentation.txt"));
/// What -h ends with instead: that it is a summary, and what the long
/// form adds. clap already appends "(see more with '--help')" to the help
/// entry itself, which says where to look but not what is there.
const MORE_HELP: &str = include_str!("./help/more_help.txt");
/// The headings those three files open with. A man page wants them as
/// sections rather than as capitalised lines inside one, and taking them
/// from here rather than matching on shape keeps a heading from being
/// invented out of ordinary prose, KNOWN LIMITS appears mid-sentence
/// three times in the option help. A test renders the page and looks for
/// each of these, so a file renamed without this list is a failure rather
/// than a section quietly lost.
const TRAILING_HEADINGS: [&str; 4] =
    ["EXAMPLES", "TIMESTAMP FORMATS", "KNOWN LIMITS", "EXIT STATUS"];

/// The command line csvdt accepts, with the help for each option. The manual
/// page and --list-options are rendered from it too, so all three describe
/// this build.
fn build_command() -> clap::Command {
    // The bundled IANA database version is part of the tool's identity: it
    // determines every -l/--local result, so output can be tied back to a
    // known ruleset.
    //
    // Published builds also carry semver build metadata identifying the exact
    // build, e.g. "1.0.1-pre+2026.07.a1b2c3d", set by the release workflow
    // through CSVDT_BUILD_METADATA. It is metadata rather than a version bump
    // because a scheduled rebuild usually changes only the time zone database
    // or the compiler, not this program; semver ignores anything after '+'
    // when comparing versions, so it distinguishes builds without claiming a
    // release that didn't happen. A local build simply has none.
    // Leaked so clap can hold a &'static str; it is a single allocation made
    // once at startup and lives for the whole run either way.
    let version: &'static str = Box::leak(
        format!(
            "{}{} (IANA tzdata {})",
            crate_version!(),
            match option_env!("CSVDT_BUILD_METADATA") {
                Some(metadata) => format!("+{}", metadata),
                None => String::new(),
            },
            chrono_tz::IANA_TZDB_VERSION
        )
        .into_boxed_str(),
    );

    clap::Command::new("csvdt")
        // -h introduces the program; --help goes on to say why a duration is
        // never reported in months, which is reference material a reader
        // asking for a summary did not ask for.
        .about(first_paragraph(ABOUT))
        .long_about(ABOUT)
        .author("Michael A Jones <yardquit@pm.me>")
        .version(version)
        .help_template(TEMPLATE)
        // The long form is folded by hand at 64 columns and printed at an
        // indent of ten, so it has never been wider than 74. The short form
        // is a table, and clap fills a table to the terminal: one sentence
        // beside a 36-column option spec ran a single line out to 211, which
        // is unreadable on a wide terminal and unreadable in a different way
        // once a narrow one folds it. A ceiling, not a width, a terminal
        // narrower than this still gets its own.
        .max_term_width(80)
        .after_help(MORE_HELP)
        .after_long_help(TRAILING_LONG)
        .arg(
            clap::Arg::new("input_file")
                .action(clap::ArgAction::Set)
                // A path rather than text: a file name need not be UTF-8.
                // Not clap's own PathBuf parser, which refuses an empty
                // value with a message of its own before the clearer one
                // below can be given.
                .value_parser(path_value())
                .value_name("FILE")
                .help(help_summary(FILE))
                .long_help(FILE)
                .num_args(1)
                .required(false)
        )

        .arg(
            clap::Arg::new("has_header")
                .action(clap::ArgAction::SetTrue)
                .value_name("has-header")
                .help(help_summary(HAS_HEADER))
                .long_help(HAS_HEADER)
                .short('H')
                .long("has-header")
                .required(false)
        )

        .arg(
            clap::Arg::new("print_header")
                .action(clap::ArgAction::SetTrue)
                .value_name("print-header")
                .help(help_summary(PRINT_HEADER))
                .long_help(PRINT_HEADER)
                .short('p')
                .long("print-header")
                .required(false)
                .requires("has_header")
        )

        .arg(
            clap::Arg::new("flexible")
                .action(clap::ArgAction::Set)
                .value_name("keep|fix|widest")
                .help(help_summary(FLEXIBLE))
                .long_help(FLEXIBLE)
                .short('f')
                .long("flexible")
                .num_args(0..=1)
                // Attached form only (-f=fix, --flexible=fix). A separated
                // value would let "csvdt -f data.csv" read the path as the
                // mode, the same trap -d avoids.
                .require_equals(true)
                .default_missing_value("keep")
                .ignore_case(true)
                .value_parser(abbreviated_values(&FLEXIBLE_MODES))
                .required(false)
        )

        .arg(
            clap::Arg::new("position")
                .action(clap::ArgAction::Set)
                .value_name("column,position")
                .help(help_summary(POSITION))
                .long_help(POSITION)
                .short('i')
                .long("insert")
                .num_args(1)
                .allow_hyphen_values(true)
                .required(false)
                .requires("actions_req"),
        )
        // The actions, of which a run takes one: clap refuses any two
        // together, with the same message it gives for any other conflict.
        .group(clap::ArgGroup::new("action")
             .args([
                 "action_split",
                 "action_to_rfc3339",
                 "action_to_utc",
                 "action_to_local",
                 "action_duration",
                 "action_offset",
                 "action_remove",
                 "action_arrange",
             ])
             .multiple(false),
        )
        // The actions that write a converted column, which -i places.
        .group(clap::ArgGroup::new("actions_req")
             .args([
                 "action_split",
                 "action_to_rfc3339",
                 "action_to_utc",
                 "action_to_local",
                 "action_duration",
                 "action_offset",
             ])
             .multiple(true),
        )

        .arg(
            clap::Arg::new("action_to_rfc3339")
                .action(clap::ArgAction::Set)
                .value_name("num")
                .help(help_summary(RFC3339))
                .long_help(RFC3339)
                .short('r')
                .long("rfc3339")
                .num_args(1)
                .required(false)
        )

        .arg(
            clap::Arg::new("action_to_utc")
                .action(clap::ArgAction::Set)
                .value_name("num")
                .help(help_summary(UTC))
                .long_help(UTC)
                .short('u')
                .long("utc")
                .num_args(1)
                .required(false)
        )

        .arg(
            clap::Arg::new("action_to_local")
                .action(clap::ArgAction::Set)
                .value_name("num")
                .help(help_summary(LOCAL))
                .long_help(LOCAL)
                .short('l')
                .long("local")
                .num_args(1)
                .required(false)
        )

        .arg(
            clap::Arg::new("action_offset")
                .action(clap::ArgAction::Set)
                .value_name("column,offset")
                .help(help_summary(OFFSET))
                .long_help(OFFSET)
                .short('o')
                .long("offset")
                .num_args(1)
                .required(false)
        )

        .arg(
            clap::Arg::new("action_split")
                .action(clap::ArgAction::Set)
                .value_name("num")
                .help(help_summary(SPLIT))
                .long_help(SPLIT)
                .short('s')
                .long("split")
                .num_args(1)
                .required(false)
        )

        .arg(
            clap::Arg::new("action_duration")
                .action(clap::ArgAction::Set)
                .value_name("num[,num]")
                .help(help_summary(DURATION))
                .long_help(DURATION)
                .short('d')
                .long("duration")
                .num_args(1)
                .required(false)
        )

        .arg(
            clap::Arg::new("round_seconds")
                .action(clap::ArgAction::SetTrue)
                .help(help_summary(ROUND_SECONDS))
                .long_help(ROUND_SECONDS)
                .long("round-seconds")
                .required(false)
                .requires("action_duration")
        )

        .arg(
            clap::Arg::new("action_remove")
                .action(clap::ArgAction::Set)
                .value_name("num[,num...]")
                .help(help_summary(REMOVE))
                .long_help(REMOVE)
                .long("remove")
                .num_args(1)
                .required(false)
        )

        .arg(
            clap::Arg::new("action_arrange")
                .action(clap::ArgAction::Set)
                .value_name("num[,num...]")
                .help(help_summary(ARRANGE))
                .long_help(ARRANGE)
                .short('a')
                .long("arrange")
                .num_args(1)
                .required(false)
        )

        .arg(
            clap::Arg::new("trim")
                .action(clap::ArgAction::Set)
                .value_name("trim")
                .help(help_summary(TRIM))
                // Trimmed, because clap writes "\n\n[default: ...]" after a
                // long help when the option has one, and the corpus file
                // ends in a newline of its own, so the reader saw two blank
                // lines where every other option shows none. Only --trim and
                // --quote carry a clap default; the three options whose
                // default is a behaviour rather than a value say so in their
                // own text. An option that gains a default wants this too.
                .long_help(TRIM.trim_end())
                .long("trim")
                .num_args(1)
                .required(false)
                .ignore_case(true)
                .default_value("none")
                .value_parser(abbreviated_values(&TRIMS)),
        )

        .arg(
            clap::Arg::new("read_delimiter")
                .action(clap::ArgAction::Set)
                .value_name("char")
                .help(help_summary(READ_DELIMITER))
                .long_help(READ_DELIMITER)
                .long("read-delimiter")
                .num_args(1)
                .required(false)
        )

        .arg(
            clap::Arg::new("single_quotes")
                .action(clap::ArgAction::SetTrue)
                .value_name("single-quote")
                .help(help_summary(SINGLE_QUOTES))
                .long_help(SINGLE_QUOTES)
                .long("single-quote")
                .required(false)
        )

        .arg(
            clap::Arg::new("comment")
                .action(clap::ArgAction::Set)
                .value_name("char")
                .help(help_summary(COMMENT))
                .long_help(COMMENT)
                .long("comment")
                .num_args(1)
                .required(false)
        )

        .arg(
            clap::Arg::new("fill_log")
                .action(clap::ArgAction::Set)
                .value_parser(path_value())
                .value_name("file")
                .help(help_summary(FILL_LOG))
                .long_help(FILL_LOG)
                .long("fill-log")
                .num_args(1)
                // Refused without a filling mode rather than writing a log of
                // nothing: an empty log reads as a clean file, which is the
                // one conclusion it must never invite.
                .requires("flexible")
                .required(false)
        )

        .arg(
            clap::Arg::new("print_delimiter")
                .action(clap::ArgAction::Set)
                .value_name("char")
                .help(help_summary(PRINT_DELIMITER))
                .long_help(PRINT_DELIMITER)
                .long("print-delimiter")
                .num_args(1)
                .required(false)
        )

        .arg(
            clap::Arg::new("quotes")
                .action(clap::ArgAction::Set)
                .value_name("quoting")
                .help(help_summary(QUOTES))
                // Trimmed for the reason given at --trim above.
                .long_help(QUOTES.trim_end())
                .long("quote")
                .num_args(1)
                .required(false)
                .ignore_case(true)
                .default_value("necessary")
                .value_parser(abbreviated_values(&QUOTINGS))
        )

        .arg(
            clap::Arg::new("peek")
                .action(clap::ArgAction::SetTrue)
                .help(help_summary(PEEK))
                .long_help(PEEK)
                .long("peek")
                .required(false)
                // Not exclusive, unlike --list-options and --generate-man:
                // this one reads the input, and has to read it the way a run
                // would or the columns it numbers are not the ones a run
                // sees. So the options that decide what a record *is* are
                // welcome, and the ones that write are refused, there is
                // no CSV here for them to act on.
                // The actions are named one by one rather than as their
                // group, so clap names the one given in its message rather
                // than listing all eight.
                .conflicts_with_all([
                    "action_to_rfc3339",
                    "action_to_utc",
                    "action_to_local",
                    "action_offset",
                    "action_split",
                    "action_duration",
                    "action_remove",
                    "action_arrange",
                    "position",
                    "round_seconds",
                    "flexible",
                    "fill_log",
                    "quotes",
                    "print_delimiter",
                ])
        )
        .arg(
            clap::Arg::new("list_options")
                .action(clap::ArgAction::SetTrue)
                .help(help_summary(LIST_OPTIONS))
                .long_help(LIST_OPTIONS)
                .long("list-options")
                .required(false)
                // A question about the program rather than part of a run, so
                // like --help and --version it stands alone.
                .exclusive(true)
        )
        .arg(
            clap::Arg::new("generate_man")
                .action(clap::ArgAction::SetTrue)
                .help(help_summary(GENERATE_MAN))
                .long_help(GENERATE_MAN)
                .long("generate-man")
                .required(false)
                // Another question about the program rather than a run.
                .exclusive(true)
        )
        // clap writes '[default: none] [possible values: ...]' onto the end
        // of the line the help text ends on, which puts three summaries back
        // over the column everything else was folded to, one of them at
        // 129. Ending a summary on a newline moves the note to a line of its
        // own, and clap indents it with the rest.
        //
        // Only where there is a note to move: the same newline with nothing
        // to follow it leaves a line of trailing spaces. Asked of each
        // argument rather than kept as a list of the three that have one
        // today, which would be a list to remember when a fourth arrives.
        .mut_args(|arg| {
            let noted = !arg.get_possible_values().is_empty()
                || !arg.get_default_values().is_empty();
            match arg.get_help().map(ToString::to_string) {
                Some(help) if noted => arg.help(format!("{help}\n")),
                _ => arg,
            }
        })
}

/// Reads the command line, and answers --help, --version, --list-options and
/// --generate-man there and then: each ends the run once it is written.
pub(crate) fn arguments() -> Result<ArgumentFlags, Box<dyn Error>> {
    let command = build_command();
    // Parsed from a clone so the Command itself survives to be introspected
    // below; get_matches consumes what it is called on.
    //
    // try_ rather than get_matches so --help and --version come back here
    // instead of being printed on clap's way out. clap prints them and
    // ignores a failed write, and there is nothing of csvdt's left to ask by
    // then: 'csvdt --help > page.txt' on a full disk would write nothing and
    // report 0, the one answer that must never be wrong. Handed to answer()
    // they are written like every other thing csvdt says about itself: a
    // closed pipe passes, anything else ends the run at 1.
    //
    // Every other kind is clap's own to report, and exit() is what
    // get_matches called for them, so those are unchanged down to the byte.
    // Parsed and printed from a copy with the verbatim gutters taken out:
    // they mark blocks for the page, and a reader of --help must never see
    // one. The page below is rendered from `command` itself, marks and all.
    let matches = match without_marks(command.clone()).try_get_matches() {
        Ok(matches) => matches,
        Err(error) => {
            use clap::error::ErrorKind;
            match error.kind() {
                ErrorKind::DisplayHelp
                | ErrorKind::DisplayVersion
                | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    answered(say_as_clap_would(&error));
                    std::process::exit(0);
                },
                _ => error.exit(),
            }
        },
    };
    answer_about_program(&command, &matches);
    flags_from(&matches)
}

/// Writes what --list-options or --generate-man asks for and ends the run;
/// returns without a word when neither was given.
fn answer_about_program(command: &clap::Command, matches: &clap::ArgMatches) {
    // Answered before anything else, and before any input is opened: this is
    // how a wrapper discovers the options, so it must work on a bare invocation.
    if matches.get_flag("list_options") {
        answer(option_list(command).as_bytes());
        std::process::exit(0);
    }

    // Answered here too, and for the same reason: the page is rendered from
    // the Command above rather than kept beside it, so it describes this
    // build and cannot fall behind it.
    if matches.get_flag("generate_man") {
        let mut rendered = Vec::new();
        // NAME wants one line and DESCRIPTION wants the rest, and the
        // Command's own 'about' is a paragraph, right where -h prints it,
        // too long where a page prints a name. So NAME is given its line
        // here, on the copy being rendered; long_about is already the whole
        // of it, and DESCRIPTION takes that.
        //
        // The page also drops the closing DOCUMENTATION section, which tells
        // a reader of --help that a page exists. This is the page.
        let page = command
            .clone()
            .about("parse CSV files and convert the timestamps in them")
            .after_long_help(TRAILING);
        let rendering = clap_mangen::Man::new(page)
            .title("CSVDT")
            .section("1")
            .source(format!("csvdt {}", crate_version!()))
            .manual("User Commands");
        if let Err(error) = rendering.render(&mut rendered) {
            // Worth a word rather than a bare status: a packager sees this
            // during a build, where silence is the hardest failure to place.
            fatal(&format!("could not render the manual page: {error}"));
            std::process::exit(1);
        }
        let rendered = man::arrange_page(&rendered, &TRAILING_HEADINGS, MAN_SUPPLEMENT);
        // Last, so nothing above can leave a space at the end of a line.
        // clap_mangen writes the SYNOPSIS by appending each argument and a
        // space after it, and roff does not notice one at the end of a line.
        // The page is read and shipped by people, though: a packager diffs
        // it, a repository shows the space as a red block, and an editor that
        // strips trailing whitespace on save would make the page differ from
        // the one the binary generates.
        let rendered = without_trailing_padding(&String::from_utf8_lossy(&rendered)).into_bytes();
        answer(&rendered);
        std::process::exit(0);
    }
}

/// The flags a run is given, from a command line clap has accepted, with each
/// value checked and resolved here, before any input is opened.
fn flags_from(matches: &clap::ArgMatches) -> Result<ArgumentFlags, Box<dyn Error>> {
    let mut args = ArgumentFlags::default();
    if let Some(value) = matches.get_one::<std::path::PathBuf>("input_file") {
        // An empty file argument is an unset shell variable, "$file" that
        // expanded to nothing, far more often than a request for standard
        // input, which is what leaving it out already means. Read as "no
        // file given" it would silently consume the pipe, or wait on a
        // terminal, where every other unreadable path is named.
        if value.as_os_str().is_empty() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "the file argument is empty, which is usually an unset shell \
                 variable. Give the file you meant, or leave the argument out \
                 to read standard input".to_string())));
        }
        // '-' is standard input, as it is to other Unix tools, and is made
        // the same as leaving FILE out here, so everything after this point
        // treats the two alike: the spool, the same-file guards, the
        // probe for a closed descriptor. A file named '-' is reached as ./-.
        if value.as_os_str() != "-" {
            args.input_file = Some(value.clone());
        }
    }

    // Unrecognised is not reachable through clap, which accepts only the
    // values in the tables; the default stands if it ever is.
    if let Some(value) = matches.get_one::<String>("trim") {
        args.whitespace_trim = resolve_value(&TRIMS, value).unwrap_or_default();
    }

    if let Some(value) = matches.get_one::<String>("read_delimiter") {
        args.input_delimiter = Some(parse_single_ascii_char("--read-delimiter", value)?);
    }

    if let Some(value) = matches.get_one::<String>("comment") {
        args.read_as_comment = Some(parse_single_ascii_char("--comment", value)?);
    }

    if let Some(value) = matches.get_one::<std::path::PathBuf>("fill_log") {
        args.fill_log = Some(value.clone());
    }

    if let Some(value) = matches.get_one::<String>("action_split") {
        args.action = Action::Split;
        args.selected_column = parse_column("--split", value)?;
    }

    if let Some(value) = matches.get_one::<String>("action_to_rfc3339") {
        args.action = Action::ToRfc3339;
        args.selected_column = parse_column("--rfc3339", value)?;
    }

    if let Some(value) = matches.get_one::<String>("action_to_utc") {
        args.action = Action::ToUtc;
        args.selected_column = parse_column("--utc", value)?;
    }

    if let Some(value) = matches.get_one::<String>("action_to_local") {
        args.selected_column = parse_column("--local", value)?;
        // Resolved once here rather than per row.
        args.action = Action::ToLocal(local_timezone()?);
    }

    if let Some(value) = matches.get_one::<String>("action_remove") {
        let mut columns = parse_column_list("--remove", value)?;
        // Refused rather than quietly deduplicated. Removing one column twice
        // is not what the list says, and --arrange refuses a repeat, so the
        // two lists should agree about the same mistake.
        if let Some(column) = repeated_column(&columns) {
            return Err(repeated_column_error(
                "--remove", column,
                "since removing one twice is not something a list can ask for"));
        }
        // Sorted highest first so removing one never shifts the position of
        // another still to come.
        columns.sort_unstable_by(|a, b| b.cmp(a));
        args.action = Action::Remove(columns);
    }

    if let Some(value) = matches.get_one::<String>("action_arrange") {
        let columns = parse_column_list("--arrange", value)?;
        if let Some(column) = repeated_column(&columns) {
            return Err(repeated_column_error(
                "--arrange", column, "since the list is the new column order"));
        }
        // Sorted copy so deciding "was this column listed?" while walking a row
        // is a binary search rather than a scan of the whole list. The list
        // itself keeps the order given, that order *is* the output order.
        let mut listed = columns.clone();
        listed.sort_unstable();
        args.action = Action::Arrange { order: columns, listed };
    }

    if let Some(value) = matches.get_one::<String>("action_duration") {
        args.action = Action::Duration { second: None };
        match value.split_once(',') {
            Some((first, second)) => {
                let column = |part: &str| {
                    part.parse::<usize>()
                        .map_err(|_| invalid_column_of("--duration", part, value, false))
                };
                args.selected_column = column(first)?;
                let second = column(second)?;
                // Refused rather than answered. The span between a column and
                // itself is PT0S on every row, which looks like a measurement
                // and is not one, and unlike parse_err, nothing in the output
                // says so. --remove and --arrange already refuse a column named
                // twice; the same mistake here was the only one that produced a
                // full column of plausible results instead.
                if second == args.selected_column {
                    return Err(repeated_column_error(
                        "--duration", second,
                        "since the span between a column and itself is PT0S on \
                         every row rather than a duration"));
                }
                args.action = Action::Duration { second: Some(second) };
            },
            None => {
                args.selected_column = parse_column("--duration", value)?;
            },
        }
    }

    args.round_seconds = matches.get_flag("round_seconds");
    // clap is told --round-seconds requires --duration, and holds it to that
    // when --round-seconds is given on its own. It does not when another action
    // is present: that action conflicts with --duration, and the requirement
    // goes with it. So `-u0 --round-seconds` would be accepted and the flag
    // would do nothing, since -u keeps whatever precision the input carried,
    // and the run would look as if it had honoured the request.
    if args.round_seconds && !matches!(args.action, Action::Duration { .. }) {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "'--round-seconds' rounds the result of '-d/--duration', and there \
             is no '-d' here. It is not a general precision setting: every other \
             conversion writes back whatever precision the timestamp carried, \
             rather than rounding it.")));
    }

    if let Some(value) = matches.get_one::<String>("action_offset") {
        let (column_word, offset_word) = value.split_once(',').ok_or_else(|| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "'--offset' expects <column>,<offset>, e.g. -o1,-06:00"))
        })?;
        args.selected_column = parse_column("--offset", column_word)?;
        args.action = Action::ToOffset(parse_offset(offset_word)?);
    }

    if let Some(value) = matches.get_one::<String>("position") {
        let (column_word, position_word) = value.split_once(',').ok_or_else(|| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "'--insert' expects <column>,<position>, e.g. -i1,replace"))
        })?;
        args.insert_column = Some(parse_signed_column("--insert", column_word)?);
        args.insert_position = parse_position_word(position_word)?;
    }

    if let Some(value) = matches.get_one::<String>("print_delimiter") {
        args.output_delimiter = Some(parse_single_ascii_char("--print-delimiter", value)?);
    }

    if let Some(value) = matches.get_one::<String>("quotes") {
        args.output_quotes = resolve_value(&QUOTINGS, value).unwrap_or_default();
    }

    args.has_headers = matches.get_flag("has_header");
    args.peek = matches.get_flag("peek");
    args.output_header = matches.get_flag("print_header");
    args.input_quotes = matches.get_flag("single_quotes");
    if let Some(mode) = matches.get_one::<String>("flexible") {
        args.flexible_record = true;
        // Resolved rather than matched literally, so -f=FIX cannot pass clap
        // and then land on the fallback.
        args.fill_mode = resolve_value(&FLEXIBLE_MODES, mode).unwrap_or_default();
    }

    check_special_characters(&args)?;
    Ok(args)
}

/// The name a message should use for a character with no useful printable form.
pub(crate) fn describe_character(character: char) -> String {
    match character {
        '\n' => "a newline".to_string(),
        '\r' => "a carriage return".to_string(),
        '\t' => "a tab".to_string(),
        // Naming these, rather than quoting a quote and printing ''' .
        '\'' => "a single quote".to_string(),
        '"' => "a double quote".to_string(),
        ' ' => "a space".to_string(),
        _ => format!("'{character}'"),
    }
}

/// Refuses delimiter and comment characters that cannot do the job asked of
/// them, as opposed to ones that merely look unusual. Each of these would be
/// accepted and then produce output that cannot be read back, or drop rows,
/// the same quiet wrongness -o refuses an impossible offset for.
///
/// A tab is not among them: that is how TSV is read and written.
pub(crate) fn check_special_characters(args: &ArgumentFlags) -> Result<(), Box<dyn Error>> {
    let invalid = |message: String| -> Box<dyn Error> {
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, message))
    };

    // The reader's quote character follows --single-quote; the writer always
    // quotes with '"'.
    let read_quote = if args.input_quotes { '\'' } else { '"' };
    let read_delimiter = args.input_delimiter.unwrap_or(',');

    // A record ends at one of these, so nothing within one can be marked by
    // one: what follows it is the next row. Given as a delimiter it would
    // silently make the whole line, or the whole file, a single field.
    for (option, character) in [
        ("--read-delimiter", args.input_delimiter),
        ("--print-delimiter", args.output_delimiter),
        ("--comment", args.read_as_comment),
    ] {
        let Some(character) = character else { continue };
        if character == '\n' || character == '\r' {
            return Err(invalid(format!(
                "'{}' was given {}, which ends a record rather than being part \
                 of one. There is no character a row can be divided on that \
                 also ends it.",
                option, describe_character(character))));
        }
    }

    // Quoting is the only way to write a field that holds the delimiter, so a
    // delimiter that is also the quote character leaves no way to write one.
    // Reading would be silently misparsed and writing unreadable, so both
    // are refused.
    for (option, character, quote, because) in [
        ("--read-delimiter", args.input_delimiter, read_quote,
         if args.input_quotes { " for reading, because of '--single-quote'" } else { "" }),
        ("--print-delimiter", args.output_delimiter, '"', " for writing"),
    ] {
        if character == Some(quote) {
            return Err(invalid(format!(
                "'{}' was given {}, which is the quote character in use{}. A \
                 field holding the delimiter is written by quoting it, so the \
                 two cannot be the same character.",
                option, describe_character(quote), because)));
        }
    }

    // A comment character shares the start of a line with the data, so one that
    // collides with the delimiter makes a comment of every row whose first
    // field is empty, and one that collides with the quote character does the
    // same to every row starting with a quoted field. That holds on both
    // sides: reading drops the rows outright, and writing produces lines no
    // reader given the same '--comment' will keep. The written rows escape
    // the tally too: it inspects the first field, and these lines gain the
    // comment character from the delimiter or quote in front of the field.
    // Either way rows leave the file and nothing is said about it.
    //
    // The write side turns on the quoting style, though. 'always' and
    // 'nonnumeric' both quote an empty first field, writing it as "", so no
    // line under them can begin with the bare delimiter; and 'never' quotes
    // nothing, so no line under it gains a quote character the field did not
    // hold, a field that starts with one itself is exactly what the tally
    // counts. Under those styles the collision cannot happen and the pairing
    // stays accepted.
    if let Some(comment) = args.read_as_comment {
        let write_delimiter = args.output_delimiter.unwrap_or(',');
        let style = args.output_quotes;
        let empty_field_written_bare = !matches!(style, Quoting::Always | Quoting::NonNumeric);
        let collision = if comment == read_delimiter {
            Some("also the delimiter being read, so every row whose first field \
                  is empty would be taken for a comment and dropped")
        } else if comment == read_quote {
            Some("also the quote character being read, so every row starting \
                  with a quoted field would be taken for a comment and dropped")
        } else if comment == write_delimiter && empty_field_written_bare {
            Some("also the delimiter being written, so every printed row whose \
                  first field is empty would start with it and be lost to any \
                  reader using the same '--comment'. '--quote always' writes \
                  such a field as \"\" and can print this delimiter")
        } else if comment == '"' && style != Quoting::Never {
            Some("also the quote character used for writing, so every printed \
                  row starting with a quoted field would be lost to any reader \
                  using the same '--comment'")
        } else {
            None
        };
        if let Some(collision) = collision {
            return Err(invalid(format!(
                "'--comment' was given {}, which is {}.",
                describe_character(comment), collision)));
        }
    }

    Ok(())
}
