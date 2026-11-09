// The text passes that turn clap_mangen's rendering into the manual page
// csvdt ships, and the verbatim gutter they read. Each pass takes the roff
// as bytes and returns it; `arrange_page` runs them in the order they need.

// Runs every pass over the rendered page, in the order they depend on.
//
// `trailing_headings` are the section titles the help ends with, and
// `supplement` is the hand-written roff spliced in before VERSION.
pub(crate) fn arrange_page(rendered: &[u8], trailing_headings: &[&str], supplement: &str) -> Vec<u8> {
    let rendered = promote_man_headings(rendered, trailing_headings);
    // After the sections above it, so a line that is one of those is
    // already .SH and cannot be taken for a subsection of itself, and
    // before the pass below, so a heading is never inside a held block.
    let rendered = promote_man_subheadings(&rendered);
    let rendered = break_after_option_tags(&rendered);
    let rendered = preserve_aligned_blocks(&rendered);
    let rendered = insert_man_sections(&rendered, supplement);
    // Last, so the two passes above still match on the lines clap_mangen
    // wrote, and so the supplement's own option names are protected too.
    protect_option_names(&rendered)
}

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
// the page is unchanged, which is why this needs no test of length, and
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
            // is not a tag to break after, a .TP followed straight by
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
// Most of the headings under KNOWN LIMITS would come out that way, leaving
// that section one long paragraph with capitals scattered through it.
//
// .SS is what the format has for this, and it settles the spacing as well as
// the break: the blank line above is dropped when there is one, since .SS
// brings its own and two together read as a gap rather than a heading.
//
// A heading is a line at the margin whose words are all capitals once the
// option names among them are set aside, 'parse_err' too, spelled as the
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
// which is what keeps 'KNOWN LIMITS, at the end of this help', prose, mid
// sentence, three times over, from becoming a section heading.
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
// line count, the measure and every other break are unchanged, and groff
// stays silent. The alternative, '.nh' over the whole page,
// would also stop hyphenating the prose, which has no such problem and
// justifies better for being allowed to break.
//
// Option names reach the rendered roff with each hyphen escaped, as
// '\-\-duration', so a word beginning '\-' is the thing to protect. Two
// places hold them. Most are in running text, where the name begins a word.
// The rest are the whole argument of a one-line font macro, '.B
// \-\-flexible=widest' in FILES being the one the corpus has, and a macro
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
    // does, so the two are the same problem and get the same answer. A macro
    // argument that is only partly a name, such as '.I the destination given
    // to \-\-fill\-log', is protected too, or it could be broken at the
    // hyphen.
    fn mark(span: &str, out: &mut String) {
        // Word-opening is the whole test: a hyphen inside a word is a real
        // hyphen, 'two-column', 'csvdt-widest-', and breaking there is
        // what a reader expects, so it is left alone.
        let mut rest = span;
        let mut at_start = true;
        while let Some(index) = rest.find("\\-") {
            let (before, from) = rest.split_at(index);
            out.push_str(before);
            // Only a hyphen that opens a word. '\%' in the middle of one is
            // not inert: it is an extra place roff may break, so getting
            // this wrong does the opposite of what the pass is for. The
            // example rows in EXAMPLES would break at their dates, since
            // '2023\-11\-14' is a word whose second and third hyphens are
            // internal.
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
// A line opening with it is text laid out on purpose, a command to be
// copied, a sample of output, a diagram whose columns line up, and is
// written to the page exactly as it stands. Everything else is prose, and
// prose is roff's to fill to the width it is read at.
//
// It is a mark in the corpus rather than a guess about the corpus. Deciding
// by shape, for instance holding any block that carries an indent, would
// freeze nested prose along with the listings. Telling the two apart needs a
// mark, because nothing in the shape of a line does it: the two lines of
// csvdt's own stderr quoted under KNOWN LIMITS are eight words each and
// wrapped like a sentence, and a rule that reads them as one joins a message
// the reader is meant to copy.
//
// The gutter never reaches a reader. It is stripped from every string clap
// is given, so --help and -h carry no trace of it, and stripped again here
// on the way into the page.
pub(crate) const VERBATIM: char = '|';

// Removes the verbatim gutter, leaving the line exactly as it was written.
pub(crate) fn strip_marks(text: &str) -> String {
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
pub(crate) fn without_marks(command: clap::Command) -> clap::Command {
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
//   marked with the VERBATIM gutter, written as it stands, with a break on
//   every line, and the gutter taken off;
//
//   indented and unmarked, a nested paragraph. The indent is handed to
//   roff as '.RS' and the words fill inside it, which is how a page nests
//   prose and how these read at any width rather than at the help file's;
//
//   at the margin and unmarked, ordinary prose, left alone to fill.
//
// The cost of holding a line is that a break is all it gets: filling stays
// on, so a line too long for the terminal is still wrapped by roff, and
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
    // first. A single space is not one, so 'headers Trim whitespace', the
    // widest name in --trim's table, one space from its description, does
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
            // has, and it only ever holds a block, the direction that
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
                // would hold every line exactly as written, but then roff may
                // not wrap a line that is too long for the terminal, and the
                // pager breaks it badly. The help is written to sit under
                // --help's indent and man adds 14 more, so whole blocks would
                // overflow 80 columns. With .br the author's line breaks are
                // kept and roff may still wrap a line that does not fit.
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


// Splices the hand-written sections into the rendered page.
//
// Everything above them is generated from the Command that parses a command
// line, which is what keeps the page from describing options this build does
// not have. It is also why there is nowhere in it to say what belongs to the
// program rather than to one of its options, the environment read, the file
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

#[cfg(test)]
mod tests {
    // A run of two lines with a column gap is a table even with nothing
    // indented, as --trim and --list-options are. Written as a unit test
    // because the corpus cannot ask it: every table there has three such
    // lines, so raising the threshold to three would change nothing in the
    // manual page and go unnoticed until the next two-row table was written.
    //
    // This is the only place that asks at all. Every table in the corpus is
    // laid out as a tag on its own line with its description indented under
    // it, so all of them are indented and the indent alone answers for them.
    // The signal stays live for the next table written flush against the
    // margin.
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

    // The three treatments in one block: prose at the margin fills, prose
    // indented under it fills inside an indent of its own, and a marked
    // block is written out as it stands.
    //
    // An indent alone must not hold a block: the middle one would freeze at
    // the width the help file is wrapped to while the paragraph above it ran
    // to the terminal's. Read at 100 columns, DESCRIPTION fills to 100 and
    // 'INPUT MUST BE UTF-8' three lines below it must too.
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
    // one that sits inside it, which is the whole of what is tested here.
    #[test]
    fn option_names_are_protected_and_dates_are_left_alone() {
        let protect = |text: &[u8]| {
            String::from_utf8(super::protect_option_names(text)).unwrap()
        };

        // A name in running text, and the same name after an opening
        // bracket, the second is mid-word by the only test that matters
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

        // A name later in a font macro's argument. Marking an argument only
        // when the whole of it is a name would leave this unprotected.
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
}
