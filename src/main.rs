use std::error::Error;
use chrono::{DateTime, Datelike, FixedOffset, Timelike};
use clap::builder::PossibleValue;
use clap::crate_version;

// What --flexible does about records narrower than the rest.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
enum FillMode {
    // Bare -f: let uneven records through untouched.
    #[default]
    Off,
    // -f=fix: pad out to the first record's width in a single pass, which is
    // all a stream allows.
    FirstRecord,
    // -f=widest: read the file once to find the widest record, then pad
    // everything to that. Needs a file it can read twice.
    Widest,
}

#[derive(Default, Debug, Clone)]
struct ArgumentFlags {
    input_file: String,
    has_headers: bool,                    // -H
    peek: bool,                           // --peek
    output_header: bool,                  // -p --print-header
    flexible_record: bool,                // -f --flexible
    fill_mode: FillMode,                  // --flexible's mode
    selected_column: usize,               // the action's own (first) column
    second_column: Option<usize>,         // --duration's second column, when given
    insert_column: Option<i64>,           // -i's column; None means "use the action's own"
    insert_position: String,              // -i's before/after/replace word; default "after"
    action_to_rfc3339: bool,              // -r --rfc3339 <col>
    action_to_utc: bool,                  // -u --utc <col>
    action_to_local: bool,                // -l --local <col>
    local_timezone: Option<chrono_tz::Tz>,// -l's resolved zone; None = use chrono's own
    action_split: bool,                   // -s --split <col>
    action_remove: bool,                  // --remove <col>[,<col>...]
    remove_columns: Vec<usize>,           // --remove's columns, high index first
    action_arrange: bool,               // -R --arrange <col>[,<col>...]
    arrange_columns: Vec<usize>,        // -R's columns, in the order given
    arrange_listed: Vec<usize>,         // the same set, sorted, for membership tests
    action_duration: bool,                // -d --duration <col>[,<col2>]
    round_seconds: bool,                  // --round-seconds
    action_offset: bool,                  // -o --offset <col> <offset>
    target_offset: Option<FixedOffset>,   // the offset parsed from -o's second value
    whitespace_trim: String,              // --trim
    input_delimiter: Option<char>,        // --read-delimiter
    input_quotes: bool,                   // --single-quotes
    read_as_comment: Option<char>,        // --comment
    fill_log: Option<String>,             // --fill-log's destination; "-" is stderr
    output_delimiter: Option<char>,       // --print-delimiter
    output_quotes: String,                // --quote
}

// Writes one line to stderr, accepting that it may not arrive. Everything
// said on stderr is commentary on a result that already stands -- a warning
// beside a successful run, or the reason for an exit status the caller reads
// either way. eprintln! panics when stderr is closed or full, which turned
// exactly those runs into exit 101 with the message lost too: reporting
// became the failure it was reporting.
// Starts every option's description on its own line.
//
// .TP puts the tag in the margin and the description beside it when the tag
// is narrower than the indent, and below it when it is not. Every option
// here is named widely enough to go below except one, so --peek alone read
// differently from the rest of OPTIONS:
//
//     --peek Numbers  the  columns  and  shows  the top of the file, then
//            exits without converting anything.
//     --comment <char>
//            The  comment  character  to use when parsing CSV.
//
// A break after the tag settles it, and settles it for whatever is named
// next: no width is measured, so an option added later reads like its
// neighbours whether its name is short or long. On a tag already too wide to
// share its line the break falls where roff was going to break anyway, and
// the page is unchanged -- which is why this needs no test of length, and
// has none to get wrong.
fn break_after_option_tags(rendered: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(rendered);
    let mut out = String::with_capacity(text.len());
    let mut after_tp = false;
    for line in text.split_inclusive('\n') {
        out.push_str(line);
        let bare = line.trim_end_matches(['\n', '\r']);
        if after_tp {
            // The tag is the one line after the request. Anything else there
            // is not a tag to break after -- a .TP followed straight by
            // another request is malformed, and left as it was found.
            if !bare.starts_with('.') {
                out.push_str(".br\n");
            }
            after_tp = false;
        } else if bare == ".TP" {
            after_tp = true;
        }
    }
    out.into_bytes()
}

// Turns the headings inside a section into real man page subsections.
//
// The help gives every heading a blank line above and none below, so that it
// sits against what it introduces. Printed as it is written that reads as
// intended, since --help writes the text out as it stands. roff fills, and a
// heading with nothing after it to end the line is simply the first words of
// the paragraph below:
//
//     COLUMNS  ARE  ADDRESSED  BY  POSITION  Always  by number from 0, never by
//     header name, so a repeated header name is not ambiguous.
//
// Twelve of the fifteen headings under KNOWN LIMITS came out that way, which
// leaves that section one long paragraph with capitals scattered through it.
//
// .SS is what the format has for this, and it settles the spacing as well as
// the break: the blank line above is dropped when there is one, since .SS
// brings its own and two together read as a gap rather than a heading.
//
// A heading is a line at the margin whose words are all capitals once the
// option names among them are set aside -- 'parse_err' too, spelled as the
// output spells it. That is what separates 'A LEAP SECOND ADDS NOTHING TO A
// SPAN' from the paragraph beginning 'Unix epoch (read by -r/--rfc3339,'.
// The names arrive escaped, so a word opening '\-' is one; this runs before
// the pass that adds '\%', and allows for it anyway so the order of the two
// is not a thing to remember.
fn promote_man_subheadings(rendered: &[u8]) -> Vec<u8> {
    fn is_heading(line: &str) -> bool {
        if line.is_empty()
            || line.starts_with('.')
            || line.starts_with('\'')
            || line.starts_with(char::is_whitespace)
        {
            return false;
        }
        if line.ends_with('.') || line.ends_with(':') || line.ends_with(',') {
            return false;
        }
        // An apostrophe reaches the roff as '\*(Aq', whose 'q' would answer
        // for a lowercase letter and rule out every heading holding one.
        let text = line.replace("\\*(Aq", "'");
        let mut words = text.split_whitespace().filter(|word| {
            let bare = word.strip_prefix("\\%").unwrap_or(word);
            !bare.starts_with("\\-") && !bare.starts_with('-')
                && *word != "parse_err"
        });
        let mut any = false;
        let all_capitals = words.all(|word| {
            any = true;
            word.chars().any(|c| c.is_ascii_uppercase())
                && !word.chars().any(|c| c.is_ascii_lowercase())
        });
        any && all_capitals
    }

    let text = String::from_utf8_lossy(rendered);
    let mut out: Vec<String> = Vec::new();
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\n', '\r']);
        if is_heading(bare) {
            // .SS supplies the space above, so the blank line the corpus put
            // there is given up rather than added to.
            if out.last().is_some_and(|last| last.trim().is_empty()) {
                out.pop();
            }
            out.push(format!(".SS {bare}\n"));
        } else {
            out.push(line.to_string());
        }
    }
    out.concat().into_bytes()
}

// Turns the capitalised lines that open csvdt's trailing help into real man
// page sections, and drops the heading clap_mangen files them all under.
//
// clap_mangen puts everything after the options into one section it calls
// EXTRA, which is not a heading any man page uses and buries three topics
// under one name. The text itself already carries their headings, so this
// only promotes them: a line that is exactly one of HEADINGS becomes .SH,
// and .SH EXTRA goes, since what it introduced now introduces itself.
//
// Works on the rendered roff rather than the source, so every escape
// clap_mangen applied is left exactly as it made it. Only whole lines match,
// which is what keeps 'KNOWN LIMITS, at the end of this help' -- prose, mid
// sentence, three times over -- from becoming a section heading.
fn promote_man_headings(rendered: &[u8], headings: &[&str]) -> Vec<u8> {
    let text = String::from_utf8_lossy(rendered);
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\n', '\r']);
        if bare == ".SH EXTRA" {
            continue;
        }
        if headings.contains(&bare) {
            out.push_str(".SH ");
            out.push_str(bare);
            out.push('\n');
        } else {
            out.push_str(line);
        }
    }
    out.into_bytes()
}

// Stops roff breaking an option name across two lines.
//
// roff hyphenates to fill a line evenly, and it does not know that
// '--duration' is one word a reader may want to copy rather than English
// prose. At 80 columns the page came out with
//
//     action's  own column (or its second column, for a two-column --du-
//     ration). This column is never out of bound: too
//
// which reads as an option that does not exist and cannot be pasted into a
// shell. Two names broke that way at 80 columns and five at 60, since the
// narrower the measure the likelier any given word lands on the boundary.
//
// '\%' at the start of a word tells roff not to hyphenate that word. It is
// only ever a hint about breaking, so it costs nothing anywhere else: the
// line count, the measure and every other break are exactly as they were,
// and groff stays silent. The alternative -- '.nh' over the whole page --
// would also stop hyphenating the prose, which has no such problem and
// justifies better for being allowed to break.
//
// Option names reach the rendered roff with each hyphen escaped, as
// '\-\-duration', so a word beginning '\-' is the thing to protect. Two
// places hold them. Most are in running text, where the name begins a word.
// The rest are the whole argument of a one-line font macro, '.B
// \-\-flexible=widest' in FILES being the one the corpus has -- and a macro
// line is skipped by the text rule, since a request must keep its own first
// word. That one is handled by name, for the font macros man pages use.
fn protect_option_names(rendered: &[u8]) -> Vec<u8> {
    // The font macros that take text as their argument. A name given to one
    // of these is set in bold or italic and then flows into the surrounding
    // paragraph, where it can be broken like any other word.
    const FONT_MACROS: [&str; 8] =
        [".B", ".I", ".BR", ".IR", ".RB", ".RI", ".BI", ".IB"];

    // Marks every option name in one span of text. A font macro's argument
    // is set in the surrounding paragraph and breaks exactly as running text
    // does, so the two are the same problem and get the same answer -- which
    // they did not before: the macro branch marked an argument only when the
    // whole of it was a name, so '.I the destination given to \-\-fill\-log'
    // reached the page unprotected and could be broken at the hyphen.
    fn mark(span: &str, out: &mut String) {
        // Word-opening is the whole test: a hyphen inside a word is a real
        // hyphen -- 'two-column', 'csvdt-widest-' -- and breaking there is
        // what a reader expects, so it is left alone.
        let mut rest = span;
        let mut at_start = true;
        while let Some(index) = rest.find("\\-") {
            let (before, from) = rest.split_at(index);
            out.push_str(before);
            // Only a hyphen that opens a word. '\%' in the middle of one is
            // not inert -- it is an extra place roff may break -- so getting
            // this wrong does the opposite of what the pass is for: it broke
            // the example rows in EXAMPLES at their dates, '2023\-11\-14'
            // being a word whose second and third hyphens are internal.
            let opens_word = before.ends_with(char::is_whitespace)
                || (at_start && before.is_empty());
            // And a name, not a dash. NAME is spelled 'csvdt \- parse CSV
            // files', where the '\-' is the separator man pages put there
            // and the word ends at it. What follows an option's hyphen is
            // either its second hyphen or the first letter of its name.
            let names_option = {
                let after = &from["\\-".len()..];
                after.starts_with("\\-")
                    || after.starts_with(|c: char| c.is_ascii_alphanumeric())
            };
            if opens_word && names_option {
                out.push_str("\\%");
            }
            out.push_str("\\-");
            rest = &from["\\-".len()..];
            at_start = false;
        }
        out.push_str(rest);
    }

    let text = String::from_utf8_lossy(rendered);
    let mut out = String::with_capacity(text.len() + text.len() / 32);
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\n', '\r']);
        let ending = &line[bare.len()..];

        if bare.starts_with('.') || bare.starts_with('\'') {
            // A request keeps its own first word, whatever it holds; the
            // argument of a font macro is text and is treated as text.
            match bare.split_once(' ') {
                Some((macro_name, argument))
                    if FONT_MACROS.contains(&macro_name) =>
                {
                    out.push_str(macro_name);
                    out.push(' ');
                    mark(argument, &mut out);
                }
                _ => out.push_str(bare),
            }
            out.push_str(ending);
            continue;
        }

        mark(bare, &mut out);
        out.push_str(ending);
    }
    out.into_bytes()
}

// The gutter that marks a block in the help files as verbatim.
//
// A line opening with it is text laid out on purpose -- a command to be
// copied, a sample of output, a diagram whose columns line up -- and is
// written to the page exactly as it stands. Everything else is prose, and
// prose is roff's to fill to the width it is read at.
//
// It is a mark in the corpus rather than a guess about the corpus. The pass
// below used to decide by shape, holding any block that carried an indent,
// which froze the nested prose along with the listings. Filling that prose
// instead needs the two told apart, and nothing in the shape of a line does
// it: the two lines of csvdt's own stderr quoted under KNOWN LIMITS are
// eight words each and wrapped like a sentence, and a rule that reads them
// as one joins a message the reader is meant to copy.
//
// The gutter never reaches a reader. It is stripped from every string clap
// is given, so --help and -h are the same bytes they were, and stripped
// again here on the way into the page.
const VERBATIM: char = '|';

// Removes the verbatim gutter, leaving the line exactly as it was written.
fn strip_marks(text: &str) -> String {
    text.split('\n')
        .map(|line| line.strip_prefix(VERBATIM).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

// The same command with every gutter taken out of every string it holds.
//
// Used for the copy that parses and prints --help, so a mark meant for the
// page is never shown to a reader. The page itself is rendered from the
// marked command, which is where the marks are read.
fn without_marks(command: clap::Command) -> clap::Command {
    // Read before the command is moved: each getter borrows it.
    let taken = |text: Option<&clap::builder::StyledStr>| {
        text.map(|text| strip_marks(&text.to_string()))
    };
    let about = taken(command.get_about());
    let long_about = taken(command.get_long_about());
    let after_help = taken(command.get_after_help());
    let after_long_help = taken(command.get_after_long_help());

    let mut command = command;
    if let Some(text) = about {
        command = command.about(text);
    }
    if let Some(text) = long_about {
        command = command.long_about(text);
    }
    if let Some(text) = after_help {
        command = command.after_help(text);
    }
    if let Some(text) = after_long_help {
        command = command.after_long_help(text);
    }
    command.mut_args(|arg| {
        let help = arg.get_help().map(|text| strip_marks(&text.to_string()));
        let long_help =
            arg.get_long_help().map(|text| strip_marks(&text.to_string()));
        let mut arg = arg;
        if let Some(text) = help {
            arg = arg.help(text);
        }
        if let Some(text) = long_help {
            arg = arg.long_help(text);
        }
        arg
    })
}

// Gives each block of the help the treatment its author asked for.
//
// roff fills by default: it joins the input lines of a paragraph and rewraps
// them to the reader's width. That is the right treatment for prose and the
// wrong one for a value table. '--quote' documents its four values in aligned
// columns, and filled it came out as
//
//     The quoting style to use when writing CSV.  always     This puts
//     quotes  around every field. Always.  necessary  This puts quotes
//     around fields only when necessary.
//                They are necessary when fields contain a  quote,  de-
//     limiter or record
//
// which is not a table, a paragraph, or readable. clap_mangen hands the help
// text to roff exactly as written and asks for no particular treatment of it,
// so this supplies the treatment.
//
// Three kinds of block, and each run of lines is cut into segments where the
// kind changes, so a paragraph that introduces a listing is not held by the
// listing under it:
//
//   marked with the VERBATIM gutter -- written as it stands, with a break on
//   every line, and the gutter taken off;
//
//   indented and unmarked -- a nested paragraph. The indent is handed to
//   roff as '.RS' and the words fill inside it, which is how a page nests
//   prose and how these read at any width rather than at the help file's;
//
//   at the margin and unmarked -- ordinary prose, left alone to fill.
//
// The cost of holding a line is that a break is all it gets: filling stays
// on, so a line too long for the terminal is still wrapped by roff -- and
// having been marked to keep its shape, it is wrapped and then justified,
// which is worse than either. A '--peek' example widened to 70 columns came
// out of an 80-column page as
//
//     $       csvdt       -H       -p        --peek        sales.csv
//     xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
//
// a command nobody can copy, from the one treatment meant to protect it.
//
// So a marked line is held to the same 64 columns as the rest of the corpus,
// not to a wider measure of its own. The width that matters is the one man
// leaves an option's help, 14 columns, and not the 7 a section's body gets:
// a marked line lives in either, and 64 plus 14 is 78. The test saying so is
// 'the_help_files_fit_the_column_the_manual_page_leaves_them', which asks it
// of every line of every file and ties the 64 to the 14 it comes from.
fn preserve_aligned_blocks(rendered: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(rendered);
    let mut out = String::with_capacity(text.len());
    let mut run: Vec<&str> = Vec::new();

    // Whether a line carries a column gap: two or more spaces with something
    // either side of them, which is how a second column is held away from a
    // first. A single space is not one, so 'headers Trim whitespace' -- the
    // widest name in --trim's table, one space from its description -- does
    // not answer for the table it sits in. Its neighbours do.
    fn has_column_gap(line: &str) -> bool {
        line.trim_end()
            .split_once(|character: char| !character.is_whitespace())
            .is_some_and(|(_, rest)| rest.contains("  "))
    }

    fn indent_of(line: &str) -> usize {
        line.len() - line.trim_start_matches(' ').len()
    }

    fn flush(run: &mut Vec<&str>, out: &mut String) {
        // A segment is a stretch of lines wanting the same treatment: same
        // gutter, same indent. The indent matters because a value's tag sits
        // at one depth and its description at another, and the description
        // is the part that fills.
        let kind = |line: &str| (line.starts_with(VERBATIM), indent_of(line));
        let mut segments: Vec<Vec<&str>> = Vec::new();
        for line in run.iter() {
            match segments.last_mut() {
                Some(open) if kind(open[0]) == kind(line) => open.push(line),
                _ => segments.push(vec![line]),
            }
        }

        let mut first = true;
        let mut after_held = false;
        for segment in &segments {
            let marked = segment[0].starts_with(VERBATIM);
            // A second signal, kept for a table nobody marked. Two lines that
            // both carry a column gap are a table in every case the corpus
            // has, and it only ever holds a block -- the direction that
            // cannot mangle anything. Two rather than one, so a single gap
            // inside a sentence cannot hold a paragraph.
            // Asked of the line without its gutter: the gutter sits at the
            // margin and the block's own indent follows it, which reads as a
            // column gap on every marked line and would answer for a block
            // the mark has already answered for.
            let gapped = segment
                .iter()
                .filter(|line| {
                    has_column_gap(line.strip_prefix(VERBATIM).unwrap_or(line))
                })
                .count()
                >= 2;
            let indent = indent_of(segment[0]);

            if !marked && !gapped && indent > 0 {
                // .RS takes the indent and .RE gives it back, and both break,
                // so nothing need be said about the line either side.
                out.push_str(&format!(".RS {indent}n\n"));
                for line in segment {
                    out.push_str(line.trim_start());
                    out.push('\n');
                }
                out.push_str(".RE\n");
                first = false;
                after_held = false;
                continue;
            }

            let held = marked || gapped;
            for line in segment {
                // .br rather than .nf: a break, with filling left on. .nf
                // would hold every line exactly as written, which is the same
                // thing until a line is too long for the terminal -- and then
                // roff is no longer allowed to wrap it, so the pager does,
                // badly. Whole blocks overflowed 80 columns that way, because
                // the help is written to sit under --help's indent and man
                // adds 14 more. With .br the author's line breaks are kept
                // and roff may still wrap a line that does not fit.
                //
                // A break is also needed on the first line after a held
                // segment, whichever kind follows it. Filling resumes there,
                // and without the break roff would pull that line up onto the
                // end of the last held one.
                if !first && (held || after_held) {
                    out.push_str(".br\n");
                }
                out.push_str(line.strip_prefix(VERBATIM).unwrap_or(line));
                out.push('\n');
                first = false;
                after_held = held;
            }
        }
        run.clear();
    }

    for line in text.lines() {
        // A blank line or a roff request ends whatever run was open. Both are
        // kept where they were: a request inside a held block still runs, and
        // a blank line still breaks a paragraph.
        if line.is_empty() || line.starts_with('.') || line.starts_with('\'') {
            flush(&mut run, &mut out);
            out.push_str(line);
            out.push('\n');
        } else {
            run.push(line);
        }
    }
    flush(&mut run, &mut out);
    out.into_bytes()
}

// Takes the trailing spaces off the page.
//
// clap_mangen writes the SYNOPSIS by appending each argument and a space
// after it, so the line ends with one. It was the only line in the page that
// did. roff neither needs it nor notices it -- a space at the end of a text
// line is collapsed by filling, and nothing here is in no-fill -- but this
// file is read and shipped by people: a packager diffs it, a repository shows
// it as a red block, and an editor that strips trailing whitespace on save
// makes the page differ from the one the binary generates.
//
// Last in the chain, so no pass after it can put one back.
fn drop_trailing_space(rendered: &[u8]) -> Vec<u8> {
    without_trailing_padding(&String::from_utf8_lossy(rendered)).into_bytes()
}

fn note(message: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "{message}");
}

// The last thing a failing run says, named the way every other diagnostic
// here is. Anonymity costs nothing on a terminal, where the command just
// typed is the line above; everywhere else it costs the only thing that
// makes a line actionable. `csvdt big.csv | sort` with stderr in a log left
// "No space left on device (os error 28)" with nothing saying which half of
// the pipeline ran out of room.
fn fatal(message: &str) {
    note(&format!("csvdt: {message}"));
}

// Writes one of the answers csvdt gives about itself -- the option list, the
// manual page -- and ends the run where it could not be written.
//
// A closed pipe stays the caller's business: `csvdt --list-options | head -1`
// is a reasonable thing to do and nothing has gone wrong. Every other failure
// used to be discarded with it, which is how `csvdt --generate-man > page.1`
// on a full disk reported success for the empty file it left behind -- and a
// package build reads that status and installs what is there.
fn answer(bytes: &[u8]) {
    use std::io::Write;
    let mut stdout = std::io::stdout();
    answered(stdout.write_all(bytes).and_then(|()| stdout.flush()));
}

// Takes the padding off the end of a line and leaves the line ending alone.
//
// A carriage return is part of the ending, not padding. On Windows the help
// corpus is checked out with CRLF unless something says otherwise, so a blank
// line inside an option's long help arrives as ten spaces and a CR -- and a
// trim that knew only about spaces and tabs stopped at the CR and left every
// one of them. 59 lines of --help on windows-latest, where Linux and macOS
// had none, which is the shape of bug that only a second platform finds.
fn without_trailing_padding(text: &str) -> String {
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
// clap prints these itself, and did until this: what its printer will not do
// is leave a line of help without trailing spaces on it. Every line of an
// option's long help is indented ten columns, blank lines included, so a
// blank line inside one arrives as ten spaces and a newline. 83 lines of
// --help were whitespace and nothing else, which is invisible on a terminal
// and litter in a file, a diff or a paste.
//
// The bytes are still clap's. StyledStr::ansi() hands back the same string
// clap's own printer writes, and it goes out through the same
// anstream::AutoStream at the same ColorChoice -- the default, Auto, which
// is what this Command asks for by never asking. So colour is decided by
// clap's rules exactly as before, on a terminal and not through a pipe, and
// the stream is the one clap would have picked: help and version to standard
// output, and anything else that renders like help to standard error.
//
// What changes is the trailing spaces, and only those.
fn say_as_clap_would(error: &clap::Error) -> std::io::Result<()> {
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
fn answered(written: std::io::Result<()>) {
    match written {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => (),
        Err(error) => {
            fatal(&error.to_string());
            std::process::exit(1);
        },
    }
}

// Refuses a standard output that cannot take a single byte, before the run
// that would write to it begins.
//
// std answers Ok to a write on a standard stream that failed with EBADF: a
// process without usable standard output is one whose output goes nowhere,
// deliberately, and not an error std will surface. So `csvdt in.csv 1<out`
// -- '<' where '>' was meant -- converted the whole file, wrote none of it,
// counted what it had written on stderr, and exited 0. Nothing later in the
// run can notice, because the flush that should have failed returns Ok too.
//
// The check is a dup of the descriptor and a write of nothing to it. A
// zero-length write puts no byte anywhere and touches no reader -- Linux
// answers a null write on a pipe before it looks for one -- while still
// going through the kernel's check that the descriptor exists and was opened
// for writing.
//
// Only EBADF is refused here. A probe on a full disk answers ENOSPC, and
// that is the run's own failure to report, with its own message, at the
// point it actually runs out of room.
//
// Unix only: this is a gap std leaves on every platform, but the descriptor
// is the part of it there is a way to ask about.
#[cfg(unix)]
fn require_writable_stdout() -> Result<(), Box<dyn Error>> {
    use std::io::Write;
    use std::os::fd::AsFd;

    let probed = std::io::stdout()
        .as_fd()
        .try_clone_to_owned()
        .map(std::fs::File::from)
        .and_then(|mut duplicate| duplicate.write(&[]));
    match probed {
        Err(error) if error.raw_os_error() == Some(libc::EBADF) => {
            Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "standard output cannot be written to. It is usually '<' \
                 where '>' was meant: the output would have gone nowhere and \
                 the run would have reported success".to_string())))
        },
        _ => Ok(()),
    }
}

#[cfg(not(unix))]
fn require_writable_stdout() -> Result<(), Box<dyn Error>> {
    Ok(())
}

// The same gap on the way in. std hides EBADF on a read as well, and hides it
// as end of input, which is a thing an input is allowed to be: `csvdt -H -r0
// 0>out.csv` -- '>' where '<' was meant, which has the shell empty out.csv on
// the way past -- read nothing, wrote nothing, and exited 0, exactly as a run
// over an empty file does.
//
// A zero-length read consumes no byte and does not wait: a pipe with no
// writer yet answers it at once, as does a terminal.
#[cfg(unix)]
fn require_readable_stdin() -> Result<(), Box<dyn Error>> {
    use std::io::Read;
    use std::os::fd::AsFd;

    let probed = std::io::stdin()
        .as_fd()
        .try_clone_to_owned()
        .map(std::fs::File::from)
        .and_then(|mut duplicate| duplicate.read(&mut []));
    match probed {
        Err(error) if error.raw_os_error() == Some(libc::EBADF) => {
            Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "standard input cannot be read from. It is usually '>' where \
                 '<' was meant, which has the shell empty the file on the way \
                 past: the run would have read nothing and reported success, \
                 which is what an empty input does".to_string())))
        },
        _ => Ok(()),
    }
}

#[cfg(not(unix))]
fn require_readable_stdin() -> Result<(), Box<dyn Error>> {
    Ok(())
}

// The identity of an open file as the platform tells it: device and inode on
// Unix, volume serial number and file index on Windows. Two handles reporting
// one identity are one file however it was spelled -- through a symbolic
// link, a hard link, a second spelling of a directory, or '/dev/stdout'.
//
// None where the question is meaningless rather than answerable: a pipe, a
// console, a device. Sharing one of those is not damaging the way sharing a
// regular file is, and a run cannot be both ends of a pipe.
#[cfg(unix)]
fn file_identity(file: &std::fs::File) -> Option<(u64, u64)> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let about = file.metadata().ok()?;
    // A regular file has data to lose; a fifo has a reader to mislead, which
    // is the same harm by another route -- '--fill-log /dev/stdout' into a
    // pipe put the log's own rows into the CSV somebody was reading, which is
    // the corruption the option exists to keep out of the output. A character
    // device is neither: '/dev/null' twice over is one identity with nothing
    // to lose and nobody to mislead, and refusing it would be refusing a run
    // that is fine.
    if !(about.file_type().is_file() || about.file_type().is_fifo()) {
        return None;
    }
    Some((about.dev(), about.ino()))
}

#[cfg(windows)]
fn file_identity(file: &std::fs::File) -> Option<(u64, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    if !file.metadata().ok()?.file_type().is_file() {
        return None;
    }
    let mut about = unsafe { std::mem::zeroed::<BY_HANDLE_FILE_INFORMATION>() };
    // SAFETY: the handle is borrowed from a live File, and the struct is
    // owned here and correctly sized for the call.
    let answered = unsafe {
        GetFileInformationByHandle(file.as_raw_handle() as _, &mut about)
    };
    if answered == 0 {
        return None;
    }
    Some((
        u64::from(about.dwVolumeSerialNumber),
        (u64::from(about.nFileIndexHigh) << 32) | u64::from(about.nFileIndexLow),
    ))
}

#[cfg(not(any(unix, windows)))]
fn file_identity(_file: &std::fs::File) -> Option<(u64, u64)> {
    None
}

// The identity of one of the run's own streams, standard input or standard
// output, which carry no path to open by.
#[cfg(unix)]
fn stream_identity(stream: std::os::fd::BorrowedFd) -> Option<(u64, u64)> {
    let duplicate = stream.try_clone_to_owned().ok().map(std::fs::File::from)?;
    file_identity(&duplicate)
}

// Refuses a '--fill-log' destination that is the input, or where the output
// is going.
//
// The log is created, and creating truncates, so this is asked before the
// file is opened rather than after. 'csvdt -f=widest --fill-log data.csv
// data.csv' emptied the input, then read the empty file it had just made,
// wrote nothing, and exited 0 -- the same shape of loss
// refuse_input_that_is_also_the_output was written for, arriving through a
// door that guard does not watch. It was 75 bytes of somebody's data and
// then it was a header line.
//
// Asked by identity rather than by path, so a symbolic link to the input is
// caught with it, and so is a redirection: 'csvdt --fill-log data.csv <
// data.csv' names no input path at all, and '--fill-log /dev/stdout' names
// no file but is one.
//
// A destination that does not exist yet cannot be either, and is the ordinary
// case: nothing is opened and nothing is refused.
fn refuse_fill_log_that_is_the_input_or_the_output(
    destination: &str,
    input_file: &str,
    reading_stdin: bool,
) -> Result<(), Box<dyn Error>> {
    if destination == "-" {
        return Ok(());
    }
    let Ok(existing) = std::fs::File::open(destination) else { return Ok(()) };
    let Some(log) = file_identity(&existing) else { return Ok(()) };

    let input = if reading_stdin {
        #[cfg(unix)]
        { use std::os::fd::AsFd; stream_identity(std::io::stdin().as_fd()) }
        #[cfg(not(unix))]
        { None }
    } else {
        std::fs::File::open(input_file).ok().and_then(|file| file_identity(&file))
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

    let output = {
        #[cfg(unix)]
        { use std::os::fd::AsFd; stream_identity(std::io::stdout().as_fd()) }
        #[cfg(not(unix))]
        { None }
    };
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

// Refuses an input file that is also where the output is going.
//
// The shell opens a redirection before csvdt is started, so `csvdt in.csv >
// in.csv` hands over a file already truncated to nothing: csvdt read an empty
// file, wrote nothing and exited 0, and the only sign anything had happened
// was that the data was gone. `csvdt in.csv >> in.csv` is the other half, and
// there the file is still whole when this is asked -- left to run it came out
// as the input followed by a conversion of the input, also at 0.
//
// Compared by device and inode rather than by path, so a symbolic link, a
// hard link, or a second spelling of the same directory is caught with it.
//
// Regular files only. Sharing a character device is meaningless rather than
// damaging -- `csvdt /dev/null > /dev/null` is one inode twice over and has
// nothing to lose -- and a run cannot be both ends of a pipe.
#[cfg(unix)]
fn refuse_input_that_is_also_the_output(
    path: &str,
    input: &std::fs::File,
) -> Result<(), Box<dyn Error>> {
    use std::os::fd::AsFd;
    use std::os::unix::fs::MetadataExt;

    let Ok(read_from) = input.metadata() else { return Ok(()) };
    if !read_from.file_type().is_file() {
        return Ok(());
    }
    let written_to = std::io::stdout()
        .as_fd()
        .try_clone_to_owned()
        .map(std::fs::File::from)
        .and_then(|duplicate| duplicate.metadata());
    let Ok(written_to) = written_to else { return Ok(()) };
    if written_to.dev() != read_from.dev() || written_to.ino() != read_from.ino() {
        return Ok(());
    }
    Err(Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("'{path}' is both the input and where the output is going. \
                 The shell opens a redirection before csvdt runs, so '> \
                 {path}' has already emptied it and '>> {path}' would leave \
                 the file followed by a conversion of itself. Write another \
                 file and move it into place"))))
}

// The same refusal on Windows, where a redirection destroys the file exactly
// as it does on Unix: cmd and PowerShell both open '> in.csv' before csvdt
// starts. This was Ok(()) -- the whole guard was Unix-only, while KNOWN LIMITS
// promised it without saying so, and every test for it was gated to Unix so
// nothing noticed.
//
// A file's identity here is its volume serial number and its 64-bit index,
// which is what dev and ino are on the other side. std has no way to ask:
// MetadataExt::file_index is still unstable, so the call is made directly.
// Being an identity rather than a path, it catches a junction, a hard link
// and a second spelling of the same file, as the Unix side does.
//
// The handles are borrowed rather than duplicated. The Unix side clones its
// descriptor because it needs a File to call metadata() on; nothing is opened
// or closed here, so there is no handle to lose and no risk of closing the
// one the run is about to write to.
#[cfg(windows)]
fn refuse_input_that_is_also_the_output(
    path: &str,
    input: &std::fs::File,
) -> Result<(), Box<dyn Error>> {
    use std::os::windows::io::AsRawHandle;

    // None where the question cannot be answered -- a pipe, a console, a
    // device -- which is the same "then it is not the case" the Unix side
    // takes for a handle it cannot stat.
    fn identity(handle: std::os::windows::io::RawHandle) -> Option<(u32, u64)> {
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let mut about = unsafe {
            std::mem::zeroed::<BY_HANDLE_FILE_INFORMATION>()
        };
        // SAFETY: the handle is borrowed from a live File or from Stdout, and
        // the struct is owned here and correctly sized for the call.
        let answered = unsafe {
            GetFileInformationByHandle(handle as _, &mut about)
        };
        if answered == 0 {
            return None;
        }
        Some((
            about.dwVolumeSerialNumber,
            (u64::from(about.nFileIndexHigh) << 32)
                | u64::from(about.nFileIndexLow),
        ))
    }

    let Ok(read_from) = input.metadata() else { return Ok(()) };
    if !read_from.file_type().is_file() {
        return Ok(());
    }
    let Some(source) = identity(input.as_raw_handle()) else {
        return Ok(());
    };
    let Some(destination) = identity(std::io::stdout().as_raw_handle()) else {
        return Ok(());
    };
    if source != destination {
        return Ok(());
    }
    Err(Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("'{path}' is both the input and where the output is going. \
                 The shell opens a redirection before csvdt runs, so '> \
                 {path}' has already emptied it and '>> {path}' would leave \
                 the file followed by a conversion of itself. Write another \
                 file and move it into place"))))
}

#[cfg(not(any(unix, windows)))]
fn refuse_input_that_is_also_the_output(
    _path: &str,
    _input: &std::fs::File,
) -> Result<(), Box<dyn Error>> {
    Ok(())
}

fn main() {
    // Asked before anything is parsed, so that every route to standard output
    // is covered by one question rather than each having to remember it.
    // --help and --version are printed by clap on its way out of get_matches,
    // where there is nothing of csvdt's left to ask, and they were the two
    // that went on reporting success over output nobody received.
    if let Err(error) = require_writable_stdout() {
        fatal(&error.to_string());
        std::process::exit(1);
    }

    let mut args = ArgumentFlags::default();
    let arguments = arguments(args);
    match arguments {
        Ok(arguments) => args=arguments,
        Err(err) => {
            fatal(&err.to_string());
            std::process::exit(100);
        },
    }

    let execute = reader_writer(args);
    match execute {
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
fn is_broken_pipe(error: &(dyn Error + 'static)) -> bool {
    let mut cause = Some(error);
    while let Some(err) = cause {
        if let Some(io) = err.downcast_ref::<std::io::Error>() {
            if io.kind() == std::io::ErrorKind::BrokenPipe {
                return true;
            }
        }
        if let Some(csv) = err.downcast_ref::<csv::Error>() {
            if let csv::ErrorKind::Io(io) = csv.kind() {
                if io.kind() == std::io::ErrorKind::BrokenPipe {
                    return true;
                }
            }
        }
        cause = err.source();
    }
    false
}

// How much of the input to look at when deciding whether it is UTF-16.
const ENCODING_PROBE_BYTES: usize = 512;

// Refuses UTF-16 input up front, returning a reader over the untouched bytes
// when it looks fine.
//
// A byte-order mark settles it outright. Without one, pure-ASCII UTF-16 is
// still recognisable because every second byte is NUL -- and that case matters
// more than it looks: a NUL is valid UTF-8, so nothing downstream objects to
// it. Such a file read with --flexible produced NUL-interleaved output and
// exited 0, which is the one thing this tool must not do.
//
// The test is the full alternating pattern rather than merely "contains a
// NUL", so a field that genuinely holds NUL bytes still passes through.
fn reject_utf16<R: std::io::Read>(mut input: R) -> Result<impl std::io::Read, Box<dyn Error>> {
    let mut probe = vec![0u8; ENCODING_PROBE_BYTES];
    let mut filled = 0;
    while filled < probe.len() {
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

// Guarantees the input hands the CSV reader a final newline when '--comment'
// is in use.
//
// csv-core (0.1.13) mishandles end-of-input inside a comment: its DFA treats
// "in a comment" like "mid-record" and fabricates a record of one empty
// field. A file whose last line is a comment with no trailing newline was
// refused as ragged, or grew a phantom empty row under '--flexible' -- and a
// comment ending in a lone \r stays in the comment the same way, since only
// \n leaves that state. Feeding the reader one extra \n when the input did
// not end with one closes the comment and costs an empty line, which the
// reader skips. Active only when a comment character is set: no other
// well-formed input misparses for want of a final newline. One malformed
// shape does pay: an input that ends inside an unclosed quote takes the
// fabricated \n into that field as data, so its parse differs from the
// same input without '--comment'. Quote state is not visible from a Read
// wrapper, and the input is already in the unclosed-quote territory KNOWN
// LIMITS describes as mangled, so this is documented there, not defended.
//
// The same only-\n rule bites harder mid-file: in a CR-only file (classic
// Mac line endings), \r ends every data record but never a comment, so the
// first comment line runs on through all the rows that follow. This shim
// cannot help there -- it acts at EOF, and rewriting \r mid-stream needs
// the quote state a Read wrapper cannot see (a \r inside a quoted field is
// data). Documented in KNOWN LIMITS, fixable only in csv-core's DFA.
struct EndInNewline<R> {
    input: R,
    active: bool,
    // Whether any byte has been read at all: empty input must stay empty
    // rather than become one blank line.
    seen: bool,
    last: u8,
    appended: bool,
}

fn end_in_newline<R: std::io::Read>(input: R, args: &ArgumentFlags) -> EndInNewline<R> {
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

// The reader's dialect, in one place so the counting pass and the real pass
// cannot disagree about where the fields are.
// The full forms --trim and --quote accept. Both may be abbreviated, and both
// are matched case-insensitively, from this one list each -- there used to be a
// second copy of each written out by hand at the point of use, and they drifted:
// one applied to_lowercase and the other did not, so '--trim ALL' passed clap
// and then matched nothing, silently meaning '--trim none' instead.
const TRIM_VALUES: [&str; 4] = ["all", "fields", "headers", "none"];
const QUOTE_VALUES: [&str; 4] = ["always", "necessary", "never", "nonnumeric"];
// -i/--insert's position word. Not a clap value_parser: -i takes
// "column,position" as one value, so the word is parsed out of it by hand.
const POSITION_VALUES: [&str; 3] = ["before", "replace", "after"];
const FLEXIBLE_VALUES: [&str; 3] = ["keep", "fix", "widest"];

// Every abbreviation of these values clap should accept: each prefix that only
// one of them has. A prefix two of them share is left out, so 'n' is refused
// rather than resolved to whichever of never/necessary/nonnumeric comes first.
fn abbreviated_values(values: &[&'static str]) -> Vec<PossibleValue> {
    let unambiguous = |prefix: &str| {
        values.iter().filter(|full| full.starts_with(prefix)).count() == 1
    };

    let mut possible = Vec::new();
    for value in values {
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

// Which full form a given value stands for. Only prefixes of exactly one full
// form are accepted above, so matching on the prefix cannot be ambiguous.
fn resolve_value(values: &[&'static str], given: &str) -> Option<&'static str> {
    let given = given.to_lowercase();
    let mut matching = values.iter().filter(|full| full.starts_with(&given));
    match (matching.next(), matching.next()) {
        (Some(full), None) if !given.is_empty() => Some(full),
        _ => None,
    }
}

fn configure_reader(args: &ArgumentFlags) -> csv::ReaderBuilder {
    let mut reader_builder = csv::ReaderBuilder::new();
    // --peek reads flexibly whatever was asked for: it writes no CSV, so a
    // record of a different width is a row of a different width in a table
    // and nothing more. Refusing it would turn away the reader most likely
    // to be looking -- somebody whose file is ragged and who wants to see
    // where. The refusal exists to stop uneven records reaching an output
    // that claims to be CSV, and there is no such output here.
    reader_builder
        .has_headers(args.has_headers)
        .flexible(args.flexible_record || args.peek);

    // Unrecognised is not reachable through clap, which accepts only the values
    // above; the reader's own default stands if it ever is.
    match resolve_value(&TRIM_VALUES, &args.whitespace_trim) {
        Some("fields") => { reader_builder.trim(csv::Trim::Fields); },
        Some("headers") => { reader_builder.trim(csv::Trim::Headers); },
        Some("none") => { reader_builder.trim(csv::Trim::None); },
        Some("all") => { reader_builder.trim(csv::Trim::All); },
        _ => {},
    }

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

// Holds a temporary file for as long as it is needed and removes it again on
// drop, including when an error unwinds past it.
struct TempSpool {
    path: std::path::PathBuf,
}

impl Drop for TempSpool {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        // Only after the file is gone, so a signal landing in between finds
        // the name still armed and removes it itself. The other order leaves
        // a window where neither does. Nothing else can be at the name to
        // remove by mistake: it carries this process's id, and this process
        // is still alive to hold it.
        #[cfg(unix)]
        disarm_spool_signals();
    }
}

// Removing the spool when a signal ends the run.
//
// Drop covers every ending Rust knows about -- a clean finish, an error on its
// way out, a panic unwinding -- but a signal is not one of them: the process
// stops where it stands and no destructor runs. So Ctrl-C during the copy, or
// the SIGTERM a job scheduler sends when a run overruns its slot, left the
// spool in the temporary directory holding the whole of the input, which is
// usually a log, and nothing ever came back for it. The help says the file is
// removed when the run ends, including if it fails; that promise did not
// survive the most ordinary way of ending a long run, and a long run is what
// this mode is for.
//
// The handler may only do async-signal-safe things, which rules out the
// allocator and so the PathBuf itself. The name is therefore laid down as a
// NUL-terminated string before the handler can ever run, and reached through
// an atomic pointer; unlink(2) is on the safe list.
#[cfg(unix)]
static SPOOL_NAME: std::sync::atomic::AtomicPtr<libc::c_char> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

// The signals that end a run in the ordinary course of things: Ctrl-C, a
// scheduler's polite stop, and a lost terminal. SIGQUIT is left alone because
// it is asked for when someone wants the core dump as it fell, and SIGKILL
// cannot be caught by anyone -- see the note in --flexible's help.
#[cfg(unix)]
const SPOOL_SIGNALS: [libc::c_int; 3] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP];

#[cfg(unix)]
extern "C" fn remove_spool_on_signal(signal: libc::c_int) {
    use std::sync::atomic::Ordering;
    let name = SPOOL_NAME.swap(std::ptr::null_mut(), Ordering::SeqCst);
    if !name.is_null() {
        // SAFETY: the pointer is either null or the one leaked below, which
        // stays valid for the rest of the process. The swap means only the
        // first signal to arrive acts on it.
        unsafe { libc::unlink(name) };
    }
    // Then die of the signal that was sent rather than of anything this
    // handler decided, so a caller's shell reports the run the way it always
    // has -- 130 for Ctrl-C, and a wait status that still says "interrupted".
    unsafe {
        libc::signal(signal, libc::SIG_DFL);
        libc::raise(signal);
    }
}

#[cfg(unix)]
fn arm_spool_signals(path: &std::path::Path) {
    use std::os::unix::ffi::OsStrExt;
    use std::sync::atomic::Ordering;

    // Leaked deliberately: the handler may read it at any moment up to the
    // end of the process, so there is no later point at which freeing it
    // would be safe. One allocation, once, for the life of the run.
    let Ok(name) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return; // a NUL in a temporary path is not something to fail a run for
    };
    SPOOL_NAME.store(
        Box::leak(name.into_boxed_c_str()).as_ptr().cast_mut(),
        Ordering::SeqCst,
    );

    // Through a function pointer rather than straight from the function item,
    // which is the cast sighandler_t actually wants.
    let handler = remove_spool_on_signal as extern "C" fn(libc::c_int);
    for signal in SPOOL_SIGNALS {
        // SAFETY: the handler touches only an atomic and unlink(2).
        unsafe { libc::signal(signal, handler as libc::sighandler_t) };
    }
}

#[cfg(unix)]
fn disarm_spool_signals() {
    use std::sync::atomic::Ordering;
    SPOOL_NAME.store(std::ptr::null_mut(), Ordering::SeqCst);
}

// Holds the spool signals off until the file exists and the handler knows its
// name, restoring the mask on the way out.
//
// The two steps cannot be done in one, and either order leaves a hole. Armed
// after the file is created -- as it was -- a signal in between takes the
// default action and leaves the copy behind, which is the one thing
// --flexible says it does not do. Armed before, the handler is pointed at a
// path this run has not created yet, and a signal there unlinks whatever
// stands at that name: in a directory shared with every other account, that
// is somebody else's file.
//
// So neither. The signal is made to wait: blocked, it stays pending rather
// than being lost, and is delivered the moment the mask is restored -- by
// which time the file is made and the handler knows it. Held for the two
// steps only, never across the copy, which is the long part of the run and
// the likeliest moment for a Ctrl-C to arrive.
#[cfg(unix)]
struct SpoolSignalsHeld(libc::sigset_t);

#[cfg(unix)]
impl SpoolSignalsHeld {
    fn new() -> Self {
        // SAFETY: sigemptyset initialises the set it is given, sigaddset adds
        // to an initialised one, and pthread_sigmask writes the old mask into
        // the second. Zeroed first so nothing is read uninitialised even if a
        // call were to fail.
        unsafe {
            let mut wanted: libc::sigset_t = std::mem::zeroed();
            let mut previous: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut wanted);
            for signal in SPOOL_SIGNALS {
                libc::sigaddset(&mut wanted, signal);
            }
            libc::pthread_sigmask(libc::SIG_BLOCK, &wanted, &mut previous);
            SpoolSignalsHeld(previous)
        }
    }
}

#[cfg(unix)]
impl Drop for SpoolSignalsHeld {
    fn drop(&mut self) {
        // SAFETY: puts back the mask new() took a copy of, and nothing else.
        unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, &self.0, std::ptr::null_mut());
        }
    }
}

// How many names to try before giving up. A live process cannot share its id,
// so a name is only ever taken by something left over or something planted.
const SPOOL_ATTEMPTS: u32 = 8;

// Creates the file the spool will be copied into.
//
// The system temporary directory is shared with every other account on the
// machine, so two things matter beyond simply opening a file:
//
//   * create_new, so an existing path is refused rather than written through.
//     Plain create followed a symlink left at the name csvdt was going to use,
//     which had csvdt overwrite whatever the symlink pointed at -- anything the
//     user could write -- and drop then removed the symlink behind it.
//   * mode 0600 on Unix, because the default 0644 published the whole input to
//     every account on the machine, and this input is often a log.
fn create_spool_file(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    options.open(path)
}

// The environment variable that moves the temporary directory here.
//
// Named rather than assumed, because the two families disagree and the advice
// is worthless where it is wrong: std::env::temp_dir reads TMPDIR on Unix and
// TMP then TEMP on Windows, so a Windows user told to "set TMPDIR" sets
// something nothing reads, tries again, and gets the same error. The
// remaining platforms fall back to the Unix spelling, which is what
// temp_dir's own fallback path does.
#[cfg(windows)]
const TEMP_DIRECTORY_VARIABLE: &str = "TMP or TEMP";
#[cfg(not(windows))]
const TEMP_DIRECTORY_VARIABLE: &str = "TMPDIR";

// Copies a reader to a temporary file so it can be read twice.
// --flexible=widest has to count before it pads, and neither standard input
// nor a pipe named as a file can be rewound.
fn spool(mut input: impl std::io::Read) -> Result<TempSpool, Box<dyn Error>> {
    let directory = std::env::temp_dir();
    let identifier = std::process::id();
    let mut refusal = None;

    for attempt in 1..=SPOOL_ATTEMPTS {
        // The plain name first, then numbered ones, so a taken name costs a
        // second try rather than the run. A file killed processes leave behind
        // is claimed by whichever id comes round to it next, and would
        // otherwise lock that run out of the mode entirely.
        let path = directory.join(match attempt {
            1 => format!("csvdt-widest-{identifier}.csv"),
            _ => format!("csvdt-widest-{identifier}-{attempt}.csv"),
        });

        // Held across creating the file and arming the handler, which are one
        // step as far as a signal is concerned -- see SpoolSignalsHeld. A name
        // already taken arms nothing, so this simply lapses and the next name
        // is tried with the mask back as it was.
        #[cfg(unix)]
        let held = SpoolSignalsHeld::new();

        match create_spool_file(&path) {
            Ok(mut file) => {
                // Owned before the copy, so a copy that fails still cleans up.
                let spool = TempSpool { path };
                // Armed before it too, and before the first byte is written:
                // the copy is the long part of the run and so the likeliest
                // moment for a Ctrl-C to arrive.
                #[cfg(unix)]
                arm_spool_signals(&spool.path);
                // Only now may one arrive. Anything sent while the file was
                // being made was held rather than lost, and lands here.
                #[cfg(unix)]
                drop(held);
                // Both failures here surfaced as a bare OS error -- "No such
                // file or directory (os error 2)" with no path, no TMPDIR, no
                // mention of the spool -- while open_input exists precisely to
                // never do that to a path the user can see.
                if let Err(error) = std::io::copy(&mut input, &mut file) {
                    return Err(format!(
                        "could not copy the input to '{}', which \
                         '--flexible=widest' reads twice: {}",
                        spool.path.display(), error).into());
                }
                return Ok(spool);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                refusal = Some(error);
            }
            Err(error) => return Err(format!(
                "could not create a temporary file for '--flexible=widest' \
                 in {}: {}. That mode reads its input twice, and a pipe \
                 cannot be rewound, so it needs one. Set {} to a \
                 directory csvdt can write.",
                directory.display(), error, TEMP_DIRECTORY_VARIABLE).into()),
        }
    }

    Err(format!(
        "could not create a temporary file for '--flexible=widest' in {}: \
         every name csvdt tried was already taken ({}). That mode reads its \
         input twice, and a pipe cannot be rewound, so it needs one. Set {} \
         to a directory csvdt can write, or clear the stale 'csvdt-widest-*' \
         files out of that one.",
        directory.display(),
        refusal
            .map(|error| error.to_string())
            .unwrap_or_else(|| "no reason given".to_string()),
        TEMP_DIRECTORY_VARIABLE,
    )
    .into())
}

// Whether a path can be opened and read a second time and give the same thing
// again. A regular file can; a fifo cannot, and neither can the pipe that
// process substitution puts behind a filename. Reading one of those twice
// drained it on the counting pass and left the real pass with nothing, so a
// file came back empty with a success status -- or, for a fifo with no further
// writer, waited for one that was never coming.
fn is_rereadable(path: &str) -> bool {
    // Follows symlinks deliberately: /dev/stdin naming a redirected regular
    // file is as re-readable as the file itself.
    std::fs::metadata(path)
        .map(|data| data.file_type().is_file())
        .unwrap_or(false)
}

// Opens a named input, saying which path failed and why. The bare io error says
// neither: a script handed the wrong path was told "No such file or directory
// (os error 2)", with nothing in it to say which of its paths that was. A
// directory is refused here too, rather than at the first read, where it
// surfaced as "Is a directory" from somewhere inside the CSV reader -- also
// without naming it.
fn open_input(path: &str) -> Result<std::fs::File, Box<dyn Error>> {
    let named = |error: std::io::Error| -> Box<dyn Error> {
        Box::new(std::io::Error::new(
            error.kind(), format!("cannot read '{path}': {error}")))
    };

    let a_directory = || -> Box<dyn Error> {
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("cannot read '{path}': it is a directory, not a file")))
    };

    match std::fs::File::open(path) {
        // Unix opens a directory quite happily and fails at the first read
        // instead, as "Is a directory" from wherever that read is made.
        Ok(file) => match file.metadata().map_err(named)?.is_dir() {
            true => Err(a_directory()),
            // Asked here rather than at the one call site that reads a file,
            // so the counting pass of '--flexible=widest' and the copy that
            // feeds its spool are covered by the same question.
            false => refuse_input_that_is_also_the_output(path, &file).map(|()| file),
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

// Counts the fields of the widest record in the file, so a following pass can
// pad every row out to it. Reads with has_headers(false) so the header line is
// measured too -- it may itself be the short one.
//
// Uses ByteRecord rather than StringRecord: counting fields needs no text, so
// this pass does no UTF-8 validation and leaves reporting a bad encoding to the
// real pass, which can point at it properly.
fn scan_widest_record(args: &ArgumentFlags, path: &str) -> Result<usize, Box<dyn Error>> {
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

fn reader_writer(args:ArgumentFlags) -> Result<(), Box<dyn Error>> {
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
        // quoted field costs field_size/buffer_size scans of it -- time
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
    // This used to be impossible -- the csv crate's writer knew nothing about
    // comments -- and the rows it would lose were counted and warned about
    // instead. Two gaps remain, and the counting still covers exactly those:
    // 'never' quotes nothing by definition, and 'nonnumeric' decides by
    // whether the field is a number, so --comment '-' over a field like -5 is
    // still written bare.
    writer_builder.comment(args.read_as_comment.map(|c| c as u8));

    if let Some(value) = args.output_delimiter {
        writer_builder.delimiter(value as u8);
    } else {
        writer_builder.delimiter(b',');
    }

    match resolve_value(&QUOTE_VALUES, &args.output_quotes) {
        Some("always") => { writer_builder.quote_style(csv::QuoteStyle::Always); },
        Some("never") => { writer_builder.quote_style(csv::QuoteStyle::Never); },
        Some("nonnumeric") => { writer_builder.quote_style(csv::QuoteStyle::NonNumeric); },
        Some("necessary") => { writer_builder.quote_style(csv::QuoteStyle::Necessary); },
        _ => {},
    }

    // Standard output was asked about in main, before any of this was
    // parsed. Standard input is asked here, where it is known whether the run
    // is going to read from it at all.
    if args.input_file.is_empty() {
        require_readable_stdin()?;
    }
    let mut writer = writer_builder.from_writer(std::io::stdout());

    // --flexible=widest reads the input twice: once to find the widest record,
    // then again to pad every row out to it. A named file can simply be opened
    // again; standard input cannot be rewound, so it is copied to a temporary
    // file first. Either way the counting pass finishes before anything is
    // written, so stdout only ever sees fully padded rows.
    // Held only so its Drop removes the temporary file at the end of the run;
    // nothing reads it, hence the underscore.
    let _spooled: Option<TempSpool>;
    let source: Option<String> = if !args.input_file.is_empty() {
        // A named input that cannot be read twice is spooled just as standard
        // input is. Only Widest reads twice, so nothing else pays for this.
        if args.fill_mode == FillMode::Widest && !is_rereadable(&args.input_file) {
            let spool = spool(open_input(&args.input_file)?)?;
            let path = spool.path.to_string_lossy().into_owned();
            _spooled = Some(spool);
            Some(path)
        } else {
            _spooled = None;
            Some(args.input_file.clone())
        }
    } else if args.fill_mode == FillMode::Widest {
        let spool = spool(std::io::stdin())?;
        let path = spool.path.to_string_lossy().into_owned();
        _spooled = Some(spool);
        Some(path)
    } else {
        _spooled = None;
        None
    };

    let tally;
    // Opened before the counting pass, so a destination that cannot be
    // written is reported before the run does its work rather than after: a
    // widest run over a large file would otherwise read the whole of it
    // before finding out it had nowhere to report.
    let mut fill_log = match &args.fill_log {
        Some(destination) => {
            refuse_fill_log_that_is_the_input_or_the_output(
                destination, &args.input_file, args.input_file.is_empty())?;
            Some(FillLog::open(destination)?)
        },
        None => None,
    };
    let scanned_width = match (&source, args.fill_mode) {
        (Some(path), FillMode::Widest) => Some(scan_widest_record(&args, path)?),
        _ => None,
    };

    match &source {
        None => {
            let input = end_in_newline(reject_utf16(std::io::stdin())?, &args);
            let reader = reader_builder.from_reader(input);
            if args.peek {
                answer(peek_stream(reader, &args)?.as_bytes());
                return Ok(());
            }
            tally = process_stream(
                reader, &mut writer, &args, scanned_width, fill_log.as_mut())?;
        },
        Some(path) => {
            let file = match open_input(path) {
                Ok(file) => file,
                Err(err) => {
                    // --flexible's mode has to be attached with '=', so writing
                    // "-f fix" leaves "fix" sitting where the file goes. Say
                    // that, rather than only reporting a file nobody named.
                    let looks_like_a_mode = matches!(
                        args.input_file.as_str(), "fix" | "keep" | "widest");
                    if args.flexible_record && looks_like_a_mode {
                        return Err(Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            format!("no file named '{}'. If you meant the \
                                     --flexible mode, attach it: -f={}",
                                    args.input_file, args.input_file))));
                    }
                    return Err(err);
                },
            };
            let input = end_in_newline(reject_utf16(file)?, &args);
            let reader = reader_builder.from_reader(input);
            if args.peek {
                answer(peek_stream(reader, &args)?.as_bytes());
                return Ok(());
            }
            tally = process_stream(
                reader, &mut writer, &args, scanned_width, fill_log.as_mut())?;
        },
    }

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

// A field as --peek shows it: one line, however many the file gave it.
//
// A quoted field may hold a newline, a carriage return or a tab, and each
// would put the table's own rows and columns somewhere they do not belong --
// a three-row table arriving as five, with the numbering above nothing. Shown
// as the two characters instead, which is what the field holds as far as a
// column number is concerned.
fn peek_field(field: &str) -> String {
    field
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

// Numbers the columns above the rows, padded so each number sits over its
// own field.
//
// Padded by character count rather than by rendered width: a terminal draws
// an emoji two cells wide and a combining accent none at all, and no count
// available here is right for every terminal. Characters are at least a rule
// a reader can predict, and the help says which rule it is.
fn peek_table(rows: &[Vec<String>]) -> String {
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
fn peek_stream<R: std::io::Read>(
    mut reader: csv::Reader<R>,
    args: &ArgumentFlags,
) -> Result<String, Box<dyn Error>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    // -H and -p mean here what they mean everywhere else: -H says the file
    // has a header, which the reader then takes as one rather than as data,
    // and -p asks for it to be written. So -H alone shows the first *data*
    // row, which is the file's second line -- and claiming -H over a file
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

// Splices the hand-written sections into the rendered page.
//
// Everything above them is generated from the Command that parses a command
// line, which is what keeps the page from describing options this build does
// not have. It is also why there is nowhere in it to say what belongs to the
// program rather than to one of its options -- the environment read, the file
// written, what to read next. GNU coreutils fills the same gap with a
// per-page man/prog.x merged by help2man; this is the same idea with one
// file.
//
// Placed before VERSION, which clap_mangen puts at the end with AUTHORS, so
// the hand-written sections sit where a reader expects them and the two
// generated closers stay last. Appended if that heading ever stops being
// emitted, since losing the sections silently is the one outcome worth
// ruling out.
//
// Runs last of the three passes, so the file is spliced in exactly as
// written: it is roff already, and the pass that keeps the help's tables
// from being filled would otherwise put breaks through the middle of it.
fn insert_man_sections(rendered: &[u8], supplement: &str) -> Vec<u8> {
    let text = String::from_utf8_lossy(rendered);
    match text.find(".SH VERSION") {
        Some(at) => {
            let mut out = String::with_capacity(text.len() + supplement.len());
            out.push_str(&text[..at]);
            out.push_str(supplement);
            out.push_str(&text[at..]);
            out.into_bytes()
        },
        None => {
            let mut out = text.into_owned();
            out.push_str(supplement);
            out.into_bytes()
        },
    }
}

// Shared by both the stdin and file input paths so the header/record loop
// exists once; monomorphized per `R`, so this costs nothing over the
// previous hand-duplicated loops.
// The record-by-record account --fill-log writes.
//
// A filling mode changes the input, and the WARNING under --flexible says so:
// the empty fields it adds cannot be told from empty fields the source really
// held. This says exactly where, one row per record, so the change can be
// reviewed against the file it was made to.
//
// CSV, so it loads in whatever the data loads in and joins to the output on
// the row number. Written as it goes rather than collected and written at the
// end: a file where every row is short would otherwise be held in memory
// entire, against a design whose memory grows with the widest record and not
// with the file.
struct FillLog {
    writer: csv::Writer<Box<dyn std::io::Write>>,
    rows: u64,
}

impl FillLog {
    // '-' means stderr, for looking at a run rather than keeping it. Anything
    // else is a path, created rather than appended to: a log that grew across
    // runs would describe rows from an input that has since changed.
    fn open(destination: &str) -> Result<Self, Box<dyn Error>> {
        let sink: Box<dyn std::io::Write> = if destination == "-" {
            Box::new(std::io::stderr())
        } else {
            Box::new(std::fs::File::create(destination).map_err(|error| {
                std::io::Error::new(
                    error.kind(),
                    format!(
                        "could not open '{destination}' for '--fill-log': \
                         {error}. Give a path csvdt can write, or '-' for \
                         standard error"
                    ),
                )
            })?)
        };
        let mut writer = csv::Writer::from_writer(sink);
        writer.write_record(
            ["line", "record", "fields", "width", "delta", "status"])?;
        Ok(FillLog { writer, rows: 0 })
    }

    // POSITION is the reader's, so 'line' is the line an editor jumps to and
    // 'record' counts records -- which drift apart the moment a field holds a
    // newline, and the pair is what lets a reader find the row either way. A
    // header is read before any record and has no position of its own; it is
    // line 1 and record 0, which is what the reader would have said.
    fn note(
        &mut self,
        position: Option<&csv::Position>,
        fields: usize,
        width: usize,
        action: &str,
    ) -> Result<(), Box<dyn Error>> {
        let (line, record) = match position {
            Some(at) => (at.line(), at.record()),
            None => (1, 0),
        };
        self.writer.write_record([
            &line.to_string(),
            &record.to_string(),
            &fields.to_string(),
            &width.to_string(),
            // Fields minus width: how far the row is from the shape it was
            // measured against, negative where it falls short and positive
            // where it runs over. That way round because the row is what is
            // being described -- the deviation is the row's, so the row's
            // count leads, as an observation does against the value it is
            // measured against. The other way round put a minus sign beside
            // the word 'over', which reads as a contradiction rather than as
            // an amount.
            //
            // A fact about the row rather than about what was done to it,
            // which is why it is not named for either -- 'added' was, and so
            // meant what was added under 'padded' and what would have been
            // under a row nothing was done to, which a reader seeing
            // "added 2" beside an untouched row could only take for the
            // first.
            //
            // Signed rather than saturating, so it says something on every
            // row. At zero the column was dead for the whole of 'over': a row
            // seven fields wide against a width of two read 0, and how far
            // over it was -- the number wanted most on exactly those rows --
            // could only be had by subtracting two other columns.
            //
            // 'delta' rather than a word naming one direction, since it
            // has two, and never zero: a row matching the width is not
            // logged at all, so every row here is off by at least one.
            &(fields as i64 - width as i64).to_string(),
            action,
        ])?;
        self.rows += 1;
        Ok(())
    }

    // Flushed here rather than left to Drop, which swallows the error: a log
    // that could not be written is worth an exit status, since the whole
    // point of it is to be read afterwards.
    fn finish(mut self) -> Result<u64, Box<dyn Error>> {
        self.writer.flush()?;
        Ok(self.rows)
    }
}

fn process_stream<R: std::io::Read, W: std::io::Write>(
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
    // first data row.
    // Widest mode brings its width in from the counting pass; FirstRecord takes
    // it from whichever record comes first.
    let mut fill_width: Option<usize> = scanned_width;

    if args.has_headers {
        // Asked before anything pads or edits the record: empty input, or input
        // that is nothing but blank lines -- which the CSV reader skips --
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
        // it still round-trips -- and one emptied by --remove is not this case
        // either: it was read, and its data rows print `""` as well, so it
        // stays with them.
        if !headers.is_empty() {
            if args.fill_mode == FillMode::FirstRecord {
                fill_width = Some(headers.len());
            }
            // Plain --flexible fills nothing and so has no width of its own.
            // The log still wants one to measure against, and a single pass
            // has only the first record to offer -- the header, when there is
            // one, exactly as fix would take it.
            if args.fill_mode == FillMode::Off && fill_log.is_some() {
                fill_width = Some(headers.len());
            }
            // The header is padded too under Widest, since the widest row may
            // be wider than it. The added names are empty rather than invented.
            if let Some(width) = fill_width {
                // Under Widest no record can be wider than the counted width,
                // the header included -- the counting pass read it too. A
                // header that is means the input changed between the two
                // reads, which the data rows already report in those terms;
                // unchecked here, it fell through to the writer's bare
                // record-length error instead.
                if headers.len() > width {
                    if let Some(log) = fill_log.as_deref_mut() {
                        log.note(None, headers.len(), width, "over")?;
                    }
                    return Err(row_wider_than_reference(
                        headers.len(), width, args.fill_mode));
                }
                if headers.len() < width {
                    if let Some(log) = fill_log.as_deref_mut() {
                        log.note(None, headers.len(), width, "padded")?;
                    }
                }
                // Under FillMode::Off the width above is the header's own, so
                // neither branch can fire and nothing pads: the header is the
                // reference rather than a row measured against it.
                while headers.len() < width {
                    headers.push_field("");
                }
            }
            // Not stored back into the reader: nothing reads its header state
            // after this point -- the loop below only calls read_record, and
            // the writer is handed the record directly.
            let new_headers = process_headers(headers, args)?;
            if args.output_header {
                // Asked of the header too, and only when one is actually
                // written: a run without -p/--print-header puts no header in
                // the output, so there is none there to be read back as a
                // comment. Missing this left the worst case of the three
                // uncounted -- see report_tally.
                let names: Vec<&str> = new_headers.iter().collect();
                tally.record_written(&names, args, true);
                writer.write_record(&new_headers)?;
            }
        }
    }

    // Reuses one StringRecord buffer across every row instead of the
    // `records()` iterator, which clones a fresh StringRecord per row.
    //
    // The row is then planned and written without ever materializing a second
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
            if record.len() < width {
                if let Some(log) = fill_log.as_deref_mut() {
                    log.note(record.position(), record.len(), width, "padded")?;
                }
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
            // it -- 'short' where fix or widest would have padded. Nothing in
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
            .map_err(|error| locate_out_of_bound(error, &record))?;
        tally.record(&edit);
        // Must be built per row: it borrows `record`, which the next
        // read_record() overwrites, so it cannot be hoisted out of the loop.
        let mut fields: Vec<&str> = Vec::with_capacity(record.len() + 2);
        apply_row_edit(&record, args, &edit, &mut fields);
        tally.record_written(&fields, args, false);
        writer.write_record(&fields)?;
        dt_cache = new_dt_cache;
    }

    Ok(tally)
}

// The CSV reader has no error for a quote that is opened and never closed: it
// reads everything after it, newlines included, as one field, so the file
// arrives as a record with too few fields. That is the same error a genuinely
// ragged file gives -- and the documented remedy for raggedness, --flexible,
// accepts the mangled record rather than reporting it, so the message that
// misdirects there is the one to say what else to look for.
fn explain_unequal_lengths(error: csv::Error) -> Box<dyn Error> {
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

// Rows longer than the reference width cannot be made rectangular by padding,
// and shortening them would throw fields away.
//
// Which mode is running decides what to say, because the two reach this for
// opposite reasons. Under 'fix' the width came from the first record and a
// wider row later is ordinary -- that is the documented limit of a single
// pass. Under 'widest' the width came from a pass over the whole input, so no
// row can be too wide for it: getting here at all means the file is not the
// one that was counted, which the message has to say, since 'widest' promises
// exactly the thing that appears to have failed.
fn row_wider_than_reference(row_len: usize, width: usize, mode: FillMode) -> Box<dyn Error> {
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

// Reads the single character an option like --read-delimiter takes.
//
// The CSV reader and writer work in bytes, so the value has to be one ASCII
// character -- a multi-byte one cannot be a byte, and the old check counted
// bytes rather than characters, so it reported a single 'e' as a string. An
// empty value is refused rather than quietly leaving the default in place: it
// is far more often an unset shell variable than a request for the default,
// and silently reading with the wrong delimiter is the kind of thing that is
// noticed several steps later, if at all.
fn parse_single_ascii_char(option: &str, value: &str) -> Result<char, Box<dyn Error>> {
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
            // The message serves three options and used to say "delimiter"
            // for all of them, calling --comment's character by the wrong
            // role.
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

// Explains a column number that will not parse, naming the option and showing
// what it was given. These used to surface Rust's own ParseIntError -- "invalid
// digit found in string", "cannot parse integer from empty string" -- which
// named neither the option nor the value, so `-a "1, 2"` reported a problem
// with a digit without mentioning the space, the list, or --arrange at all.
fn invalid_column(option: &str, value: &str, signed: bool) -> Box<dyn Error> {
    let detail = if value.is_empty() {
        "was given an empty column number. A stray comma leaves one empty, as \
         in \"1,\" or \"0,,2\", and so does an unset shell variable.".to_string()
    } else if value.chars().any(char::is_whitespace) {
        format!("was given \"{value}\", which contains a space. Write it \
                 without one, and a list of columns as 0,2 rather than 0, 2.")
    } else if !signed && value.starts_with('-') {
        format!("was given \"{value}\". Columns are counted from 0 upwards, so \
                 there is no negative one -- the first column is 0.")
    } else if value.chars().all(|character| character.is_ascii_digit()) {
        format!("was given \"{value}\", which is too large to be a column \
                 number.")
    } else {
        format!("was given \"{value}\", which is not a column number. Columns \
                 are named by position, counted from 0.")
    };

    Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData, format!("'{option}' {detail}")))
}

fn parse_column(option: &str, value: &str) -> Result<usize, Box<dyn Error>> {
    value.parse::<usize>().map_err(|_| invalid_column(option, value, false))
}

// -i/--insert's column may be negative, since it is clamped into the row rather
// than refused, so the message for it does not rule a minus sign out.
fn parse_signed_column(option: &str, value: &str) -> Result<i64, Box<dyn Error>> {
    value.parse::<i64>().map_err(|_| invalid_column(option, value, true))
}

// Each column of a "0,2,3" list, reported one value at a time rather than as
// the whole list, since one value is what failed.
fn parse_column_list(option: &str, value: &str) -> Result<Vec<usize>, Box<dyn Error>> {
    value.split(',').map(|part| parse_column(option, part)).collect()
}

// The first column named more than once in a list, if any. Both options that
// take a list refuse a repeat: for -a the list is the new order, so a repeat
// would duplicate a field and leave the row wider than it came in, and for
// --remove it is a miscount -- a list that does not say what it looks like.
fn repeated_column(columns: &[usize]) -> Option<usize> {
    let mut sorted = columns.to_vec();
    sorted.sort_unstable();
    sorted
        .windows(2)
        .find(|pair| pair[0] == pair[1])
        .map(|pair| pair[0])
}

fn repeated_column_error(option: &str, column: usize, because: &str) -> Box<dyn Error> {
    Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("'{option}' names column {column} more than once. Each column \
                 may appear at most once, {because}"),
    ))
}

fn out_of_bound() -> Box<dyn Error> {
    Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "Specified column is out of bound"))
}

// Attaches which row an out-of-bound column was measured against, in the
// same terms the CSV reader's own errors use. The bare message named neither
// the record nor its width, and it bites exactly where that matters: a
// ragged file, where one short row is the whole story and finding it is the
// task. Recognised by the exact message rather than a type, since
// out_of_bound is its only source; the header checks keep the bare form, as
// they are about the arguments rather than any row.
fn locate_out_of_bound(
    error: Box<dyn Error>, record: &csv::StringRecord,
) -> Box<dyn Error> {
    if error.to_string() != "Specified column is out of bound" {
        return error;
    }
    let location = match record.position() {
        Some(position) => format!(
            "record {} (line: {})", position.record(), position.line()),
        None => "this record".to_string(),
    };
    Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("Specified column is out of bound: {} has {} field(s), \
                 counted from 0", location, record.len())))
}

// The marker every conversion writes in place of a result it could not
// produce. One word for all of them, which is what makes `grep -c parse_err`
// the answer to "did anything fail" -- see KNOWN LIMITS.
const PARSE_ERR: &str = "parse_err";

// Whether a moment can be written as RFC3339 at all. Its date-fullyear is
// four digits, so 9999 is the last year there is; chrono reaches far further
// (to +/-262143) and falls back to ISO 8601's expanded form outside that,
// which is a different format wearing the same shape.
//
// The floor is 0001 rather than 0000, which is the one place this range is
// narrower than four digits allow. There is no year zero: the Gregorian and
// Julian calendars run 1 BC straight into AD 1, and ISO 8601 only writes 0000
// for 1 BC by prior agreement between the parties exchanging the data. RFC3339
// makes no such agreement, so a 0000 in a file is a value from outside the
// calendar the rest of the column is in -- far more likely a sentinel or a
// miscounted epoch than a date anybody recorded.
//
// The year is read from the value as it will be rendered, not from its UTC
// year, because a conversion can carry it over the edge without moving the
// instant: 9999-12-31T23:59:59+00:00 is year 10000 on a +14:00 clock while
// still 9999 in UTC. Generic over the zone for that reason -- -u renders in
// UTC, -o in a fixed offset, -l in a named zone, and all three have to be
// asked about the calendar they will actually print.
fn is_rfc3339_year<Zone: chrono::TimeZone>(datetime: &DateTime<Zone>) -> bool {
    (1..=9999).contains(&datetime.year())
}

// Whether the offset a moment will be rendered with can be written as RFC3339
// at all. Its time-offset is "+"/"-" HH ":" MM -- whole minutes, with nowhere
// to put a seconds part.
//
// Only -l can produce one that is not, and it does so from ordinary data. A
// zone's offset before it adopted standard time is the local mean time of the
// place, which is whatever fraction of an hour its longitude works out to:
// Europe/Amsterdam +00:17:30, America/New_York -04:56:02. Nor is that only
// ancient history -- Africa/Monrovia kept -00:44:30 until January 1972, inside
// the range of real logs.
//
// chrono renders those by rounding the offset to the nearest minute while the
// clock time keeps its seconds, so the two halves of the result no longer agree:
// 1971-06-01T12:00:00Z came out as 1971-06-01T11:15:30-00:45, which reads back
// as 12:00:30Z. A timestamp naming a different instant from the one it was made
// from is the quiet wrong answer this tool exists to avoid, so such a value is
// marked instead -- the input is still there beside it to be read.
fn is_rfc3339_offset<Zone: chrono::TimeZone>(datetime: &DateTime<Zone>) -> bool {
    use chrono::Offset;
    datetime.offset().fix().local_minus_utc() % 60 == 0
}

// Renders a converted moment as RFC3339, or the parse_err marker when the
// conversion has taken it outside the format. Writing chrono's expanded form
// instead would put a value in an RFC3339 column that is not RFC3339, cannot be
// read back, and gives no sign of the boundary it crossed. The same goes for a
// sub-minute offset, which chrono renders by rounding the offset alone and so
// moves the instant.
fn to_rfc3339_in_range<Zone: chrono::TimeZone>(datetime: DateTime<Zone>) -> String
where
    Zone::Offset: std::fmt::Display,
{
    match is_rfc3339_year(&datetime) && is_rfc3339_offset(&datetime) {
        true => datetime.to_rfc3339(),
        false => String::from(PARSE_ERR),
    }
}

// Clamps -i's column into a valid index for this record instead of
// producing an out-of-bound error: negative (or otherwise too low) becomes
// the first column, too high becomes the last -- -i's target can never be
// out of bound.
fn clamp_insert_column(value: i64, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    if value < 0 {
        return 0;
    }
    // try_from rather than `as`: on a 32-bit target `as usize` truncates, so
    // a column past 2^32 wrapped to a small index instead of clamping to the
    // last column as documented. A value usize cannot hold is over any
    // record's length by construction, which is the Err arm.
    match usize::try_from(value) {
        Ok(column) if column < len => column,
        _ => len - 1,
    }
}

// Inserts `before`/`after` `column` (or overwrites it, for "replace") with a
// single value, based on the user-facing --insert position string. Shared by
// every action that only ever writes one field, so the before/replace/after
// dispatch exists exactly once instead of once per action.
fn insert_single<'a>(
    vecbuff: &mut Vec<&'a str>,
    column: usize,
    position: &str,
    before: &'a str,
    replace: &'a str,
    after: &'a str,
) {
    match position {
        "before" => {
            vecbuff.insert(column, before);
        },
        "replace" => {
            vecbuff[column] = replace;
        },
        "after" => {
            vecbuff.insert(column+1, after);
        },
        _ => unreachable!(),
    }
}

// Same as `insert_single`, but for actions (split, and its parse-error path)
// that write two adjacent fields.
fn insert_pair<'a>(
    vecbuff: &mut Vec<&'a str>,
    column: usize,
    position: &str,
    before: (&'a str, &'a str),
    replace: (&'a str, &'a str),
    after: (&'a str, &'a str),
) {
    match position {
        "before" => {
            vecbuff.insert(column, before.1);
            vecbuff.insert(column, before.0);
        },
        "replace" => {
            vecbuff[column] = replace.1;
            vecbuff.insert(column, replace.0);
        },
        "after" => {
            vecbuff.insert(column+1, after.1);
            vecbuff.insert(column+1, after.0);
        },
        _ => unreachable!(),
    }
}

// Drops every column in `columns` from `fields`. `columns` must be ordered
// highest index first (as --remove's parsing guarantees), so each removal
// leaves the positions of those still to come untouched.
fn remove_columns(fields: &mut Vec<&str>, columns: &[usize]) {
    for &column in columns {
        fields.remove(column);
    }
}

// Returns `fields` in --arrange's order: the columns it names first, in the
// order given, then every column it didn't name, keeping their original
// relative order. Nothing is dropped and nothing is duplicated, so the row
// always comes out the same width it went in.
//
// `listed` is the same set of columns as `order` but sorted, so deciding
// whether a column was named is a binary search rather than a scan.
fn arranged<'a>(fields: &[&'a str], order: &[usize], listed: &[usize]) -> Vec<&'a str> {
    let mut out: Vec<&'a str> = Vec::with_capacity(fields.len());
    for &column in order {
        out.push(fields[column]);
    }
    for (index, field) in fields.iter().enumerate() {
        if listed.binary_search(&index).is_err() {
            out.push(field);
        }
    }
    out
}

// Parses an RFC3339 timestamp, tolerating a trailing numeric offset that's
// missing its colon (e.g. "+0000" or "-0530" instead of "+00:00"/"-05:30").
// Such timestamps are common in data exported from tools that follow
// RFC2822/ISO 8601's looser offset format instead of strict RFC3339. The
// strict parse is tried first and used as-is when it succeeds, so
// well-formed input pays no extra cost.
fn parse_rfc3339_lenient(value: &str) -> Result<DateTime<FixedOffset>, chrono::ParseError> {
    // Surrounding whitespace is skipped here rather than trimmed out of the
    // field, so a file written with ", " separators converts without --trim
    // having to alter what is passed through. It used to arrive pre-trimmed,
    // which is why --trim defaulted to all -- and that default reached inside
    // quoted fields, throwing away spaces the source had quoted to keep.
    let value = value.trim();
    // chrono accepts any number of fractional digits and silently truncates
    // to nanoseconds -- a tenth digit vanishes with no error. This tool's
    // promise is the opposite, "what the source had is what comes out", so
    // precision a value cannot carry is refused into parse_err rather than
    // quietly cut. The error itself is manufactured by parsing an empty
    // string, since chrono gives no way to construct one.
    if fractional_digits(value) > 9 {
        return chrono::DateTime::parse_from_rfc3339("");
    }
    let strict = chrono::DateTime::parse_from_rfc3339(value);
    let parsed = if strict.is_ok() {
        strict
    } else {
        match insert_offset_colon(value) {
            Some(fixed) => chrono::DateTime::parse_from_rfc3339(&fixed),
            None => strict,
        }
    };
    match parsed {
        Ok(stamp) if !leap_second_is_possible(stamp) => chrono::DateTime::parse_from_rfc3339(""),
        other => other,
    }
}

// Whether a value wearing :60 is somewhere a leap second can be.
//
// chrono reads a :60 seconds field on any minute at all and folds it onto the
// second that follows, so 2024-06-15T14:00:60Z and 14:00:60Z's neighbour
// 14:01:00Z arrive as one instant: -d between them is PT0S, and no conversion
// can tell them apart. That makes a damaged value -- :60 is what a formatter
// that rounds 59.5 up without carrying writes -- read as a different instant
// than the one it names, quietly, where this tool's rule is to mark what it
// cannot answer faithfully. It was written back out wearing :60 as well, and
// on an ordinary minute that is not RFC3339 at all: standard readers refuse
// it, so the output did not read back.
//
// A leap second is the last second of a UTC month and is simultaneous
// everywhere, so the test is on the instant rather than on the clock face the
// record happens to carry: 2017-01-01T05:29:60+05:30 is the real one at the
// end of 2016 and is kept, while 2016-12-31T23:59:60+02:00 reads 23:59:60 on
// its own clock but lands mid-evening in UTC and is not. Structural rather
// than a table of the leap seconds IERS has actually declared: a table would
// refuse every future one until the binary was rebuilt, which is the failure
// the bundled tz data already has to live with and not worth repeating for a
// value the format itself permits.
fn leap_second_is_possible(stamp: DateTime<FixedOffset>) -> bool {
    // chrono carries a leap second as :59 with a nanosecond field run past a
    // whole second, which is the only way to recognise one after parsing.
    if stamp.nanosecond() < 1_000_000_000 {
        return true;
    }
    let utc = stamp.with_timezone(&chrono::Utc);
    utc.hour() == 23 && utc.minute() == 59 && is_last_day_of_month(utc.date_naive())
}

fn is_last_day_of_month(date: chrono::NaiveDate) -> bool {
    match date.succ_opt() {
        Some(next) => next.month() != date.month(),
        // 9999-12-31 has no next day to compare against, and is one.
        None => true,
    }
}

// How many digits follow the first '.', which in anything RFC3339-shaped is
// the start of the fractional seconds -- no other part of the format holds a
// dot. A value where it is something else entirely fails to parse either way.
fn fractional_digits(value: &str) -> usize {
    match value.find('.') {
        None => 0,
        Some(dot) => value[dot + 1..]
            .bytes()
            .take_while(|byte| byte.is_ascii_digit())
            .count(),
    }
}

// If `value` ends in a 4-digit numeric offset with no colon (e.g. "+0000"),
// returns it with a colon inserted (e.g. "+00:00"). Returns None for
// anything else -- including offsets that already have a colon, or a "Z"
// suffix -- so it never touches already-valid RFC3339 strings.
fn insert_offset_colon(value: &str) -> Option<String> {
    if value.len() < 5 {
        return None;
    }
    let sign_index = value.len() - 5;
    let sign = value.as_bytes()[sign_index];
    if sign != b'+' && sign != b'-' {
        return None;
    }
    // Everything below slices `value` by byte index, which panics on a
    // position inside a character. Safe only because the check above found an
    // ASCII byte here: no byte of a multi-byte character can equal '+' or '-',
    // so sign_index is a character boundary, and the four ASCII digits
    // confirmed next make sign_index+3 one as well. A sign accepted in some
    // other alphabet -- Unicode's minus, say -- would need is_char_boundary
    // before any of this. A value ending mid-character otherwise reaches here
    // routinely, since the last five bytes of anything at all are tested.
    let offset_digits = &value[sign_index+1..];
    if offset_digits.len() == 4 && offset_digits.bytes().all(|b| b.is_ascii_digit()) {
        let mut fixed = String::with_capacity(value.len() + 1);
        fixed.push_str(&value[..sign_index+3]);
        fixed.push(':');
        fixed.push_str(&value[sign_index+3..]);
        Some(fixed)
    } else {
        None
    }
}

// Resolves the timezone -l/--local converts into, once at startup rather
// than per row.
//
// The zone is looked up in the IANA database bundled with the binary
// (chrono-tz) instead of read from the host's tzdata files. That keeps a
// conversion reproducible: the same input gives the same output on any
// machine, whatever tzdata the operating system happens to carry -- which
// matters when the result may be relied on as evidence. It also sidesteps a
// defect in chrono's own tzdata reader, which never applies Morocco's
// daylight-saving rule and so reports Africa/Casablanca and Africa/El_Aaiun
// an hour early year-round.
//
// Returns None when TZ holds a POSIX rule string rather than a zone name
// (e.g. "EST5EDT,M3.2.0,M11.1.0"), which the bundled database can't name;
// the caller falls back to chrono's own handling for that case.
// Whether a TZ value spells POSIX rules rather than naming a zone. POSIX
// writes an abbreviation of three or more letters -- or a bracketed form like
// "<+04>" -- followed immediately by its offset: "EST5EDT,M3.2.0,M11.1.0",
// "UTC+3", "GMT0BST,M3.5.0/1,M10.5.0". A zone name never carries a digit or a
// sign in that position, which is what separates rules from a misspelt name.
fn is_posix_tz_rule(value: &str) -> bool {
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

// The standard-time offset a POSIX rule declares, in seconds east of UTC,
// or None if the rule is not shaped the way POSIX says. Only the first
// field is read: the abbreviation, then [+-]hh[:mm[:ss]]. POSIX writes the
// sign the other way round from everyone else -- the value is what must be
// added to local time to reach UTC -- so "EST5" is UTC-5 and "IST-2" is
// UTC+2, and the sign is flipped on the way out.
fn posix_rule_std_offset(value: &str) -> Option<i32> {
    let tail = if let Some(bracketed) = value.strip_prefix('<') {
        &bracketed[bracketed.find('>')? + 1..]
    } else {
        &value[value.chars().take_while(char::is_ascii_alphabetic).count()..]
    };
    let (sign, rest) = match tail.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, tail.strip_prefix('+').unwrap_or(tail)),
    };
    let digits: String = rest.chars()
        .take_while(|c| c.is_ascii_digit() || *c == ':')
        .collect();
    let mut parts = digits.split(':');
    let hours: i32 = parts.next()?.parse().ok()?;
    let minutes: i32 = match parts.next() {
        Some(field) => field.parse().ok()?,
        None => 0,
    };
    let seconds: i32 = match parts.next() {
        Some(field) => field.parse().ok()?,
        None => 0,
    };
    Some(-sign * (hours * 3600 + minutes * 60 + seconds))
}

// Whether chrono made sense of the POSIX rule in TZ, judged by whether it
// agrees with the rule about standard time.
//
// chrono reads the rule itself, and where it cannot -- a transition time
// past 24:00, which real zones publish; Asia/Jerusalem's own footer is
// IST-2IDT,M3.4.4/26,M10.5.0 -- it falls back to UTC without a word. That
// left `-l` answering +00:00 for a rule naming a zone hours away, exit 0:
// the same quiet mistake the unknown-name refusal above exists to stop,
// arrived at by a different road.
//
// Two probes six months apart, because whichever of them falls in standard
// time must carry the offset the rule declares, and which one that is
// depends on the hemisphere. A zone whose standard offset really is UTC
// cannot be told apart from a rule chrono dropped, but then the standard
// half of the answer is right either way.
fn chrono_reads_posix_rule(declared: i32) -> bool {
    use chrono::{Offset, TimeZone};
    [1u32, 7].iter().any(|month| {
        chrono::NaiveDate::from_ymd_opt(2024, *month, 15)
            .and_then(|date| date.and_hms_opt(12, 0, 0))
            .is_some_and(|naive| {
                chrono::Local.offset_from_utc_datetime(&naive)
                    .fix().local_minus_utc() == declared
            })
    })
}

// Resolves the zone -l works in, from TZ or from the platform.
//
// Ok(Some) is a zone found in the bundled database. Ok(None) means the value
// does not name a zone and is left to chrono's own reading -- POSIX rules, or
// the path form glibc allows. Err refuses a value that is neither, because the
// alternative is what this used to do: a misspelt zone fell through to UTC, so
// `-l` reported +00:00 as though it were the zone the user believed they had
// named. That is the same quiet mistake `-o` goes out of its way to refuse,
// and it is worse here, since a plausible timestamp carries no sign of it.
fn local_timezone() -> Result<Option<chrono_tz::Tz>, Box<dyn Error>> {
    let mut from_tz = true;
    let name = match std::env::var("TZ") {
        // POSIX: an empty TZ means UTC.
        Ok(value) if value.is_empty() => String::from("UTC"),
        // A leading ':' is POSIX's implementation-defined form, e.g.
        // ":America/Chicago" -- strip it before looking the zone up.
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
    // a name -- but only when it names a file, since chrono answers UTC for
    // one that does not exist, and a mistyped path deserves the answer a
    // mistyped name gets.
    // Both checks below apply only where chrono reads TZ at all. Windows
    // takes its zone from the system rather than from the environment, so
    // neither form is honoured there in the first place -- '--local's help
    // says as much -- and there is nothing for a wrong value to spoil.
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
        // Left to chrono, which reads the rule form itself -- but checked
        // first, since a rule it cannot read becomes UTC silently.
        if cfg!(unix) && posix_rule_std_offset(&name).is_some_and(|declared| {
            !chrono_reads_posix_rule(declared)
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
    // name the platform supplied sent the user checking the spelling of a
    // variable they never set -- and a system-configured zone cannot be
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

// The range real UTC offsets occupy: -12:00 (Baker Island) to +14:00 (the
// Line Islands). No place has ever used anything outside it.
const MIN_TARGET_OFFSET_SECONDS: i32 = -12 * 3600;
const MAX_TARGET_OFFSET_SECONDS: i32 = 14 * 3600;

// Parses a standalone UTC offset for -o/--offset, e.g. "+02:00", "-0600",
// or "Z". Reuses parse_rfc3339_lenient (so the same "+HHMM" leniency
// applies here too) by wrapping the value in a throwaway timestamp and
// reading back its offset, rather than duplicating offset-parsing logic.
//
// Offsets outside the range real places use are rejected. chrono itself
// accepts anything up to +/-23:59, so without this a typo like "+15:00" for
// "+05:00" would be honoured and produce a plausible-looking timestamp
// carrying an offset that cannot exist -- exactly the sort of quiet error
// this tool otherwise goes out of its way not to make. Only the target the
// user asks for is checked; offsets found in the data are left alone, since
// those are the input's business rather than a mistake in the command.
fn parse_offset(value: &str) -> Result<FixedOffset, Box<dyn Error>> {
    let malformed = || -> Box<dyn Error> {
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "'--offset' expects a UTC offset like +02:00, -0600, or Z"))
    };

    let candidate = format!("1970-01-01T00:00:00{}", value.to_uppercase());
    let datetime = parse_rfc3339_lenient(&candidate).map_err(|_| malformed())?;
    let offset = *datetime.offset();
    let seconds = offset.local_minus_utc();

    if seconds < MIN_TARGET_OFFSET_SECONDS || seconds > MAX_TARGET_OFFSET_SECONDS {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("'--offset' target {} is not a real UTC offset; \
                     they run from -12:00 to +14:00", value))));
    }

    Ok(offset)
}

// Formats a duration as an ISO 8601 duration string: P[n]DT[n]H[n]M[n]S.
// Zero-valued components are omitted (a bare "P" with nothing after it
// would be invalid, so an all-zero duration renders as "PT0S"). Not the
// same as chrono::TimeDelta's own Display, which dumps the whole duration
// as a single raw-seconds "PT<n>S" instead of breaking it into d/h/m/s.
fn format_iso8601_duration(delta: chrono::TimeDelta, round_seconds: bool) -> String {
    // The difference is taken low-to-high at both call sites, but that does
    // not make it non-negative: chrono orders timestamps by their clock
    // fields while subtraction accounts a leap second's extra time, so
    // across 23:59:60 the "later" operand can subtract to less. Taking the
    // magnitude here restores what the formatting below relies on --
    // subsec_nanos() in 0..=999_999_999 -- or the sign of a sub-second
    // delta lands inside the fraction as a literal '-' (PT0.-5S).
    let delta = delta.abs();
    let mut total_seconds = delta.num_seconds();
    let mut nanos = delta.subsec_nanos();

    if round_seconds {
        // Half-up to the nearest second. Only ever reached when the user
        // explicitly passes --round-seconds.
        if nanos >= 500_000_000 {
            total_seconds += 1;
        }
        nanos = 0;
    }

    let days = total_seconds / 86400;
    let hours = (total_seconds / 3600) % 24;
    let minutes = (total_seconds / 60) % 60;
    let seconds = total_seconds % 60;

    let mut result = String::from("P");
    if days != 0 {
        result.push_str(&format!("{}D", days));
    }
    if hours != 0 || minutes != 0 || seconds != 0 || nanos != 0 {
        result.push('T');
        if hours != 0 {
            result.push_str(&format!("{}H", hours));
        }
        if minutes != 0 {
            result.push_str(&format!("{}M", minutes));
        }
        if seconds != 0 || nanos != 0 {
            if nanos != 0 {
                // Zero-pad to nanosecond width, then drop trailing zeros so
                // half a second is ".5" rather than ".500000000".
                let fraction = format!("{:09}", nanos);
                result.push_str(&format!("{}.{}S", seconds, fraction.trim_end_matches('0')));
            } else {
                result.push_str(&format!("{}S", seconds));
            }
        }
    }
    if result == "P" {
        result.push_str("T0S");
    }
    result
}

fn process_headers(headers:csv::StringRecord, args:&ArgumentFlags) -> Result<csv::StringRecord, Box<dyn Error>> {
    let mut vecbuff: Vec<&str> = headers.iter().collect();
    let len = vecbuff.len();
    let position = args.insert_position.as_str();

    if args.action_split {
        if args.selected_column >= len { return Err(out_of_bound()); }
        let target = insert_target(args, args.selected_column, len);
        insert_pair(&mut vecbuff, target, position,
            ("split_date>", "split_time>"),
            ("split_date", "split_time"),
            ("<split_date", "<split_time"));
        return Ok(csv::StringRecord::from(vecbuff));
    }

    if args.action_to_rfc3339 {
        if args.selected_column >= len { return Err(out_of_bound()); }
        let target = insert_target(args, args.selected_column, len);
        insert_single(&mut vecbuff, target, position, "to_rfc3339>", "to_rfc3339", "<to_rfc3339");
        return Ok(csv::StringRecord::from(vecbuff));
    }

    if args.action_to_utc {
        if args.selected_column >= len { return Err(out_of_bound()); }
        let target = insert_target(args, args.selected_column, len);
        insert_single(&mut vecbuff, target, position, "to_utc>", "to_utc", "<to_utc");
        return Ok(csv::StringRecord::from(vecbuff));
    }

    if args.action_to_local {
        if args.selected_column >= len { return Err(out_of_bound()); }
        let target = insert_target(args, args.selected_column, len);
        insert_single(&mut vecbuff, target, position, "to_local>", "to_local", "<to_local");
        return Ok(csv::StringRecord::from(vecbuff));
    }

    if args.action_offset {
        if args.selected_column >= len { return Err(out_of_bound()); }
        let target = insert_target(args, args.selected_column, len);
        insert_single(&mut vecbuff, target, position, "to_offset>", "to_offset", "<to_offset");
        return Ok(csv::StringRecord::from(vecbuff));
    }

    if args.action_duration {
        if args.selected_column >= len { return Err(out_of_bound()); }
        let fallback = match args.second_column {
            Some(second) => {
                if second >= len { return Err(out_of_bound()); }
                second
            },
            None => args.selected_column,
        };
        let target = insert_target(args, fallback, len);
        if args.second_column.is_some() {
            insert_single(&mut vecbuff, target, position, "row_duration>", "row_duration", "<row_duration");
        } else {
            insert_single(&mut vecbuff, target, position, "duration>", "duration", "<duration");
        }
        return Ok(csv::StringRecord::from(vecbuff));
    }

    if args.action_arrange {
        for &column in &args.arrange_columns {
            if column >= len { return Err(out_of_bound()); }
        }
        let reordered = arranged(&vecbuff, &args.arrange_columns, &args.arrange_listed);
        return Ok(csv::StringRecord::from(reordered));
    }

    if args.action_remove {
        for &column in &args.remove_columns {
            if column >= len { return Err(out_of_bound()); }
        }
        remove_columns(&mut vecbuff, &args.remove_columns);
        return Ok(csv::StringRecord::from(vecbuff));
    }

    Ok(csv::StringRecord::from(vecbuff))
}

// The one edit an action makes to a row: which column it targets and the
// value(s) written there. Computed separately from applying it, because the
// owned Strings have to live in the *caller's* frame -- that's what lets the
// field list borrow them and go straight to the CSV writer, with no
// intermediate StringRecord copy per row (see process_stream).
enum RowEdit {
    Passthrough,
    Single { target: usize, value: String },
    Pair { target: usize, first: String, second: String },
    Remove,
    Arrange,
}

impl RowEdit {
    // Whether a conversion ran for this row at all. --remove and -a/--arrange
    // move fields about without reading one, so there is nothing that could
    // have failed; nor is there with no action given.
    fn is_conversion(&self) -> bool {
        matches!(self, RowEdit::Single { .. } | RowEdit::Pair { .. })
    }

    // Whether the conversion could not produce a result for this row. Asked of
    // the edit rather than counted at each of the thirteen places that write
    // the marker, so a conversion added later cannot forget to be counted.
    fn is_parse_error(&self) -> bool {
        match self {
            RowEdit::Single { value, .. } => value == PARSE_ERR,
            RowEdit::Pair { first, second, .. } => {
                first == PARSE_ERR || second == PARSE_ERR
            },
            RowEdit::Passthrough | RowEdit::Remove | RowEdit::Arrange => false,
        }
    }
}

// Whether the writer, told the comment character, still writes a field that
// begins with it bare -- which is what decides whether the field survives as
// data or starts a comment when the output is read back.
//
// The writer quotes it itself under 'necessary' (the comment character is a
// reason to quote, exactly like the delimiter) and under 'always'. The two
// styles left are the two real gaps: 'never' quotes nothing by definition, and
// 'nonnumeric' decides by whether the field is a number and consults nothing
// else -- so --comment '-' over a field like -5 is written bare. That numeric
// test is csv-core's own is_non_numeric, asked directly rather than mirrored,
// so this answer cannot drift from what the writer actually does.
fn writer_leaves_comment_bare(field: &str, style: &str) -> bool {
    match resolve_value(&QUOTE_VALUES, style) {
        Some("never") => true,
        Some("nonnumeric") => !csv_core::is_non_numeric(field.as_bytes()),
        _ => false,
    }
}

// Whether the writer leaves this field bare although it holds the delimiter
// being written -- which is what turns one field into two when the output is
// read back, and every column number after it into a different one.
//
// Only 'nonnumeric' is asked. 'necessary' quotes a field holding the delimiter
// and 'always' quotes every field, so under those two it cannot happen.
// 'never' can do it, and often does, but the help says so in as many words --
// it "never writes quotes, even where that produces invalid CSV data", and
// this is that, announced in advance. 'nonnumeric' says the opposite: quotes
// go on fields that do not need them, which no reader takes to mean a field
// that does need them goes without. So it is the one style that can lose a
// field boundary without having warned anybody.
//
// It takes a delimiter a number can hold -- --print-delimiter '.' over the
// field 5.5, or '-' over -5, or 'e' over 1e5. The numeric test is csv-core's
// own is_non_numeric, asked directly rather than mirrored, so the answer
// cannot drift from what the writer actually does.
fn writer_splits_on_delimiter(field: &str, args: &ArgumentFlags) -> bool {
    if resolve_value(&QUOTE_VALUES, &args.output_quotes) != Some("nonnumeric") {
        return false;
    }
    field.contains(args.output_delimiter.unwrap_or(','))
        && !csv_core::is_non_numeric(field.as_bytes())
}

// Whether a field, as this run writes it, comes back changed when the output
// is read again with '--single-quote'. The writer always quotes with '"' --
// the output is ordinary CSV -- but that reader takes only ''' as a quote, so
// a field the writer quoted keeps its '"' as data and splits on any delimiter
// inside it; and a bare field that itself begins with ''' is taken for a
// quoted one. Reading the output back without '--single-quote' is exact.
//
// The quoted-or-bare question is the writer's, answered per style: 'always'
// quotes everything, 'never' nothing, 'nonnumeric' asks csv-core's own
// is_non_numeric, and 'necessary' quotes a field holding the delimiter, the
// quote, a record boundary, or the comment character -- the same set the
// writer is configured with above, restated here because csv-core does not
// expose its decision.
fn single_quote_misreads(field: &str, args: &ArgumentFlags) -> bool {
    let quoted = match resolve_value(&QUOTE_VALUES, &args.output_quotes) {
        Some("always") => true,
        Some("never") => false,
        Some("nonnumeric") => csv_core::is_non_numeric(field.as_bytes()),
        _ => field.contains(args.output_delimiter.unwrap_or(','))
            || field.contains('"')
            || field.contains('\r')
            || field.contains('\n')
            || args.read_as_comment.is_some_and(|c| field.contains(c)),
    };
    quoted || field.starts_with('\'')
}

// How many rows a conversion was asked for a value for, how many it could not
// produce one for, and how many rows the output starts with the comment
// character in a field the writer left unquoted.
#[derive(Default)]
struct Tally {
    converted: u64,
    failed: u64,
    commented_out: u64,
    // Whether the header is one of them, which is a worse outcome than a data
    // row and so is said separately.
    commented_header: bool,
    // Rows that a '--single-quote' re-read would take apart -- see
    // single_quote_misreads. Only counted when that flag is on.
    single_quote_misread: u64,
    // Rows holding a bare field that itself holds the delimiter being written,
    // so a second read finds more fields in the row than were written -- see
    // writer_splits_on_delimiter. Only reachable under '--quote nonnumeric'.
    delimiter_split: u64,
}

impl Tally {
    fn record(&mut self, edit: &RowEdit) {
        if edit.is_conversion() {
            self.converted += 1;
            if edit.is_parse_error() {
                self.failed += 1;
            }
        }
    }

    // Counted from the fields as they go to the writer, since that is the only
    // point at which what the output will actually look like is known: which
    // field is first depends on --remove, -a/--arrange and -i, and whether it
    // is quoted depends on --quote and on the field's own contents.
    //
    // HEADER says this is the header record. It is asked the same question as
    // any other row -- a column name may start with the comment character as
    // readily as a value can -- but what follows from losing it is different,
    // so it is remembered rather than only counted.
    fn record_written(&mut self, fields: &[&str], args: &ArgumentFlags, header: bool) {
        // Any field of the row can break a '--single-quote' re-read, not only
        // the first: a quoted field splits mid-row wherever it sits. One
        // quoting rule is the record's rather than any field's: a row that is
        // a single empty field is written quoted ("") so the record does not
        // vanish into a blank line -- under every style, 'never' included.
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
        // header is no worse off than a data row here -- both come back one
        // field wider, and the numbering they are read by comes back with them.
        if fields.iter().any(|field| writer_splits_on_delimiter(field, args)) {
            self.delimiter_split += 1;
        }

        let Some(comment) = args.read_as_comment else { return };
        let Some(first) = fields.first() else { return };
        if !first.starts_with(comment) {
            return;
        }
        if writer_leaves_comment_bare(first, &args.output_quotes) {
            self.commented_out += 1;
            self.commented_header |= header;
        }
    }
}

// Says at the end of a run how much of it did not convert.
//
// A few failures among many rows is ordinary -- one malformed line should not
// throw away a large log -- so that stays exit 0 and is reported on stderr,
// where it does not disturb the output but a person can see it. Nothing
// converting at all is a different thing: the column given is not the one
// holding the timestamps, or they are not in a format csvdt reads. That used to
// exit 0 as well, which is indistinguishable from success -- a script would
// carry on and analyse a file of markers.
// Says on stderr that the log has something in it, and what it measured
// against, since the three modes do not agree on that and a reader comparing
// two logs of the same file would otherwise have to guess.
//
// Only when a log was asked for: a run that did not ask has not changed how
// it reports. And said even when the count is zero, because "no rows" is the
// answer a reader wanted -- a log that is only a header line reads like one
// that failed to be written.
fn report_fill_log(rows: u64, args: &ArgumentFlags) {
    let destination = match args.fill_log.as_deref() {
        Some("-") => "standard error".to_string(),
        Some(path) => format!("'{path}'"),
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

fn report_tally(tally: &Tally, comment: Option<char>, delimiter: char)
    -> Result<(), Box<dyn Error>> {
    // Said whatever else the run did, since it is about the output rather than
    // about any conversion -- a --remove or a plain reformat reaches it too.
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
            // column names. That row then stops being data -- a header is
            // exempt from every conversion -- so it is not merely lost, it is
            // silently promoted, and every column number still lines up.
            if tally.commented_header {
                " The header is one of them, so a second read takes the first \
                 data row for the column names, and that row is then exempt \
                 from the conversion rather than missing from it."
            } else {
                ""
            },
            // The usual advice recommends the styles that quote the field --
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
    // promises quotes a field does not need -- so this is the one place the
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

// Resolves -i's column for this row, falling back to the action's own
// default column when -i wasn't given.
fn insert_target(args: &ArgumentFlags, fallback: usize, len: usize) -> usize {
    match args.insert_column {
        Some(value) => clamp_insert_column(value, len),
        None => fallback,
    }
}

// Parses the row and works out what to write, without touching the field
// list. Returns the edit plus the row-over-row duration cache to carry into
// the next row.
fn plan_row_edit(
    record: &csv::StringRecord,
    args: &ArgumentFlags,
    dt_cache: Option<DateTime<FixedOffset>>,
) -> Result<(RowEdit, Option<DateTime<FixedOffset>>), Box<dyn Error>> {
    let len = record.len();
    let parse_err = || String::from(PARSE_ERR);

    if args.action_split {
        if args.selected_column >= len { return Err(out_of_bound()); }
        let target = insert_target(args, args.selected_column, len);
        let (first, second) = match parse_rfc3339_lenient(&record[args.selected_column]) {
            Ok(datetime) => (
                format!("{}", datetime.date_naive()),
                // %.f keeps a fraction the source carried and prints nothing
                // when there is none, as RFC3339 output does. Hard-coded whole
                // seconds dropped it silently, which no other conversion here
                // does: -d keeps it unless --round-seconds asks otherwise, and
                // -u/-l/-o write back whatever came in.
                format!("{}", datetime.format("%H:%M:%S%.f %Z")),
            ),
            Err(_) => (parse_err(), parse_err()),
        };
        return Ok((RowEdit::Pair { target, first, second }, dt_cache));
    }

    if args.action_to_rfc3339 {
        if args.selected_column >= len { return Err(out_of_bound()); }
        let target = insert_target(args, args.selected_column, len);
        // Trimmed for the same reason parse_rfc3339_lenient is.
        let value = match record[args.selected_column].trim().parse::<i64>() {
            Ok(timestamp) => match chrono::DateTime::from_timestamp(timestamp, 0) {
                // chrono reaches far further than RFC3339 does, and formats a
                // year outside four digits in ISO 8601's expanded form
                // ("+57534-06-16T...", "-0001-12-31T..."). That is not RFC3339,
                // which this option promises: csvdt cannot read it back, and
                // nor can anything else expecting the format. Epoch
                // milliseconds handed to an option expecting seconds land here,
                // which is a mistake worth reporting rather than answering with
                // a timestamp no one can use.
                Some(datetime) if is_rfc3339_year(&datetime) => datetime.to_rfc3339(),
                _ => parse_err(),
            },
            Err(_) => parse_err(),
        };
        return Ok((RowEdit::Single { target, value }, dt_cache));
    }

    if args.action_to_utc {
        if args.selected_column >= len { return Err(out_of_bound()); }
        let target = insert_target(args, args.selected_column, len);
        let value = match parse_rfc3339_lenient(&record[args.selected_column]) {
            Ok(datetime) => to_rfc3339_in_range(
                datetime.with_timezone(&chrono::Utc)),
            Err(_) => parse_err(),
        };
        return Ok((RowEdit::Single { target, value }, dt_cache));
    }

    if args.action_to_local {
        if args.selected_column >= len { return Err(out_of_bound()); }
        let target = insert_target(args, args.selected_column, len);
        let value = match parse_rfc3339_lenient(&record[args.selected_column]) {
            Ok(datetime) => match args.local_timezone {
                Some(zone) => to_rfc3339_in_range(datetime.with_timezone(&zone)),
                // TZ held a POSIX rule string rather than a zone name; let
                // chrono parse it the way it always has.
                None => to_rfc3339_in_range(
                    datetime.with_timezone(&chrono::Local)),
            },
            Err(_) => parse_err(),
        };
        return Ok((RowEdit::Single { target, value }, dt_cache));
    }

    if args.action_offset {
        if args.selected_column >= len { return Err(out_of_bound()); }
        let target = insert_target(args, args.selected_column, len);
        let value = match parse_rfc3339_lenient(&record[args.selected_column]) {
            Ok(datetime) => to_rfc3339_in_range(
                datetime.with_timezone(&args.target_offset.unwrap())),
            Err(_) => parse_err(),
        };
        return Ok((RowEdit::Single { target, value }, dt_cache));
    }

    if args.action_duration {
        if args.selected_column >= len { return Err(out_of_bound()); }

        if let Some(second_column) = args.second_column {
            if second_column >= len { return Err(out_of_bound()); }
            let target = insert_target(args, second_column, len);
            let value = match (
                parse_rfc3339_lenient(&record[args.selected_column]),
                parse_rfc3339_lenient(&record[second_column]),
            ) {
                (Ok(dt_a), Ok(dt_b)) => {
                    let duration = if dt_a > dt_b { dt_a - dt_b } else { dt_b - dt_a };
                    format_iso8601_duration(duration, args.round_seconds)
                },
                _ => parse_err(),
            };
            return Ok((RowEdit::Single { target, value }, dt_cache));
        }

        let target = insert_target(args, args.selected_column, len);
        match parse_rfc3339_lenient(&record[args.selected_column]) {
            Ok(datetime) => {
                // No previous row yet (first row, or every row so far failed
                // to parse) means there's nothing to measure against, so the
                // duration is zero. Tracked with None rather than a sentinel
                // timestamp -- a sentinel would make real data holding that
                // exact instant (e.g. the Unix epoch, which is how a lot of
                // exports write a null date) look like "no previous row" and
                // silently report PT0S.
                let previous = dt_cache.unwrap_or(datetime);
                let duration = if datetime > previous {
                    datetime - previous
                } else {
                    previous - datetime
                };
                let value = format_iso8601_duration(duration, args.round_seconds);
                return Ok((RowEdit::Single { target, value }, Some(datetime)));
            },
            // A row we can't parse leaves the cache untouched, so the next
            // parseable row still measures against the last real timestamp.
            Err(_) => {
                let value = parse_err();
                return Ok((RowEdit::Single { target, value }, dt_cache));
            },
        }
    }

    if args.action_arrange {
        for &column in &args.arrange_columns {
            if column >= len { return Err(out_of_bound()); }
        }
        return Ok((RowEdit::Arrange, dt_cache));
    }

    if args.action_remove {
        // Every named column is checked before any is dropped, so a row is
        // either edited fully or reported as out of bound, never left
        // half-removed.
        for &column in &args.remove_columns {
            if column >= len { return Err(out_of_bound()); }
        }
        return Ok((RowEdit::Remove, dt_cache));
    }

    Ok((RowEdit::Passthrough, dt_cache))
}

// Fills `fields` with the row's final contents: the record's own fields,
// borrowed, with `edit` applied. Every &str borrows either the record or the
// edit, so nothing is copied here.
fn apply_row_edit<'a>(
    record: &'a csv::StringRecord,
    args: &ArgumentFlags,
    edit: &'a RowEdit,
    fields: &mut Vec<&'a str>,
) {
    let position = args.insert_position.as_str();
    fields.clear();

    // Handled before the common fill: the output row is built straight from
    // the record in the new order, rather than filled in input order and then
    // permuted, which would need a second buffer to read from.
    if let RowEdit::Arrange = edit {
        for &column in &args.arrange_columns {
            fields.push(&record[column]);
        }
        for (index, field) in record.iter().enumerate() {
            if args.arrange_listed.binary_search(&index).is_err() {
                fields.push(field);
            }
        }
        return;
    }

    fields.extend(record.iter());
    match edit {
        RowEdit::Passthrough => {},
        RowEdit::Single { target, value } => {
            insert_single(fields, *target, position, value, value, value);
        },
        RowEdit::Pair { target, first, second } => {
            insert_pair(fields, *target, position,
                (first, second), (first, second), (first, second));
        },
        RowEdit::Remove => {
            remove_columns(fields, &args.remove_columns);
        },
        // Built and returned above, before the common fill.
        RowEdit::Arrange => unreachable!(),
    }
}

// Convenience wrapper used by the unit tests, which assert on a whole
// StringRecord. The streaming path deliberately skips this and writes the
// borrowed field list straight out instead.
#[cfg(test)]
fn process_record(
    record: &csv::StringRecord,
    args: &ArgumentFlags,
    dt_cache: Option<DateTime<FixedOffset>>,
) -> Result<(csv::StringRecord, Option<DateTime<FixedOffset>>), Box<dyn Error>> {
    let (edit, cache) = plan_row_edit(record, args, dt_cache)?;
    let mut fields: Vec<&str> = Vec::with_capacity(record.len() + 2);
    apply_row_edit(record, args, &edit, &mut fields);
    Ok((csv::StringRecord::from(fields), cache))
}

// Validates and normalizes the position word from -i's second value.
// insert_single/insert_pair also match on this string, but they're called
// deep inside the row-processing hot path -- validating here means a typo
// is a clean startup error instead of an `unreachable!()` panic per row.
fn parse_position_word(word: &str) -> Result<String, Box<dyn Error>> {
    // Resolved to the full word here, so everything downstream matches on
    // "before"/"replace"/"after" and nothing else has to know the
    // abbreviations. They were written out again in each place that dispatched
    // on the result -- the duplication that made '--trim ALL' mean none.
    match resolve_value(&POSITION_VALUES, word) {
        Some(position) => Ok(position.to_string()),
        None => Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "'--insert' expects a position of before, after, or replace"))),
    }
}

// The format version reported on --list-options' first line. Bump it only if
// the meaning of a field changes; adding a comment line or an option does not,
// since consumers are told to skip comments and read fields positionally.
const OPTION_LIST_VERSION: u32 = 1;

// Renders --list-options' output by asking clap what it was given, so the list
// cannot disagree with the options that actually exist. Nothing here names an
// option: a new .arg() appears in the output with no change to this function.
fn option_list(command: &clap::Command) -> String {
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

// The one-line form of a help text: the sentence it opens with.
//
// -h and --help are one text at two depths rather than two texts. A second
// corpus of one-liners would be a second thing to keep true, and the first
// thing to go stale -- which is the argument this program already makes for
// generating its manual page rather than keeping one by hand. Every file in
// src/help opens with a sentence saying what the option is; the summary is
// that sentence, taken rather than written again.
//
// A sentence ends at a full stop outside brackets. Inside them it does not:
// four entries carry an "(e.g. -o1,-06:00)" whose stop would cut the line in
// half, and the syntax it shows is the part a reader of the short help most
// needs. Reflowed, because the corpus is wrapped for the long form and the
// short one is a single sentence for clap to wrap to the terminal.
fn help_summary(text: &str) -> String {
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

// The column the whole help corpus is written to.
const HELP_COLUMNS: usize = 64;

// TEXT folded to that column.
//
// clap only wraps text with its wrap_help feature, which this program does
// not take: the corpus is folded by hand instead, so that what a reader sees
// is what was written and a table stays a table. A summary is made rather
// than written, so it has to be folded here or not at all -- left flowing,
// one ran a single line of -h out to 211 columns.
//
// A word naming an option is joined to the word before it, and the pair is
// what gets placed. In the description column a line beginning '--' reads as
// the start of a new entry, to someone skimming and to anything parsing the
// output. Two earlier versions mended a bad break after making it -- move
// the word before down, then keep moving until the line no longer starts
// with '--' -- and each was wrong in its own way: the first left
// '--version or ...' at a line start where two option names fell together,
// the second pushed lines past 64 columns by adding to a line already full.
// Nothing to mend if the break cannot be made.
fn wrapped(text: &str) -> String {
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

// The opening paragraph of TEXT, for the same reason: the short help wants
// the introduction, and the paragraphs after it are reference material that
// a reader asking for a summary did not ask for.
//
// Keeps the newline the paragraph ends on, which is the blank line the help
// template puts between the introduction and the usage line.
// A paragraph ends at a blank line, which is not the same as ending at
// "\n\n": git hands a Windows checkout the corpus with CRLF endings, where
// the blank line is "\r\n\r\n" and the shorter pattern is nowhere in the
// file. Looking for "\n\n" there found nothing, so -h printed the whole
// introduction -- the durations paragraph included -- on the one platform
// nobody reads the help on while writing it.
fn first_paragraph(text: &str) -> &str {
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if line.trim().is_empty() {
            return &text[..at];
        }
        at += line.len();
    }
    text
}

fn arguments(mut args: ArgumentFlags) -> Result<ArgumentFlags, Box<dyn Error>> {
    const TEMPLATE: &str = include_str!("./help/help_template.txt");
    const ABOUT: &str = include_str!("./help/about.txt");
    const FILE: &str = include_str!("./help/file.txt");
    const HAS_HEADER: &str = include_str!("./help/has_header.txt");
    const PRINT_HEADER: &str = include_str!("./help/print_header.txt");
    const FLEXIBLE: &str = include_str!("./help/flexible.txt");
    const POSITION: &str = include_str!("./help/position.txt");
    const TRIM: &str = include_str!("./help/trim.txt");
    const READ_DELIMITER: &str = include_str!("./help/read_delimiter.txt");
    const SINGLE_QUOTES: &str = include_str!("./help/single_quotes.txt");
    const COMMENT: &str = include_str!("./help/comment.txt");
    const FILL_LOG: &str = include_str!("./help/fill_log.txt");
    const PRINT_DELIMITER: &str = include_str!("./help/print_delimiter.txt");
    const QUOTES: &str = include_str!("./help/quotes.txt");
    const SPLIT: &str = include_str!("./help/split.txt");
    const RFC3339: &str = include_str!("./help/rfc3339.txt");
    const UTC: &str = include_str!("./help/utc.txt");
    const LOCAL: &str = include_str!("./help/local.txt");
    const DURATION: &str = include_str!("./help/duration.txt");
    const ROUND_SECONDS: &str = include_str!("./help/round_seconds.txt");
    const OFFSET: &str = include_str!("./help/offset.txt");
    const REMOVE: &str = include_str!("./help/remove.txt");
    const ARRANGE: &str = include_str!("./help/arrange.txt");
    const LIST_OPTIONS: &str = include_str!("./help/list_options.txt");
    const PEEK: &str = include_str!("./help/peek.txt");
    const GENERATE_MAN: &str = include_str!("./help/generate_man.txt");
    // Not help text: roff, for the page only. See insert_man_sections.
    const MAN_SUPPLEMENT: &str = include_str!("./man/supplement.roff");
    // The one trailing section clap allows, composed from the three files.
    // include_str! expands to a literal, so concat! can join them.
    const TRAILING: &str = concat!(
        // First of the trailing sections, where a man page puts EXAMPLES:
        // after the options, before the reference material.
        include_str!("./help/examples.txt"),
        include_str!("./help/formats.txt"),
        include_str!("./help/limits.txt"),
        include_str!("./help/exit_status.txt"),
    );
    // The same, plus where to go next. The page does not get this one: it
    // ends in a SEE ALSO that says it already, and better, since a page is
    // read where the other pages are. concat! wants literals, so the four
    // are named again rather than TRAILING being extended.
    const TRAILING_LONG: &str = concat!(
        include_str!("./help/examples.txt"),
        include_str!("./help/formats.txt"),
        include_str!("./help/limits.txt"),
        include_str!("./help/exit_status.txt"),
        include_str!("./help/documentation.txt"),
    );
    // What -h ends with instead: that it is a summary, and what the long
    // form adds. clap already appends "(see more with '--help')" to the help
    // entry itself, which says where to look but not what is there.
    const MORE_HELP: &str = include_str!("./help/more_help.txt");
    // The headings those three files open with. A man page wants them as
    // sections rather than as capitalised lines inside one, and taking them
    // from here rather than matching on shape keeps a heading from being
    // invented out of ordinary prose -- KNOWN LIMITS appears mid-sentence
    // three times in the option help. A test renders the page and looks for
    // each of these, so a file renamed without this list is a failure rather
    // than a section quietly lost.
    const TRAILING_HEADINGS: [&str; 4] =
        ["EXAMPLES", "TIMESTAMP FORMATS", "KNOWN LIMITS", "EXIT STATUS"];

    // The bundled IANA database version is part of the tool's identity: it
    // determines every -l/--local result, so output can be tied back to a
    // known ruleset.
    //
    // Published builds also carry semver build metadata identifying the exact
    // build -- e.g. "1.0.1-pre+2026.07.a1b2c3d" -- set by the release workflow
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

    let command = clap::Command::new("csvdt")
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
        // once a narrow one folds it. A ceiling, not a width -- a terminal
        // narrower than this still gets its own.
        .max_term_width(80)
        .after_help(MORE_HELP)
        .after_long_help(TRAILING_LONG)
        .arg(
            clap::Arg::new("input_file")
                .action(clap::ArgAction::Set)
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
                // mode, which is the trap -d was already fixed for.
                .require_equals(true)
                .default_missing_value("keep")
                .ignore_case(true)
                .value_parser(abbreviated_values(&FLEXIBLE_VALUES))
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
                .conflicts_with_all([
                    "action_to_utc",
                    "action_to_local",
                    "action_split",
                    "action_duration",
                    "action_offset",
                    "action_remove",
                    "action_arrange"
                ])
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
                .conflicts_with_all([
                    "action_to_rfc3339",
                    "action_to_local",
                    "action_split",
                    "action_duration",
                    "action_offset",
                    "action_remove",
                    "action_arrange"
                ])
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
                .conflicts_with_all([
                    "action_to_utc",
                    "action_to_rfc3339",
                    "action_split",
                    "action_duration",
                    "action_offset",
                    "action_remove",
                    "action_arrange"
                ])
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
                .conflicts_with_all([
                    "action_to_utc",
                    "action_to_local",
                    "action_to_rfc3339",
                    "action_split",
                    "action_duration",
                    "action_remove",
                    "action_arrange"
                ])
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
                .conflicts_with_all([
                    "action_to_utc",
                    "action_to_local",
                    "action_to_rfc3339",
                    "action_duration",
                    "action_offset",
                    "action_remove",
                    "action_arrange"
                ])
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
                .conflicts_with_all([
                    "action_to_utc",
                    "action_to_local",
                    "action_to_rfc3339",
                    "action_split",
                    "action_offset",
                    "action_remove",
                    "action_arrange"
                ])
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
                .conflicts_with_all([
                    "action_to_utc",
                    "action_to_local",
                    "action_to_rfc3339",
                    "action_split",
                    "action_duration",
                    "action_offset",
                    "action_arrange",
                ])
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
                .conflicts_with_all([
                    "action_to_utc",
                    "action_to_local",
                    "action_to_rfc3339",
                    "action_split",
                    "action_duration",
                    "action_offset",
                    "action_remove",
                ])
        )

        .arg(
            clap::Arg::new("trim")
                .action(clap::ArgAction::Set)
                .value_name("trim")
                .help(help_summary(TRIM))
                // Trimmed, because clap writes "\n\n[default: ...]" after a
                // long help when the option has one -- and the corpus file
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
                .value_parser(abbreviated_values(&TRIM_VALUES)),
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
                .value_parser(abbreviated_values(&QUOTE_VALUES))
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
                // welcome, and the ones that write are refused -- there is
                // no CSV here for them to act on.
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
        // over the column everything else was folded to -- one of them at
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
        });

    // Parsed from a clone so the Command itself survives to be introspected
    // below; get_matches consumes what it is called on.
    //
    // try_ rather than get_matches so --help and --version come back here
    // instead of being printed on clap's way out. clap prints them with the
    // same 'let it fail quietly' that --generate-man used to have, and there
    // is nothing of csvdt's left to ask by then: 'csvdt --help > page.txt' on
    // a full disk wrote nothing and reported 0, the one answer that must
    // never be wrong. Handed to answer() they are written like every other
    // thing csvdt says about itself -- a closed pipe passes, anything else
    // ends the run at 1.
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

    // Answered before anything else, and before any input is opened: this is
    // how a wrapper discovers the options, so it must work on a bare invocation.
    if matches.get_flag("list_options") {
        answer(option_list(&command).as_bytes());
        std::process::exit(0);
    }

    // Answered here too, and for the same reason: the page is rendered from
    // the Command above rather than kept beside it, so it describes this
    // build and cannot fall behind it.
    if matches.get_flag("generate_man") {
        let mut rendered = Vec::new();
        // NAME wants one line and DESCRIPTION wants the rest, and the
        // Command's own 'about' is a paragraph -- right where -h prints it,
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
        let rendered = promote_man_headings(&rendered, &TRAILING_HEADINGS);
        // After the sections above it, so a line that is one of those is
        // already .SH and cannot be taken for a subsection of itself, and
        // before the pass below, so a heading is never inside a held block.
        let rendered = promote_man_subheadings(&rendered);
        let rendered = break_after_option_tags(&rendered);
        let rendered = preserve_aligned_blocks(&rendered);
        let rendered = insert_man_sections(&rendered, MAN_SUPPLEMENT);
        // Last, so the two passes above still match on the lines clap_mangen
        // wrote, and so the supplement's own option names are protected too.
        let rendered = protect_option_names(&rendered);
        // Last, so nothing above can leave a space at the end of a line.
        let rendered = drop_trailing_space(&rendered);
        answer(&rendered);
        std::process::exit(0);
    }

    if let Some(value) = matches.get_one::<String>("input_file") {
        // An empty file argument is an unset shell variable -- "$file" that
        // expanded to nothing -- far more often than a request for standard
        // input, which is what leaving it out already means. Read as "no
        // file given" it silently consumed the pipe, or waited on a
        // terminal, where every other unreadable path is named.
        if value.is_empty() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "the file argument is empty, which is usually an unset shell \
                 variable. Give the file you meant, or leave the argument out \
                 to read standard input".to_string())));
        }
        args.input_file = value.to_string();
    }

    if let Some(value) = matches.get_one::<String>("trim") {
        args.whitespace_trim = value.to_string();
    }

    if let Some(value) = matches.get_one::<String>("read_delimiter") {
        args.input_delimiter = Some(parse_single_ascii_char("--read-delimiter", value)?);
    }

    if let Some(value) = matches.get_one::<String>("comment") {
        args.read_as_comment = Some(parse_single_ascii_char("--comment", value)?);
    }

    if let Some(value) = matches.get_one::<String>("fill_log") {
        args.fill_log = Some(value.clone());
    }

    if let Some(value) = matches.get_one::<String>("action_split") {
        args.action_split = true;
        args.selected_column = parse_column("--split", value)?;
    }

    if let Some(value) = matches.get_one::<String>("action_to_rfc3339") {
        args.action_to_rfc3339 = true;
        args.selected_column = parse_column("--rfc3339", value)?;
    }

    if let Some(value) = matches.get_one::<String>("action_to_utc") {
        args.action_to_utc = true;
        args.selected_column = parse_column("--utc", value)?;
    }

    if let Some(value) = matches.get_one::<String>("action_to_local") {
        args.action_to_local = true;
        args.selected_column = parse_column("--local", value)?;
        // Resolved once here rather than per row.
        args.local_timezone = local_timezone()?;
    }

    if let Some(value) = matches.get_one::<String>("action_remove") {
        args.action_remove = true;
        let mut columns = parse_column_list("--remove", value)?;
        // Refused rather than quietly deduplicated. Removing one column twice
        // is not what the list says, and the difference used to be invisible:
        // --arrange has always refused a repeat, so the two lists disagreed
        // about the same mistake.
        if let Some(column) = repeated_column(&columns) {
            return Err(repeated_column_error(
                "--remove", column,
                "since removing one twice is not something a list can ask for"));
        }
        // Sorted highest first so removing one never shifts the position of
        // another still to come.
        columns.sort_unstable_by(|a, b| b.cmp(a));
        args.remove_columns = columns;
    }

    if let Some(value) = matches.get_one::<String>("action_arrange") {
        args.action_arrange = true;
        let columns = parse_column_list("--arrange", value)?;
        if let Some(column) = repeated_column(&columns) {
            return Err(repeated_column_error(
                "--arrange", column, "since the list is the new column order"));
        }
        // Sorted copy so deciding "was this column listed?" while walking a row
        // is a binary search rather than a scan of the whole list. The list
        // itself keeps the order given -- that order *is* the output order.
        let mut listed = columns.clone();
        listed.sort_unstable();
        args.arrange_columns = columns;
        args.arrange_listed = listed;
    }

    if let Some(value) = matches.get_one::<String>("action_duration") {
        args.action_duration = true;
        match value.split_once(',') {
            Some((first, second)) => {
                args.selected_column = parse_column("--duration", first)?;
                let second = parse_column("--duration", second)?;
                // Refused rather than answered. The span between a column and
                // itself is PT0S on every row, which looks like a measurement
                // and is not one -- and unlike parse_err, nothing in the output
                // says so. --remove and --arrange already refuse a column named
                // twice; the same mistake here was the only one that produced a
                // full column of plausible results instead.
                if second == args.selected_column {
                    return Err(repeated_column_error(
                        "--duration", second,
                        "since the span between a column and itself is PT0S on \
                         every row rather than a duration"));
                }
                args.second_column = Some(second);
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
    // goes with it. So `-u0 --round-seconds` was accepted and the flag then did
    // nothing -- -u keeps whatever precision the input carried, and the run
    // looked like it had honoured the request.
    if args.round_seconds && !args.action_duration {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "'--round-seconds' rounds the result of '-d/--duration', and there \
             is no '-d' here. It is not a general precision setting: every other \
             conversion writes back whatever precision the timestamp carried, \
             rather than rounding it.")));
    }

    if let Some(value) = matches.get_one::<String>("action_offset") {
        args.action_offset = true;
        let (column_word, offset_word) = value.split_once(',').ok_or_else(|| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "'--offset' expects <column>,<offset>, e.g. -o1,-06:00"))
        })?;
        args.selected_column = parse_column("--offset", column_word)?;
        args.target_offset = Some(parse_offset(offset_word)?);
    }

    if let Some(value) = matches.get_one::<String>("position") {
        let (column_word, position_word) = value.split_once(',').ok_or_else(|| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "'--insert' expects <column>,<position>, e.g. -i1,replace"))
        })?;
        args.insert_column = Some(parse_signed_column("--insert", column_word)?);
        args.insert_position = parse_position_word(position_word)?;
    } else {
        args.insert_position = "after".to_string();
    }

    if let Some(value) = matches.get_one::<String>("print_delimiter") {
        args.output_delimiter = Some(parse_single_ascii_char("--print-delimiter", value)?);
    }

    if let Some(value) = matches.get_one::<String>("quotes") {
            args.output_quotes = value.to_lowercase().to_string();
    }

    args.has_headers = matches.get_flag("has_header");
    args.peek = matches.get_flag("peek");
    args.output_header = matches.get_flag("print_header");
    args.input_quotes = matches.get_flag("single_quotes");
    if let Some(mode) = matches.get_one::<String>("flexible") {
        args.flexible_record = true;
        // Resolved rather than matched literally, so -f=FIX cannot pass clap
        // and then land on the fallback -- which is what '--trim ALL' did.
        args.fill_mode = match resolve_value(&FLEXIBLE_VALUES, mode) {
            Some("fix") => FillMode::FirstRecord,
            Some("widest") => FillMode::Widest,
            _ => FillMode::Off,
        };
    }

    check_special_characters(&args)?;
    Ok(args)
}

// The name a message should use for a character with no useful printable form.
fn describe_character(character: char) -> String {
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

// Refuses delimiter and comment characters that cannot do the job asked of
// them, as opposed to ones that merely look unusual. Each of these was
// accepted, and each then produced output that could not be read back, or
// dropped rows -- the same quiet wrongness -o refuses an impossible offset for.
//
// A tab is not among them: that is how TSV is read and written.
fn check_special_characters(args: &ArgumentFlags) -> Result<(), Box<dyn Error>> {
    let invalid = |message: String| -> Box<dyn Error> {
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, message))
    };

    // The reader's quote character follows --single-quote; the writer always
    // quotes with '"'.
    let read_quote = if args.input_quotes { '\'' } else { '"' };
    let read_delimiter = args.input_delimiter.unwrap_or(',');

    // A record ends at one of these, so nothing within one can be marked by
    // one: what follows it is the next row. Given as a delimiter it silently
    // made the whole line, or the whole file, into a single field.
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
    // Reading came out silently misparsed and writing came out unreadable,
    // where both should have been refused.
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
    // the tally too -- it inspects the first field, and these lines gain the
    // comment character from the delimiter or quote in front of the field.
    // Either way rows leave the file and nothing is said about it.
    //
    // The write side turns on the quoting style, though. 'always' and
    // 'nonnumeric' both quote an empty first field, writing it as "", so no
    // line under them can begin with the bare delimiter; and 'never' quotes
    // nothing, so no line under it gains a quote character the field did not
    // hold -- a field that starts with one itself is exactly what the tally
    // counts. Under those styles the collision cannot happen and the pairing
    // stays accepted, as it always was.
    if let Some(comment) = args.read_as_comment {
        let write_delimiter = args.output_delimiter.unwrap_or(',');
        let style = resolve_value(&QUOTE_VALUES, &args.output_quotes);
        let empty_field_written_bare = !matches!(style, Some("always") | Some("nonnumeric"));
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
        } else if comment == '"' && style != Some("never") {
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

#[cfg(test)]
mod tests {

    // A paragraph ends at a blank line, and a blank line is not always
    // "\n\n". git hands a Windows checkout the corpus with CRLF endings,
    // where it is "\r\n\r\n" and the shorter pattern is nowhere in the
    // file -- so -h printed the whole introduction there, the durations
    // paragraph included, and only Windows CI noticed.
    //
    // Asked here because it cannot be asked of the binary: this checkout has
    // one set of endings, and the question is about the other.
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

    // Folding a summary must not put an option name at the start of a line:
    // in the description column that reads as the start of a new entry, to
    // someone skimming and to anything parsing the output. Two earlier
    // versions mended a bad break after making it, and each broke something
    // else -- a second option name arriving at the line start, and lines
    // pushed past the column everything else was folded to.
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

    // A run of two lines with a column gap is a table even with nothing
    // indented, which is what --trim and --list-options were. Written as a
    // unit test because the corpus could not ask it: every table there had
    // three such lines, so raising the threshold to three changed nothing
    // in the manual page and would go unnoticed until the next two-row
    // table was written.
    //
    // It is now the only place that asks at all. Every table in the corpus
    // was re-laid as a tag on its own line with its description indented
    // under it, so all of them are indented and the indent alone answers
    // for them. The integration test that used to cover this had --trim for
    // a fixture and lost it; the signal it covers is still live, for the
    // next table someone writes flush against the margin.
    #[test]
    fn two_gapped_lines_are_a_table_and_one_is_not() {
        let table = b"none    Preserves fields and headers.\n\
                      all     Trim whitespace from both.\n";
        let held = String::from_utf8(super::preserve_aligned_blocks(table))
            .unwrap();
        assert!(held.contains(".br\nall     Trim"),
                "a two-row table lost its break:\n{held}");

        // And a lone gap inside a sentence is not a table: prose has to go
        // on filling to the width it is read at.
        let prose = b"The quoting style to use when writing CSV, which\n\
                      decides when a field is  quoted at all.\n";
        let filled = String::from_utf8(super::preserve_aligned_blocks(prose))
            .unwrap();
        assert!(!filled.contains(".br"),
                "prose was held unfilled by a single gap:\n{filled}");
    }

    // Padding comes off the end of a line; the line ending does not.
    //
    // The carriage return is the case that matters, and the one that got
    // away: on Windows the corpus is checked out CRLF, so a blank line inside
    // an option's long help is ten spaces and a CR. A trim that knew only
    // spaces and tabs stopped at the CR and left all ten -- 59 lines of
    // --help on windows-latest against none on Linux and macOS.
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

    // The three treatments in one block: prose at the margin fills, prose
    // indented under it fills inside an indent of its own, and a marked
    // block is written out as it stands.
    //
    // An indent alone used to hold a block, so the middle one froze at the
    // width the help file is wrapped to while the paragraph above it ran to
    // the terminal's. Read at 100 columns, DESCRIPTION filled to 100 and
    // 'INPUT MUST BE UTF-8' three lines below it stopped at 64.
    #[test]
    fn the_gutter_decides_what_is_held_and_what_fills() {
        let block = b"A byte-order mark is stripped if present. Anything\n\
                      else is refused rather than guessed at:\n\
                      \x20 cp1252, which Excel writes on a Western locale,\n\
                      \x20 fails naming the record and the line it is in.\n\
                      |    iconv -f CP1252 -t UTF-8 in.csv > out.csv\n\
                      |    iconv -f UTF-16 -t UTF-8 in.csv > out.csv\n\
                      A field that genuinely holds NUL bytes is passed\n\
                      through untouched.\n";
        let out = String::from_utf8(super::preserve_aligned_blocks(block))
            .unwrap();

        // Prose at the margin fills, above the block and below it.
        assert!(!out.contains("Anything\n.br\n"),
                "prose above a list was held unfilled:\n{out}");
        assert!(!out.contains("passed\n.br\nthrough untouched"),
                "prose after a held block was itself held:\n{out}");

        // The nested paragraph is handed its indent and fills inside it,
        // rather than being broken line by line.
        assert!(out.contains(".RS 2n\ncp1252, which Excel"),
                "nested prose was not given an indent of its own:\n{out}");
        assert!(!out.contains("Western locale,\n.br\n"),
                "nested prose was held unfilled:\n{out}");

        // The marked block is held exactly as written, and the gutter that
        // said so never reaches the page.
        assert!(out.contains("    iconv \\-f CP1252 -t UTF-8 in.csv > out.csv\n.br\n")
                    || out.contains("    iconv -f CP1252 -t UTF-8 in.csv > out.csv\n.br\n"),
                "a marked block lost its break:\n{out}");
        assert!(!out.contains(super::VERBATIM),
                "the verbatim gutter reached the page:\n{out}");

        // And the line that resumes prose after a held block needs a break
        // of its own, or roff pulls it up onto the last line held.
        assert!(out.contains(".br\nA field that genuinely"),
                "prose after a held block was not broken from it:\n{out}");
    }

    // The gutter is the only thing a strip takes off, and it comes off every
    // line that carries one. Everything a reader sees goes through this.
    #[test]
    fn stripping_a_gutter_leaves_the_line_as_it_was() {
        let marked = "Convert first if needed:\n|    iconv -f CP1252 in.csv\n\
                      |    iconv -f UTF-16 in.csv\nA field that holds NUL\n";
        assert_eq!(super::strip_marks(marked),
                   "Convert first if needed:\n    iconv -f CP1252 in.csv\n\
                    \x20   iconv -f UTF-16 in.csv\nA field that holds NUL\n");

        // A pipe anywhere but the first column is data: '[=<keep|fix|widest>]'
        // is a value syntax --help prints, and it must come through whole.
        let value = "Takes an optional mode [=<keep|fix|widest>]\n";
        assert_eq!(super::strip_marks(value), value);
    }

    // '\%' asks roff not to hyphenate the word it opens, and asks for a
    // break where it falls inside one. The two are the same escape, so this
    // pass is only safe while it can tell a hyphen that opens a word from
    // one that sits inside it -- which is the whole of what is tested here.
    #[test]
    fn option_names_are_protected_and_dates_are_left_alone() {
        let protect = |text: &[u8]| {
            String::from_utf8(super::protect_option_names(text)).unwrap()
        };

        // A name in running text, and the same name after an opening
        // bracket -- the second is mid-word by the only test that matters
        // to roff, which is that no space precedes it.
        let prose = b"for a two\\-column \\-\\-duration). This column\n";
        assert_eq!(protect(prose),
                   "for a two\\-column \\%\\-\\-duration). This column\n");

        // The whole argument of a font macro. The request keeps its own
        // first word; only what follows may be marked.
        let macro_line = b".B \\-\\-flexible=widest\n";
        assert_eq!(protect(macro_line), ".B \\%\\-\\-flexible=widest\n");

        // And the case that must not be touched. Every hyphen in a date is
        // internal, so marking one would offer roff a break it did not have
        // before and split an example row down the middle.
        let row = b"7,2023\\-11\\-14T22:13:20+01:00,info\n";
        assert_eq!(protect(row), "7,2023\\-11\\-14T22:13:20+01:00,info\n");

        // A hyphen that opens a word at the very start of a line is still
        // one that opens a word.
        let opening = b"\\-\\-peek numbers the columns\n";
        assert_eq!(protect(opening), "\\%\\-\\-peek numbers the columns\n");

        // A request that is not a font macro keeps its argument as it is.
        assert_eq!(protect(b".TP \\-\\-quote\n"), ".TP \\-\\-quote\n");

        // A name later in a font macro's argument, which is the case the
        // macro branch used to miss: it marked an argument only when the
        // whole of it was a name, so this reached the page unprotected.
        let inside = b".I the destination given to \\-\\-fill\\-log\n";
        assert_eq!(
            protect(inside),
            ".I the destination given to \\%\\-\\-fill\\-log\n",
        );

        // And the same argument read the same way as running text, so a
        // date inside one is left alone exactly as a date in a paragraph is.
        let dated = b".I 2023\\-11\\-14T22:13:20+01:00\n";
        assert_eq!(protect(dated), ".I 2023\\-11\\-14T22:13:20+01:00\n");
    }
    use super::*;

    fn default_args() -> ArgumentFlags {
        ArgumentFlags {
            insert_position: "after".to_string(),
            ..Default::default()
        }
    }

    fn headers(fields: &[&str]) -> csv::StringRecord {
        csv::StringRecord::from(fields.to_vec())
    }

    fn record(fields: &[&str]) -> csv::StringRecord {
        csv::StringRecord::from(fields.to_vec())
    }

    fn as_vec(record: &csv::StringRecord) -> Vec<&str> {
        record.iter().collect()
    }

    // ---- open_input ------------------------------------------------------

    #[test]
    fn a_directory_is_reported_as_one_on_either_platform() {
        // The platforms differ underneath: Unix opens a directory and fails at
        // the first read, Windows refuses the open as access denied. Neither
        // message says "directory", so the diagnosis is made on this side --
        // and this runs on both, which is the only way to know it holds.
        let directory = std::env::temp_dir();
        let error = open_input(&directory.to_string_lossy())
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

    fn names(values: &[PossibleValue]) -> Vec<String> {
        values.iter().map(|value| value.get_name().to_string()).collect()
    }

    #[test]
    fn abbreviations_are_every_prefix_only_one_value_has() {
        // 'n' is left out: necessary, never and nonnumeric all start with it,
        // and resolving it to one of them would be a guess.
        assert_eq!(
            names(&abbreviated_values(&QUOTE_VALUES)),
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
            names(&abbreviated_values(&TRIM_VALUES)),
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
            assert_eq!(resolve_value(&TRIM_VALUES, given), Some("all"), "{given}");
        }
        assert_eq!(resolve_value(&QUOTE_VALUES, "NEV"), Some("never"));
        assert_eq!(resolve_value(&QUOTE_VALUES, "nonnumeric"), Some("nonnumeric"));
    }

    #[test]
    fn an_ambiguous_or_unknown_value_resolves_to_nothing() {
        // Every value clap accepts resolves, so these cannot be reached
        // through the command line -- the point is that if the two ever part
        // company again, the result is no match rather than a wrong one.
        for given in ["n", "ne", "", "x", "alll", "fieldss"] {
            assert_eq!(resolve_value(&QUOTE_VALUES, given), None, "{given}");
        }
    }

    #[test]
    fn every_value_clap_accepts_also_resolves() {
        // The drift this replaced: clap accepted a value that then matched
        // nothing. Every accepted form of all four value lists has to resolve,
        // so no option can gain a value it does not then act on.
        let lists: [&[&'static str]; 4] = [
            &TRIM_VALUES, &QUOTE_VALUES, &POSITION_VALUES, &FLEXIBLE_VALUES,
        ];
        for values in lists {
            for accepted in names(&abbreviated_values(values)) {
                assert!(
                    resolve_value(values, &accepted).is_some(),
                    "clap accepts {accepted:?}, which resolves to nothing",
                );
            }
        }
    }

    // ---- process_headers -------------------------------------------------

    #[test]
    fn headers_no_action_are_unchanged() {
        let args = default_args();
        let result = process_headers(headers(&["id", "ts", "val"]), &args).unwrap();
        assert_eq!(as_vec(&result), vec!["id", "ts", "val"]);
    }

    #[test]
    fn headers_split_default_position_is_after_own_column() {
        let mut args = default_args();
        args.action_split = true;
        args.selected_column = 1;
        let result = process_headers(headers(&["id", "ts", "val"]), &args).unwrap();
        assert_eq!(
            as_vec(&result),
            vec!["id", "ts", "<split_date", "<split_time", "val"]
        );
    }

    #[test]
    fn headers_split_explicit_insert_position() {
        for (col, mode, expected) in [
            (1, "before", vec!["id", "split_date>", "split_time>", "ts", "val"]),
            (1, "replace", vec!["id", "split_date", "split_time", "val"]),
            (1, "after", vec!["id", "ts", "<split_date", "<split_time", "val"]),
        ] {
            let mut args = default_args();
            args.action_split = true;
            args.selected_column = 1;
            args.insert_column = Some(col);
            args.insert_position = mode.to_string();
            let result = process_headers(headers(&["id", "ts", "val"]), &args).unwrap();
            assert_eq!(as_vec(&result), expected, "mode={mode}");
        }
    }

    #[test]
    fn headers_split_insert_column_independent_of_own_column() {
        // -i targets an arbitrary column, decoupled from the action's own column.
        let mut args = default_args();
        args.action_split = true;
        args.selected_column = 1;
        args.insert_column = Some(0);
        args.insert_position = "before".to_string();
        let result = process_headers(headers(&["id", "ts", "val"]), &args).unwrap();
        assert_eq!(
            as_vec(&result),
            vec!["split_date>", "split_time>", "id", "ts", "val"]
        );
    }

    #[test]
    fn headers_rfc3339_all_positions() {
        for (mode, expected) in [
            ("before", vec!["id", "to_rfc3339>", "ts", "val"]),
            ("replace", vec!["id", "to_rfc3339", "val"]),
            ("after", vec!["id", "ts", "<to_rfc3339", "val"]),
        ] {
            let mut args = default_args();
            args.action_to_rfc3339 = true;
            args.selected_column = 1;
            args.insert_position = mode.to_string();
            let result = process_headers(headers(&["id", "ts", "val"]), &args).unwrap();
            assert_eq!(as_vec(&result), expected, "mode={mode}");
        }
    }

    #[test]
    fn headers_utc_all_positions() {
        for (mode, expected) in [
            ("before", vec!["id", "to_utc>", "ts", "val"]),
            ("replace", vec!["id", "to_utc", "val"]),
            ("after", vec!["id", "ts", "<to_utc", "val"]),
        ] {
            let mut args = default_args();
            args.action_to_utc = true;
            args.selected_column = 1;
            args.insert_position = mode.to_string();
            let result = process_headers(headers(&["id", "ts", "val"]), &args).unwrap();
            assert_eq!(as_vec(&result), expected, "mode={mode}");
        }
    }

    #[test]
    fn headers_local_all_positions() {
        for (mode, expected) in [
            ("before", vec!["id", "to_local>", "ts", "val"]),
            ("replace", vec!["id", "to_local", "val"]),
            ("after", vec!["id", "ts", "<to_local", "val"]),
        ] {
            let mut args = default_args();
            args.action_to_local = true;
            args.selected_column = 1;
            args.insert_position = mode.to_string();
            let result = process_headers(headers(&["id", "ts", "val"]), &args).unwrap();
            assert_eq!(as_vec(&result), expected, "mode={mode}");
        }
    }

    #[test]
    fn headers_offset_all_positions() {
        for (mode, expected) in [
            ("before", vec!["id", "to_offset>", "ts", "val"]),
            ("replace", vec!["id", "to_offset", "val"]),
            ("after", vec!["id", "ts", "<to_offset", "val"]),
        ] {
            let mut args = default_args();
            args.action_offset = true;
            args.selected_column = 1;
            args.target_offset = Some(FixedOffset::west_opt(6 * 3600).unwrap());
            args.insert_position = mode.to_string();
            let result = process_headers(headers(&["id", "ts", "val"]), &args).unwrap();
            assert_eq!(as_vec(&result), expected, "mode={mode}");
        }
    }

    #[test]
    fn headers_offset_out_of_bounds_is_a_clean_error() {
        let mut args = default_args();
        args.action_offset = true;
        args.selected_column = 9;
        args.target_offset = Some(FixedOffset::west_opt(6 * 3600).unwrap());
        let result = process_headers(headers(&["id", "ts", "val"]), &args);
        assert!(result.is_err());
    }

    #[test]
    fn headers_duration_single_column_all_positions() {
        for (mode, expected) in [
            ("before", vec!["id", "duration>", "ts", "val"]),
            ("replace", vec!["id", "duration", "val"]),
            ("after", vec!["id", "ts", "<duration", "val"]),
        ] {
            let mut args = default_args();
            args.action_duration = true;
            args.selected_column = 1;
            args.insert_position = mode.to_string();
            let result = process_headers(headers(&["id", "ts", "val"]), &args).unwrap();
            assert_eq!(as_vec(&result), expected, "mode={mode}");
        }
    }

    #[test]
    fn headers_duration_two_column_defaults_to_second_column() {
        let mut args = default_args();
        args.action_duration = true;
        args.selected_column = 1;
        args.second_column = Some(2);
        let result = process_headers(headers(&["id", "start", "end"]), &args).unwrap();
        assert_eq!(as_vec(&result), vec!["id", "start", "end", "<row_duration"]);
    }

    #[test]
    fn headers_duration_two_column_explicit_insert_position() {
        for (mode, expected) in [
            ("before", vec!["id", "start", "row_duration>", "end"]),
            ("replace", vec!["id", "start", "row_duration"]),
            ("after", vec!["id", "start", "end", "<row_duration"]),
        ] {
            let mut args = default_args();
            args.action_duration = true;
            args.selected_column = 1;
            args.second_column = Some(2);
            args.insert_column = Some(2);
            args.insert_position = mode.to_string();
            let result = process_headers(headers(&["id", "start", "end"]), &args).unwrap();
            assert_eq!(as_vec(&result), expected, "mode={mode}");
        }
    }

    #[test]
    fn headers_duration_two_column_out_of_bound_second_column_is_a_clean_error() {
        let mut args = default_args();
        args.action_duration = true;
        args.selected_column = 0;
        args.second_column = Some(9);
        let result = process_headers(headers(&["id", "start", "end"]), &args);
        assert!(result.is_err());
    }

    #[test]
    fn headers_remove() {
        let mut args = default_args();
        args.action_remove = true;
        args.remove_columns = vec![1];
        let result = process_headers(headers(&["id", "ts", "val"]), &args).unwrap();
        assert_eq!(as_vec(&result), vec!["id", "val"]);
    }

    #[test]
    fn headers_arrange_named_first_then_the_rest() {
        let mut args = default_args();
        args.action_arrange = true;
        args.arrange_columns = vec![3, 1];
        args.arrange_listed = vec![1, 3];
        let result =
            process_headers(headers(&["a", "b", "c", "d", "e"]), &args).unwrap();
        assert_eq!(as_vec(&result), vec!["d", "b", "a", "c", "e"]);
    }

    #[test]
    fn record_arrange_preserves_width_and_order() {
        let mut args = default_args();
        args.action_arrange = true;
        args.arrange_columns = vec![2, 0];
        args.arrange_listed = vec![0, 2];
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) =
            process_record(&record(&["a", "b", "c", "d"]), &args, cache).unwrap();
        assert_eq!(as_vec(&result), vec!["c", "a", "b", "d"]);
    }

    #[test]
    fn record_arrange_out_of_bound_is_a_clean_error() {
        let mut args = default_args();
        args.action_arrange = true;
        args.arrange_columns = vec![9];
        args.arrange_listed = vec![9];
        let cache: Option<DateTime<FixedOffset>> = None;
        assert!(process_record(&record(&["a", "b"]), &args, cache).is_err());
    }

    #[test]
    fn headers_remove_several_columns() {
        let mut args = default_args();
        args.action_remove = true;
        // As --remove's parsing produces it: highest index first.
        args.remove_columns = vec![3, 1];
        let result = process_headers(headers(&["id", "ts", "val", "note"]), &args).unwrap();
        assert_eq!(as_vec(&result), vec!["id", "val"]);
    }

    #[test]
    fn headers_remove_rejects_the_list_if_any_column_is_out_of_bound() {
        let mut args = default_args();
        args.action_remove = true;
        args.remove_columns = vec![9, 0];
        assert!(process_headers(headers(&["id", "ts"]), &args).is_err());
    }

    #[test]
    fn headers_remove_out_of_bounds_is_a_clean_error() {
        let mut args = default_args();
        args.action_remove = true;
        args.remove_columns = vec![9];
        let result = process_headers(headers(&["id", "ts", "val"]), &args);
        assert!(result.is_err());
    }

    // Regression test for a fixed crash bug: bounds-checking used to compute
    // `vecbuff.len() - 1`, underflowing (subtraction overflow panic) when the
    // record had zero fields, e.g. the header row on genuinely empty input.
    // Any column index must be a clean error against a zero-field record.
    #[test]
    fn headers_zero_field_record_is_a_clean_error_not_a_panic() {
        let mut args = default_args();
        args.action_remove = true;
        args.remove_columns = vec![0];
        let result = process_headers(headers(&[]), &args);
        assert!(result.is_err());
    }

    // ---- process_record: bounds checking ----------------------------------

    #[test]
    fn record_out_of_bound_column_is_a_clean_error() {
        let mut args = default_args();
        args.action_remove = true;
        args.remove_columns = vec![9];
        let cache: Option<DateTime<FixedOffset>> = None;
        let result = process_record(&record(&["1", "2", "3"]), &args, cache);
        assert!(result.is_err());
    }

    #[test]
    fn record_column_at_last_valid_index_is_allowed() {
        let mut args = default_args();
        args.action_remove = true;
        args.remove_columns = vec![2]; // last index of a 3-field record
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) = process_record(&record(&["1", "2", "3"]), &args, cache).unwrap();
        assert_eq!(as_vec(&result), vec!["1", "2"]);
    }

    #[test]
    fn record_zero_field_record_is_a_clean_error_not_a_panic() {
        let mut args = default_args();
        args.action_remove = true;
        args.remove_columns = vec![0];
        let cache: Option<DateTime<FixedOffset>> = None;
        let result = process_record(&record(&[]), &args, cache);
        assert!(result.is_err());
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
            chrono::DateTime::parse_from_rfc3339("2024-01-15T10:30:00+00:00").unwrap()
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
            // not. What comes back may well be Some -- the last five bytes of
            // "eeee-1234" are an ASCII sign and four digits whatever precedes
            // them -- and that is harmless, because what it builds still has
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

    // ---- process_record: split --------------------------------------------

    #[test]
    fn record_split_valid_timestamp_default_position() {
        let mut args = default_args();
        args.action_split = true;
        args.selected_column = 1;
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) = process_record(
            &record(&["1", "2024-01-15T10:30:00Z", "x"]),
            &args,
            cache,
        )
        .unwrap();
        assert_eq!(
            as_vec(&result),
            vec!["1", "2024-01-15T10:30:00Z", "2024-01-15", "10:30:00 +00:00", "x"]
        );
    }

    #[test]
    fn record_split_uses_the_inputs_own_offset_not_utcs_date() {
        // 00:30+05:00 and 19:30Z the day before are the same instant, but
        // split must report the date/time as given in the input's own
        // offset -- it doesn't convert to UTC first.
        let mut args = default_args();
        args.action_split = true;
        args.selected_column = 1;
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) = process_record(
            &record(&["1", "2024-01-01T00:30:00+05:00", "x"]),
            &args,
            cache,
        )
        .unwrap();
        assert_eq!(
            as_vec(&result),
            vec!["1", "2024-01-01", "00:30:00 +05:00", "x"]
        );
    }

    #[test]
    fn record_split_accepts_offset_missing_a_colon() {
        let mut args = default_args();
        args.action_split = true;
        args.selected_column = 1;
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) = process_record(
            &record(&["1", "2024-01-15T10:30:00+0000", "x"]),
            &args,
            cache,
        )
        .unwrap();
        assert_eq!(
            as_vec(&result),
            vec!["1", "2024-01-15T10:30:00+0000", "2024-01-15", "10:30:00 +00:00", "x"]
        );
    }

    #[test]
    fn record_split_invalid_timestamp_all_positions() {
        // A data row's parse-error marker is always the plain token, in every
        // position -- the `>`/`<` decorations are a header-only convention.
        for (mode, expected) in [
            (
                "before",
                vec!["1", "parse_err", "parse_err", "not-a-date", "x"],
            ),
            ("replace", vec!["1", "parse_err", "parse_err", "x"]),
            (
                "after",
                vec!["1", "not-a-date", "parse_err", "parse_err", "x"],
            ),
        ] {
            let mut args = default_args();
            args.action_split = true;
            args.selected_column = 1;
            args.insert_position = mode.to_string();
            let cache: Option<DateTime<FixedOffset>> = None;
            let (result, _) =
                process_record(&record(&["1", "not-a-date", "x"]), &args, cache).unwrap();
            assert_eq!(as_vec(&result), expected, "mode={mode}");
        }
    }

    // ---- process_record: to_rfc3339 ----------------------------------------

    #[test]
    fn record_to_rfc3339_valid_epoch() {
        let mut args = default_args();
        args.action_to_rfc3339 = true;
        args.selected_column = 1;
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) =
            process_record(&record(&["1", "1700000000", "x"]), &args, cache).unwrap();
        assert_eq!(
            as_vec(&result),
            vec!["1", "2023-11-14T22:13:20+00:00", "x"]
        );
    }

    #[test]
    fn record_to_rfc3339_non_numeric_is_parse_err() {
        let mut args = default_args();
        args.action_to_rfc3339 = true;
        args.selected_column = 1;
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) = process_record(&record(&["1", "abc", "x"]), &args, cache).unwrap();
        assert_eq!(as_vec(&result), vec!["1", "parse_err", "x"]);
    }

    #[test]
    fn record_to_rfc3339_unrepresentable_epoch_is_a_parse_err() {
        let mut args = default_args();
        args.action_to_rfc3339 = true;
        args.selected_column = 1;
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
        let huge = i64::MAX.to_string();
        let (result, _) =
            process_record(&record(&["1", &huge, "x"]), &args, cache).unwrap();
        assert_eq!(as_vec(&result), vec!["1", "parse_err", "x"]);
    }

    // ---- process_record: to_utc / to_local ---------------------------------

    #[test]
    fn record_to_utc_converts_offset() {
        let mut args = default_args();
        args.action_to_utc = true;
        args.selected_column = 1;
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) = process_record(
            &record(&["1", "2024-06-01T10:00:00+05:00", "x"]),
            &args,
            cache,
        )
        .unwrap();
        assert_eq!(
            as_vec(&result),
            vec!["1", "2024-06-01T05:00:00+00:00", "x"]
        );
    }

    #[test]
    fn record_to_utc_accepts_offset_missing_a_colon() {
        let mut args = default_args();
        args.action_to_utc = true;
        args.selected_column = 1;
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) = process_record(
            &record(&["1", "2024-06-01T10:00:00+0500", "x"]),
            &args,
            cache,
        )
        .unwrap();
        assert_eq!(
            as_vec(&result),
            vec!["1", "2024-06-01T05:00:00+00:00", "x"]
        );
    }

    #[test]
    fn record_to_utc_shifts_the_calendar_date_across_midnight() {
        // 01:00+05:00 is the previous UTC day -- a pure offset shift, no
        // timezone/DST logic involved, but the date-rollover arithmetic
        // still has to be right.
        let mut args = default_args();
        args.action_to_utc = true;
        args.selected_column = 1;
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) = process_record(
            &record(&["1", "2024-01-01T01:00:00+05:00", "x"]),
            &args,
            cache,
        )
        .unwrap();
        assert_eq!(
            as_vec(&result),
            vec!["1", "2023-12-31T20:00:00+00:00", "x"]
        );
    }

    #[test]
    fn record_to_utc_invalid_timestamp_is_parse_err() {
        let mut args = default_args();
        args.action_to_utc = true;
        args.selected_column = 1;
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) = process_record(&record(&["1", "nope", "x"]), &args, cache).unwrap();
        assert_eq!(as_vec(&result), vec!["1", "parse_err", "x"]);
    }

    #[test]
    fn record_to_local_produces_the_same_instant_as_utc() {
        let mut args = default_args();
        args.action_to_local = true;
        args.selected_column = 1;
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) = process_record(
            &record(&["1", "2024-06-01T10:00:00+05:00", "x"]),
            &args,
            cache,
        )
        .unwrap();
        let produced = as_vec(&result)[1].to_string();
        let parsed = chrono::DateTime::parse_from_rfc3339(&produced).unwrap();
        let original = chrono::DateTime::parse_from_rfc3339("2024-06-01T10:00:00+05:00").unwrap();
        assert_eq!(parsed, original, "local conversion must preserve the instant");
    }

    #[test]
    fn record_to_local_accepts_offset_missing_a_colon() {
        let mut args = default_args();
        args.action_to_local = true;
        args.selected_column = 1;
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) = process_record(
            &record(&["1", "2024-06-01T10:00:00+0500", "x"]),
            &args,
            cache,
        )
        .unwrap();
        let produced = as_vec(&result)[1].to_string();
        let parsed = chrono::DateTime::parse_from_rfc3339(&produced).unwrap();
        let original = chrono::DateTime::parse_from_rfc3339("2024-06-01T10:00:00+05:00").unwrap();
        assert_eq!(parsed, original, "local conversion must preserve the instant");
    }

    // ---- parse_offset -------------------------------------------------------

    #[test]
    fn parse_offset_accepts_colon_no_colon_and_z() {
        let texas = FixedOffset::west_opt(6 * 3600).unwrap();
        assert_eq!(parse_offset("-06:00").unwrap(), texas);
        assert_eq!(parse_offset("-0600").unwrap(), texas);
        assert_eq!(parse_offset("z").unwrap(), FixedOffset::east_opt(0).unwrap());
        assert_eq!(parse_offset("Z").unwrap(), FixedOffset::east_opt(0).unwrap());
        assert_eq!(
            parse_offset("+0200").unwrap(),
            FixedOffset::east_opt(2 * 3600).unwrap()
        );
    }

    #[test]
    fn parse_offset_rejects_garbage() {
        assert!(parse_offset("banana").is_err());
        assert!(parse_offset("").is_err());
    }

    // ---- format_iso8601_duration ---------------------------------------------

    #[test]
    fn format_iso8601_duration_omits_zero_components() {
        use chrono::TimeDelta;
        assert_eq!(format_iso8601_duration(TimeDelta::zero(), false), "PT0S");
        assert_eq!(format_iso8601_duration(TimeDelta::seconds(5), false), "PT5S");
        assert_eq!(
            format_iso8601_duration(TimeDelta::minutes(1) + TimeDelta::seconds(30), false),
            "PT1M30S"
        );
        assert_eq!(format_iso8601_duration(TimeDelta::hours(1), false), "PT1H");
        assert_eq!(
            format_iso8601_duration(TimeDelta::days(1), false),
            "P1D",
            "a whole day with no time component must omit the T entirely"
        );
        assert_eq!(
            format_iso8601_duration(
                TimeDelta::days(1) + TimeDelta::hours(6) + TimeDelta::minutes(30)
            , false),
            "P1DT6H30M"
        );
        assert_eq!(
            format_iso8601_duration(TimeDelta::days(3653), false),
            "P3653D",
            "large day counts stay a plain day component, not years/months"
        );
    }

    #[test]
    fn format_iso8601_duration_keeps_sub_second_precision() {
        use chrono::TimeDelta;
        assert_eq!(
            format_iso8601_duration(TimeDelta::milliseconds(999), false),
            "PT0.999S"
        );
        assert_eq!(
            format_iso8601_duration(TimeDelta::milliseconds(1500), false),
            "PT1.5S"
        );
        // Trailing zeros are trimmed, so half a second is ".5" not
        // ".500000000".
        assert_eq!(
            format_iso8601_duration(TimeDelta::nanoseconds(1), false),
            "PT0.000000001S"
        );
        assert_eq!(
            format_iso8601_duration(
                TimeDelta::days(1) + TimeDelta::hours(3) + TimeDelta::milliseconds(250),
                false
            ),
            "P1DT3H0.25S"
        );
    }

    #[test]
    fn format_iso8601_duration_round_seconds_is_half_up_and_carries() {
        use chrono::TimeDelta;
        assert_eq!(format_iso8601_duration(TimeDelta::milliseconds(400), true), "PT0S");
        assert_eq!(format_iso8601_duration(TimeDelta::milliseconds(500), true), "PT1S");
        assert_eq!(format_iso8601_duration(TimeDelta::milliseconds(999), true), "PT1S");
        // Rounding up must carry into the larger units, not produce "PT60S".
        assert_eq!(
            format_iso8601_duration(TimeDelta::milliseconds(59_600), true),
            "PT1M"
        );
        assert_eq!(
            format_iso8601_duration(
                TimeDelta::hours(23) + TimeDelta::minutes(59) + TimeDelta::milliseconds(59_800),
                true
            ),
            "P1D"
        );
    }

    #[test]
    fn format_iso8601_duration_whole_seconds_unaffected_by_rounding() {
        use chrono::TimeDelta;
        for round in [false, true] {
            assert_eq!(format_iso8601_duration(TimeDelta::zero(), round), "PT0S");
            assert_eq!(
                format_iso8601_duration(TimeDelta::days(1) + TimeDelta::hours(12), round),
                "P1DT12H"
            );
        }
    }

    // ---- process_record: offset ----------------------------------------------

    #[test]
    fn record_offset_converts_to_the_explicit_target() {
        let mut args = default_args();
        args.action_offset = true;
        args.selected_column = 1;
        args.target_offset = Some(FixedOffset::west_opt(6 * 3600).unwrap());
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) = process_record(
            &record(&["1", "2024-06-15T14:00:00+02:00", "x"]),
            &args,
            cache,
        )
        .unwrap();
        assert_eq!(
            as_vec(&result),
            vec!["1", "2024-06-15T06:00:00-06:00", "x"]
        );
    }

    #[test]
    fn record_offset_accepts_offset_missing_a_colon() {
        let mut args = default_args();
        args.action_offset = true;
        args.selected_column = 1;
        args.target_offset = Some(FixedOffset::west_opt(6 * 3600).unwrap());
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) = process_record(
            &record(&["1", "2024-06-15T14:00:00+0200", "x"]),
            &args,
            cache,
        )
        .unwrap();
        assert_eq!(
            as_vec(&result),
            vec!["1", "2024-06-15T06:00:00-06:00", "x"]
        );
    }

    #[test]
    fn record_offset_shifts_the_calendar_date_and_year_across_midnight() {
        // 00:30+02:00 on Jan 1 shifts back across both midnight and the
        // year boundary once re-expressed at -06:00.
        let mut args = default_args();
        args.action_offset = true;
        args.selected_column = 1;
        args.target_offset = Some(FixedOffset::west_opt(6 * 3600).unwrap());
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) = process_record(
            &record(&["1", "2024-01-01T00:30:00+02:00", "x"]),
            &args,
            cache,
        )
        .unwrap();
        assert_eq!(
            as_vec(&result),
            vec!["1", "2023-12-31T16:30:00-06:00", "x"]
        );
    }

    #[test]
    fn record_offset_invalid_timestamp_is_a_parse_err() {
        let mut args = default_args();
        args.action_offset = true;
        args.selected_column = 1;
        args.target_offset = Some(FixedOffset::west_opt(6 * 3600).unwrap());
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) = process_record(&record(&["1", "nope", "x"]), &args, cache).unwrap();
        assert_eq!(as_vec(&result), vec!["1", "parse_err", "x"]);
    }

    #[test]
    fn record_offset_insert_column_independent_of_own_column() {
        let mut args = default_args();
        args.action_offset = true;
        args.selected_column = 1;
        args.target_offset = Some(FixedOffset::west_opt(6 * 3600).unwrap());
        args.insert_column = Some(0);
        args.insert_position = "before".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) = process_record(
            &record(&["1", "2024-06-15T14:00:00+02:00", "x"]),
            &args,
            cache,
        )
        .unwrap();
        assert_eq!(
            as_vec(&result),
            vec!["2024-06-15T06:00:00-06:00", "1", "2024-06-15T14:00:00+02:00", "x"]
        );
    }

    // ---- process_record: duration (single column) --------------------------

    #[test]
    fn record_duration_first_row_is_zero_and_seeds_cache() {
        let mut args = default_args();
        args.action_duration = true;
        args.selected_column = 1;
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, new_cache) = process_record(
            &record(&["1", "2024-01-01T00:00:00Z"]),
            &args,
            cache,
        )
        .unwrap();
        assert_eq!(as_vec(&result), vec!["1", "PT0S"]);
        assert_eq!(
            new_cache,
            Some(chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap())
        );
    }

    #[test]
    fn record_duration_accepts_offset_missing_a_colon() {
        let mut args = default_args();
        args.action_duration = true;
        args.selected_column = 1;
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, new_cache) = process_record(
            &record(&["1", "2024-01-01T00:00:00+0000"]),
            &args,
            cache,
        )
        .unwrap();
        assert_eq!(as_vec(&result), vec!["1", "PT0S"]);
        assert_eq!(
            new_cache,
            Some(chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00+00:00").unwrap())
        );
    }

    #[test]
    fn record_duration_accumulates_row_over_row_even_when_unsorted() {
        let mut args = default_args();
        args.action_duration = true;
        args.selected_column = 1;
        args.insert_position = "replace".to_string();

        let mut cache: Option<DateTime<FixedOffset>> = None;

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
            Some(chrono::DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z").unwrap())
        );
    }

    #[test]
    fn record_duration_correct_across_a_dst_spring_forward() {
        // US clocks jump 02:00 -> 03:00 on 2024-03-10, so these timestamps'
        // wall-clock difference is 2h but the real elapsed time is 1h --
        // duration must reflect the instant, not naive wall-clock
        // subtraction, or this would come out as 0d2h0m0s instead.
        let mut args = default_args();
        args.action_duration = true;
        args.selected_column = 1;
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;

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
        args.action_duration = true;
        args.selected_column = 1;
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;

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
        args.action_duration = true;
        args.selected_column = 1;
        args.insert_position = "replace".to_string();
        let seed = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap();

        let (result, new_cache) =
            process_record(&record(&["1", "not-a-date"]), &args, Some(seed)).unwrap();
        assert_eq!(as_vec(&result), vec!["1", "parse_err"]);
        assert_eq!(new_cache, Some(seed), "cache must not advance on a parse error");
    }

    // ---- process_record: duration (two columns / same-row) -----------------

    #[test]
    fn record_duration_two_column_computes_absolute_difference_regardless_of_order() {
        let mut args = default_args();
        args.action_duration = true;
        args.selected_column = 0;
        args.second_column = Some(4);
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;

        // column 0 earlier than column 4; result replaces column 4 (the second column)
        let (result, _) = process_record(
            &record(&["2024-01-01T00:00:00Z", "a", "b", "c", "2024-01-02T12:00:00Z"]),
            &args,
            cache,
        )
        .unwrap();
        assert_eq!(as_vec(&result), vec!["2024-01-01T00:00:00Z", "a", "b", "c", "P1DT12H"]);

        // column 0 later than column 4 -- same absolute duration
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
        args.action_duration = true;
        args.selected_column = 0;
        args.second_column = Some(1);
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
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
        args.action_duration = true;
        args.selected_column = 0;
        args.second_column = Some(1);
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
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
        args.action_duration = true;
        args.selected_column = 0;
        args.second_column = Some(1);
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;

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
        // (+02:00) on the same real day -- each country's own DST-adjusted
        // offset, not a shared reference offset. 12:00 EDT = 16:00 UTC;
        // 20:00 CEST = 18:00 UTC; real difference is 2h.
        let mut args = default_args();
        args.action_duration = true;
        args.selected_column = 0;
        args.second_column = Some(1);
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
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
        args.action_duration = true;
        args.selected_column = 0;
        args.second_column = Some(1);
        args.insert_position = "replace".to_string();
        let seed = chrono::DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z").unwrap();
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
        args.action_duration = true;
        args.selected_column = 0;
        args.second_column = Some(9);
        let cache: Option<DateTime<FixedOffset>> = None;
        let result = process_record(&record(&["1", "2"]), &args, cache);
        assert!(result.is_err());
    }

    #[test]
    fn record_duration_insert_column_independent_of_action_columns() {
        // -i can target a column that's neither of --duration's own two columns.
        let mut args = default_args();
        args.action_duration = true;
        args.selected_column = 0;
        args.second_column = Some(2);
        args.insert_column = Some(1);
        args.insert_position = "replace".to_string();
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) = process_record(
            &record(&["2024-01-01T00:00:00Z", "untouched", "2024-01-02T12:00:00Z"]),
            &args,
            cache,
        )
        .unwrap();
        assert_eq!(
            as_vec(&result),
            vec!["2024-01-01T00:00:00Z", "P1DT12H", "2024-01-02T12:00:00Z"]
        );
    }

    // ---- process_record: remove ---------------------------------------------

    #[test]
    fn record_remove_several_columns_uses_input_positions() {
        // Highest index first, so dropping one cannot shift another still to
        // come: columns 0 and 1 of the input, not 0 and then whatever moved up.
        let mut args = default_args();
        args.action_remove = true;
        args.remove_columns = vec![1, 0];
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) =
            process_record(&record(&["a", "b", "c", "d"]), &args, cache).unwrap();
        assert_eq!(as_vec(&result), vec!["c", "d"]);
    }

    #[test]
    fn record_remove_drops_the_named_column() {
        let mut args = default_args();
        args.action_remove = true;
        args.remove_columns = vec![1];
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, _) = process_record(&record(&["1", "2", "3"]), &args, cache).unwrap();
        assert_eq!(as_vec(&result), vec!["1", "3"]);
    }

    #[test]
    fn record_no_action_is_unchanged() {
        let args = default_args();
        let cache: Option<DateTime<FixedOffset>> = None;
        let (result, new_cache) =
            process_record(&record(&["1", "2", "3"]), &args, cache).unwrap();
        assert_eq!(as_vec(&result), vec!["1", "2", "3"]);
        assert_eq!(new_cache, None);
    }

    // ---- is_rfc3339_year / is_rfc3339_offset / to_rfc3339_in_range ---------
    //
    // The two guards deciding whether a converted moment can be written at
    // all. Reached through -u/-l/-o above, but tested here directly as well:
    // what they answer at the boundary is the whole of their job, and going
    // through a conversion to ask makes the boundary harder to place.

    fn utc_at(year: i32, month: u32, day: u32, hour: u32) -> DateTime<chrono::Utc> {
        use chrono::TimeZone;
        chrono::Utc.with_ymd_and_hms(year, month, day, hour, 0, 0).unwrap()
    }

    #[test]
    fn rfc3339_year_spans_0001_to_9999_and_no_further() {
        assert!(is_rfc3339_year(&utc_at(2024, 6, 15, 12)));
        // Both ends of the range are inside it.
        assert!(is_rfc3339_year(&utc_at(1, 1, 1, 0)));
        assert!(is_rfc3339_year(&utc_at(9999, 12, 31, 23)));
        // And the first year on each side is not. The ceiling is where four
        // digits run out; the floor is one year short of that, because there
        // is no year zero for a record to have been written in.
        assert!(!is_rfc3339_year(&utc_at(0, 12, 31, 23)));
        assert!(!is_rfc3339_year(&utc_at(-1, 12, 31, 23)));
        assert!(!is_rfc3339_year(&utc_at(10000, 1, 1, 0)));
    }

    #[test]
    fn rfc3339_offset_is_whole_minutes_only() {
        use chrono::TimeZone;
        for seconds in [0, 60, 30 * 60, 5 * 3600 + 45 * 60, -12 * 3600, 14 * 3600] {
            let zone = FixedOffset::east_opt(seconds).unwrap();
            let moment = zone.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
            assert!(is_rfc3339_offset(&moment), "{seconds}s is whole minutes");
        }
        // Local mean time, which is what a zone used before standard time and
        // what RFC3339's +HH:MM has nowhere to put. Amsterdam's +00:17:30,
        // New York's -04:56:02, Monrovia's -00:44:30.
        for seconds in [17 * 60 + 30, -(4 * 3600 + 56 * 60 + 2), -(44 * 60 + 30)] {
            let zone = FixedOffset::east_opt(seconds).unwrap();
            let moment = zone.with_ymd_and_hms(1900, 6, 15, 12, 0, 0).unwrap();
            assert!(!is_rfc3339_offset(&moment), "{seconds}s is not whole minutes");
        }
    }

    #[test]
    fn to_rfc3339_in_range_marks_what_it_cannot_write() {
        assert_eq!(
            to_rfc3339_in_range(utc_at(2024, 6, 15, 12)),
            "2024-06-15T12:00:00+00:00"
        );
        // Outside the four-digit year, chrono would write ISO 8601's expanded
        // form -- a different format wearing the same shape.
        assert_eq!(to_rfc3339_in_range(utc_at(10000, 1, 1, 0)), PARSE_ERR);
    }

    #[test]
    fn to_rfc3339_in_range_refuses_a_sub_minute_offset_rather_than_moving_the_instant() {
        use chrono::TimeZone;
        // Africa/Monrovia's -00:44:30, which it kept until January 1972.
        // chrono renders this by rounding the offset to -00:45 while the clock
        // time keeps its seconds, so the result names an instant 30 seconds
        // from the one it was made from. That is the quiet wrong answer, and
        // it round-trips as proof: parsing what chrono writes gives back a
        // different moment.
        let monrovia = FixedOffset::west_opt(44 * 60 + 30).unwrap();
        let moment = monrovia.with_ymd_and_hms(1971, 6, 1, 11, 15, 30).unwrap();
        let rendered = moment.to_rfc3339();
        assert_ne!(
            chrono::DateTime::parse_from_rfc3339(&rendered).unwrap(),
            moment,
            "chrono's own rendering moves the instant; that is what is guarded"
        );
        assert_eq!(to_rfc3339_in_range(moment), PARSE_ERR);
    }

    // ---- row_wider_than_reference -------------------------------------------

    #[test]
    fn a_row_too_wide_is_explained_by_the_mode_that_hit_it() {
        // 'fix' takes its width from the first record, so a wider row later is
        // ordinary -- the documented limit of reading once.
        let fix = row_wider_than_reference(4, 2, FillMode::FirstRecord).to_string();
        assert!(fix.contains("--flexible=fix"), "said: {fix}");
        assert!(fix.contains("width of the first record"), "said: {fix}");

        // 'widest' takes its width from a pass over the whole input, so no row
        // can be wider than it. Getting here means the file is not the one
        // that was counted, and saying "the width of the first record" named
        // both a mode the user had not asked for and a width source that was
        // not theirs -- while the guarantee that had apparently failed went
        // unmentioned.
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
        // Unchecked, that fell through to the writer's bare record-length
        // error while a data row in the same state got the tailored one.
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
        let error = match process_stream(reader, &mut writer, &args, Some(2), None) {
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
        process_stream(reader, &mut writer, &args, Some(3), None).unwrap();
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
        // One past u32::MAX: on a 32-bit target the old `as usize` cast
        // truncated this to 0, so the insert landed before the first column
        // instead of clamping to the last. On the 64-bit hosts CI runs, both
        // codings agree -- this line pins the documented answer, and would
        // catch the wrap on any 32-bit runner that is ever added.
        assert_eq!(clamp_insert_column(4_294_967_296, 3), 2);
        // An empty record has no column to clamp to. The actions all refuse a
        // zero-length record before reaching here, so this is the defensive
        // answer rather than a reachable one -- but it must not underflow.
        assert_eq!(clamp_insert_column(5, 0), 0);
        assert_eq!(clamp_insert_column(-5, 0), 0);
    }

    #[test]
    fn insert_target_uses_the_actions_own_column_when_i_is_absent() {
        let mut args = default_args();
        assert_eq!(insert_target(&args, 1, 3), 1);
        // -i, once given, replaces that fallback and is clamped to the row.
        args.insert_column = Some(0);
        assert_eq!(insert_target(&args, 1, 3), 0);
        args.insert_column = Some(99);
        assert_eq!(insert_target(&args, 1, 3), 2);
    }

    // ---- insert_single / insert_pair ---------------------------------------

    #[test]
    fn insert_single_places_the_value_by_position() {
        for (position, expected) in [
            ("before", vec!["a", "new>", "b", "c"]),
            ("replace", vec!["a", "new", "c"]),
            ("after", vec!["a", "b", "<new", "c"]),
        ] {
            let mut fields = vec!["a", "b", "c"];
            insert_single(&mut fields, 1, position, "new>", "new", "<new");
            assert_eq!(fields, expected, "position={position}");
        }
        // "after" the last column appends rather than running past the end.
        let mut fields = vec!["a", "b", "c"];
        insert_single(&mut fields, 2, "after", "new>", "new", "<new");
        assert_eq!(fields, vec!["a", "b", "c", "<new"]);
    }

    #[test]
    fn insert_pair_keeps_its_two_values_in_order() {
        // The pair is written left-to-right whichever side of the column it
        // lands on, which the two separate inserts make easy to get backwards.
        for (position, expected) in [
            ("before", vec!["a", "one>", "two>", "b", "c"]),
            ("replace", vec!["a", "one", "two", "c"]),
            ("after", vec!["a", "b", "<one", "<two", "c"]),
        ] {
            let mut fields = vec!["a", "b", "c"];
            insert_pair(&mut fields, 1, position,
                ("one>", "two>"), ("one", "two"), ("<one", "<two"));
            assert_eq!(fields, expected, "position={position}");
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
        // which the bundled database cannot name and chrono handles itself.
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

    fn tally(converted: u64, failed: u64) -> Tally {
        Tally { converted, failed, ..Default::default() }
    }

    #[test]
    fn report_tally_is_quiet_when_nothing_failed() {
        assert!(report_tally(&tally(40, 0), None, ',').is_ok());
        // Nothing attempted at all -- a header-only file, or no action -- is
        // not a failure either.
        assert!(report_tally(&tally(0, 0), None, ',').is_ok());
    }

    #[test]
    fn report_tally_fails_only_when_nothing_converted() {
        // Some failures among many rows is ordinary: reported on stderr, but
        // still a successful run.
        assert!(report_tally(&tally(40122, 3), None, ',').is_ok());
        // Every value failing is not, and is the one case that is an error --
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

    fn commented_args(comment: Option<char>, style: &str) -> ArgumentFlags {
        ArgumentFlags {
            read_as_comment: comment,
            output_quotes: style.to_string(),
            ..default_args()
        }
    }

    #[test]
    fn record_written_counts_only_a_first_field_the_writer_leaves_bare() {
        // 'never' is now the style that leaves a leading comment character
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
        // there is nothing to count -- and the same for 'always'.
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

        // A header the writer quotes is not counted, so the flag stays clear
        // -- the flag follows the count rather than merely being the header.
        let mut safe = Tally::default();
        safe.record_written(&["#id", "ts"], &commented_args(Some('#'), "necessary"), true);
        assert_eq!(safe.commented_out, 0);
        assert!(!safe.commented_header);
    }

    fn delimited_args(delimiter: Option<char>, style: &str) -> ArgumentFlags {
        ArgumentFlags {
            output_delimiter: delimiter,
            output_quotes: style.to_string(),
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
        assert!(!writer_leaves_comment_bare("#x", "necessary"));
        assert!(!writer_leaves_comment_bare("#x", "always"));
        assert!(writer_leaves_comment_bare("#x", "never"));

        // --comment '-' over numeric data: -5 is a number, so 'nonnumeric'
        // writes it bare; -5x is not, so it is quoted and safe.
        assert!(writer_leaves_comment_bare("-5", "nonnumeric"));
        assert!(!writer_leaves_comment_bare("-5x", "nonnumeric"));
        assert!(!writer_leaves_comment_bare("#x", "nonnumeric"));

        // "Number" is csv-core's own is_non_numeric, asked directly, and its
        // reading is a float parse -- so scientific notation and even the
        // infinities count as numbers and are written bare. Pinned since the
        // answer now comes from the writer's crate rather than a mirror, and
        // these are the cases where a hand-rolled "is it a number" would have
        // guessed differently.
        assert!(writer_leaves_comment_bare("-1e5", "nonnumeric"));
        assert!(writer_leaves_comment_bare("-inf", "nonnumeric"));
        assert!(writer_leaves_comment_bare("-5.", "nonnumeric"));
    }
}
