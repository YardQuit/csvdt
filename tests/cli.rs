use assert_cmd::Command;
use predicates::prelude::*;

fn csvdt() -> Command {
    Command::cargo_bin("csvdt").unwrap()
}

// --help with its whitespace collapsed.
//
// What the help promises is that it says a thing, not that it says it on one
// line: the corpus is wrapped by hand, so a phrase sits across a line break
// whenever the wrapping moves. Two tests learned that the hard way and
// collapsed the whitespace themselves; a third and fourth did not, and failed
// on a rewrap with the sentence still present and still saying the same
// thing -- reporting on formatting while claiming to report on content.
fn help_words() -> String {
    let help = csvdt().arg("--help").output().unwrap();
    let help = String::from_utf8(help.stdout).expect("--help should be UTF-8");
    help.split_whitespace().collect::<Vec<_>>().join(" ")
}

// The output of one help flag.
fn help_output(flag: &str) -> String {
    let help = csvdt().arg(flag).output().unwrap();
    String::from_utf8(help.stdout).expect("help should be UTF-8")
}

// Every option in a help listing, as long name to description, with the
// description put back on one line.
//
// An entry begins at the left, near enough: '-H, --has-header' sits at two
// columns and '--round-seconds', having no short form, at six. Everything
// clap wraps into the description column is indented far past that, which
// is what tells a continuation line from the next entry -- including the
// value tables in the long form, whose rows begin with an option name of
// their own and would otherwise read as entries.
fn option_entries(text: &str) -> std::collections::BTreeMap<String, String> {
    let mut entries = std::collections::BTreeMap::new();
    let mut current = String::new();
    let mut inside = false;
    for line in text.lines() {
        if line.trim_end() == "Options:" {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        // Text at the left margin closes the section: every line inside it
        // is indented, and what follows is after-help. Not a blank line --
        // the long form puts one between every pair of entries, having given
        // each description a line of its own.
        if indent == 0 {
            inside = false;
            continue;
        }
        if indent <= 6 && trimmed.starts_with('-') {
            let long = trimmed
                .split_whitespace()
                .find_map(|word| word.strip_prefix("--"))
                // '--flexible[=<keep|fix|widest>]' is the name and then the
                // value it takes, which is not part of the name.
                .map(|word| {
                    word.chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                        .collect::<String>()
                })
                .expect("every entry should carry a long name");
            // The description starts after the run of spaces separating it
            // from the option spec.
            let description = match trimmed.find("  ") {
                Some(at) => trimmed[at..].trim_start().to_string(),
                None => String::new(),
            };
            current = long.clone();
            entries.insert(long, description);
        } else if !current.is_empty() {
            let entry = entries.get_mut(&current).unwrap();
            if !entry.is_empty() {
                entry.push(' ');
            }
            entry.push_str(trimmed);
        }
    }
    entries
}

// A description without the notes clap appends, which it appends to both
// forms alike and so cannot tell them apart. Only those two: a column
// reference like '[0...]' is the help's own prose and stays.
fn without_clap_notes(description: &str) -> String {
    let mut text = description.trim_end();
    while text.ends_with(']') {
        let Some(at) = text.rfind('[') else { break };
        let note = &text[at + 1..text.len() - 1];
        if !note.starts_with("default:") && !note.starts_with("possible values:")
        {
            break;
        }
        text = text[..at].trim_end();
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn help_explains_the_timestamp_formats_in_use() {
    csvdt()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("RFC3339"))
        .stdout(predicate::str::contains("Unix epoch"))
        .stdout(predicate::str::contains("ISO 8601 duration"));
}

#[test]
fn help_explains_why_duration_never_uses_years_or_months() {
    // The top-level command description (not just -d's own help) should
    // tell investigators/users up front why duration results are exact
    // days/hours/minutes/seconds and never an approximated year/month.
    csvdt()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("never approximates a duration"));
}

// ---- basic passthrough / CSV plumbing -------------------------------------

#[test]
fn passthrough_with_no_flags() {
    csvdt()
        .write_stdin("a,b,c\n1,2,3\n")
        .assert()
        .success()
        .stdout("a,b,c\n1,2,3\n");
}

#[test]
fn has_header_without_print_header_drops_header_row() {
    csvdt()
        .arg("-H")
        .write_stdin("id,ts,val\n1,2,3\n")
        .assert()
        .success()
        .stdout("1,2,3\n");
}

#[test]
fn has_header_with_print_header_keeps_header_row() {
    csvdt()
        .args(["-H", "-p"])
        .write_stdin("id,ts,val\n1,2,3\n")
        .assert()
        .success()
        .stdout("id,ts,val\n1,2,3\n");
}

#[test]
fn print_header_without_has_header_is_rejected() {
    csvdt()
        .arg("-p")
        .write_stdin("a,b\n1,2\n")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("--has-header"));
}

#[test]
fn reads_from_a_file_argument_instead_of_stdin() {
    let dir = tempfile_dir();
    let path = dir.join("in.csv");
    std::fs::write(&path, "a,b\n1,2\n").unwrap();

    csvdt()
        .arg(&path)
        .assert()
        .success()
        .stdout("a,b\n1,2\n");
}

fn tempfile_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("csvdt-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// Point a run's temporary directory at DIR, whichever variable this platform
// reads for it.
//
// std::env::temp_dir asks TMPDIR on Unix and TMP then TEMP on Windows, so
// setting only TMPDIR moves nothing on Windows: the run spools to the real
// system directory, and a test asserting its own directory stayed empty
// passes without having tested anything. Two did, on the one platform where
// the spool has never been watched.
fn with_temp_dir(
    command: assert_cmd::Command,
    dir: &std::path::Path,
) -> assert_cmd::Command {
    let mut command = command;
    command.env("TMPDIR", dir).env("TMP", dir).env("TEMP", dir);
    command
}

// ---- column actions: split / rfc3339 / utc / local / duration / remove ---
// Every action now takes its column number as its own argument (e.g. -s1),
// glued or space-separated. -i is independent: "-i <col> <mode>".

#[test]
fn split_default_position_is_after_own_column() {
    csvdt()
        .args(["-H", "-p", "-s1"])
        .write_stdin("id,ts,val\n1,2024-01-15T10:30:00Z,10\n")
        .assert()
        .success()
        .stdout(
            "id,ts,<split_date,<split_time,val\n\
             1,2024-01-15T10:30:00Z,2024-01-15,10:30:00 +00:00,10\n",
        );
}

#[test]
fn split_invalid_timestamp_yields_parse_err_markers() {
    csvdt()
        .args(["-H", "-p", "-s1", "-i", "1,replace"])
        .write_stdin("id,ts,val\n1,not-a-date,10\n2,2024-01-15T10:30:00Z,20\n")
        .assert()
        .success()
        .stdout(
            "id,split_date,split_time,val\n\
             1,parse_err,parse_err,10\n\
             2,2024-01-15,10:30:00 +00:00,20\n",
        );
}

#[test]
fn every_conversion_marks_what_it_cannot_parse_the_same_way_and_still_exits_0() {
    // Not aborting is deliberate: one malformed line should not throw away a
    // run over a large log. The cost is that the status cannot be used to tell
    // whether anything failed -- a file whose timestamps are not the assumed
    // format converts "successfully" into a file of markers -- so the marker is
    // the only signal there is, and every action has to use the same one for
    // `grep -c parse_err` to be the answer. KNOWN LIMITS says exactly this.
    for (action, convertible) in [
        ("-r0", "1700000000"),
        ("-u0", "2024-01-01T00:00:00Z"),
        ("-l0", "2024-01-01T00:00:00Z"),
        ("-s0", "2024-01-01T00:00:00Z"),
        ("-o0,+01:00", "2024-01-01T00:00:00Z"),
        ("-d0", "2024-01-01T00:00:00Z"),
    ] {
        csvdt()
            .args(["-H", "-p", action])
            .write_stdin(format!("ts\n{convertible}\nJan  1 00:00:00\n"))
            .assert()
            .success()
            .code(0)
            .stdout(predicate::str::contains("parse_err"))
            // Reported on stderr rather than in the output, and it does not
            // make the run a failure: 1 of 2 converted.
            .stderr(predicate::str::contains("1 of 2 values"));
    }
}

#[test]
fn a_run_where_nothing_converted_is_a_failure_and_a_run_that_partly_did_is_not() {
    // The two cases the old exit 0 could not tell apart. A few failures among
    // many rows is ordinary -- one malformed line must not throw away a large
    // log -- but nothing converting at all means the column given is not the
    // one holding the timestamps, or they are not in a format csvdt reads. That
    // exited 0 too, which is indistinguishable from success: a script would
    // carry on and analyse a file of markers.
    csvdt()
        .args(["-H", "-p", "-u0"])
        .write_stdin("ts\nJan  1 00:00:00\nJan  1 00:00:05\n")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("nothing converted"))
        // The output is still written; the status is what carries the news.
        .stdout("ts,<to_utc\nJan  1 00:00:00,parse_err\nJan  1 00:00:05,parse_err\n");

    // One good row is enough for the run to have done its job.
    csvdt()
        .args(["-H", "-p", "-u0"])
        .write_stdin("ts\nJan  1 00:00:00\n2024-01-01T00:00:00Z\n")
        .assert()
        .success()
        .code(0)
        .stderr(predicate::str::contains("1 of 2 values could not be converted"));

    // Nothing to say when everything converted.
    csvdt()
        .args(["-H", "-p", "-u0"])
        .write_stdin("ts\n2024-01-01T00:00:00Z\n")
        .assert()
        .success()
        .stderr("");

    // Nor when nothing was asked to convert: --remove and -a read no value, so
    // there is nothing that could have failed.
    for arguments in [vec!["-H", "-p", "--remove", "1"], vec!["-H", "-p", "-a1"]] {
        csvdt()
            .args(&arguments)
            .write_stdin("a,b\nJan  1 00:00:00,x\n")
            .assert()
            .success()
            .stderr("");
    }

    // And an empty input converted nothing because there was nothing there,
    // which is not the same as having failed at it.
    csvdt()
        .args(["-H", "-p", "-u0"])
        .write_stdin("")
        .assert()
        .success()
        .stderr("");
}

#[test]
fn a_value_that_will_not_parse_is_still_there_to_be_read_unless_replaced() {
    // The marker is a word rather than an empty field because an empty field is
    // invisible. For the same reason the value that failed should stay in the
    // output next to it -- otherwise the marker says something went wrong
    // without saying what. -i replace is the documented exception.
    csvdt()
        .args(["-H", "-p", "-u0"])
        .write_stdin("ts,msg\nJan  1 00:00:00,syslog\n2024-01-01T00:00:00Z,ok\n")
        .assert()
        .success()
        .stdout(
            "ts,<to_utc,msg\n\
             Jan  1 00:00:00,parse_err,syslog\n\
             2024-01-01T00:00:00Z,2024-01-01T00:00:00+00:00,ok\n",
        );

    csvdt()
        .args(["-H", "-p", "-u0", "-i0,replace"])
        .write_stdin("ts,msg\nJan  1 00:00:00,syslog\n2024-01-01T00:00:00Z,ok\n")
        .assert()
        .success()
        .stdout("to_utc,msg\nparse_err,syslog\n2024-01-01T00:00:00+00:00,ok\n");
}

#[test]
fn help_says_what_parse_err_means_and_that_it_does_not_fail_the_run() {
    // The marker is the only report of a failed conversion there is, so it has
    // to be findable from --help. It went undocumented for a long time.
    csvdt()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("parse_err"))
        .stdout(predicate::str::contains("exits 0"));
}

#[test]
fn help_says_the_writer_knows_the_comment_character() {
    // The protection lives in the writer, so someone deciding whether the
    // output of a --comment run is safe to re-read has to be able to find the
    // answer -- and the two styles it cannot protect under -- from --help.
    //
    // Read with the whitespace collapsed, because what is promised is that
    // --help says these things, not that it says them on one line. Asserting
    // the line breaks made this fail when a help file was rewrapped, with the
    // sentence still present and still saying the same thing -- a test that
    // reports on formatting while claiming to report on content.
    let help = help_words();

    for promised in [
        "The writer knows this character too",
        "or the '--comment' character, when one is in use",
    ] {
        assert!(
            help.contains(promised),
            "--help no longer says {promised:?}",
        );
    }
}

#[test]
fn a_record_length_error_says_what_else_to_look_for() {
    // There is no error for a quote that is opened and never closed: the reader
    // takes everything after it as one field, to the end of the file, so it
    // arrives as a record with too few fields. That is also what a genuinely
    // ragged file gives -- and the answer to raggedness, --flexible, accepts the
    // mangled record rather than reporting it. So the message that misdirects
    // has to say what else to look for.
    for input in [
        "a,b\n\"x,2\n3,4\n", // an unclosed quote
        "a,b\n1\n",          // genuinely ragged
    ] {
        csvdt()
            .args(["-H", "-p"])
            .write_stdin(input)
            .assert()
            .failure()
            .code(1)
            .stderr(predicate::str::contains("found record with 1 fields"))
            .stderr(predicate::str::contains("opened and never closed"));
    }
}

#[test]
fn an_unclosed_quote_swallows_the_rest_of_the_file_under_flexible() {
    // The half that makes the hint above worth having. Three rows come out as
    // one field: valid CSV, nothing like the input, exit 0. Pinned because it is
    // a limit rather than a bug -- a field genuinely holding newlines is a real
    // thing, and once parsed the two are indistinguishable -- so it must not
    // quietly change without KNOWN LIMITS changing with it.
    csvdt()
        .args(["-H", "-p", "-f"])
        .write_stdin("a,b\n\"x,2\n3,4\n5,6\n")
        .assert()
        .success()
        // The field ends with the newline it really holds: nothing trims it out
        // of the data any more.
        .stdout("a,b\n\"x,2\n3,4\n5,6\n\"\n");

    // A field that really does hold a newline is the same shape, which is why
    // this cannot be refused: here the quote is closed, and the row is right.
    csvdt()
        .args(["-H", "-p"])
        .write_stdin("a,b\n\"x\ny\",2\n")
        .assert()
        .success()
        .stdout("a,b\n\"x\ny\",2\n");
}

#[test]
fn a_conversion_that_leaves_rfc3339s_year_range_is_marked_not_written() {
    // RFC3339's date-fullyear is four digits. chrono reaches much further and
    // falls back to ISO 8601's expanded form -- +10000-01-01, -0001-12-31 --
    // which is a different format wearing the same shape, cannot be read back,
    // and gave no sign of the boundary it had crossed. The guard for this
    // existed but was wired into -r alone, so -u, -o and -l wrote it out.
    //
    // Note none of these move the instant far: a conversion can cross the
    // boundary purely by changing the clock it is read on.
    for (arguments, input, convertible) in [
        // The input's own offset carries the UTC year over.
        (vec!["-H", "-u0"], "9999-12-31T23:59:59-12:00", "2024-01-01T00:00:00Z"),
        (vec!["-H", "-u0"], "0001-01-01T00:00:00+14:00", "2024-01-01T00:00:00Z"),
        // The target offset does it, while the UTC year stays in range.
        (vec!["-H", "-o0,+14:00"], "9999-12-31T23:59:59+00:00", "2024-01-01T00:00:00Z"),
        (vec!["-H", "-o0,-12:00"], "0001-01-01T00:00:00+00:00", "2024-01-01T00:00:00Z"),
        // And the epoch path, which is where the guard already was.
        (vec!["-H", "-r0"], "253402300800", "0"),
    ] {
        csvdt()
            .args(&arguments)
            .write_stdin(format!("ts\n{convertible}\n{input}\n"))
            .assert()
            .success()
            .stdout(predicate::str::contains("parse_err"))
            .stdout(predicate::str::contains("+10000").not())
            .stdout(predicate::str::contains("-0001").not());
    }

    // A named zone reaches it the same way.
    csvdt()
        .args(["-H", "-l0"])
        .env("TZ", "Asia/Tokyo")
        .write_stdin("ts\n2024-01-01T00:00:00Z\n9999-12-31T23:59:59+00:00\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("parse_err"));

    // The edges themselves are inside the format and convert normally.
    for (arguments, input, expected) in [
        (vec!["-H", "-u0"], "9999-12-31T23:59:59+00:00", "9999-12-31T23:59:59+00:00"),
        (vec!["-H", "-u0"], "0001-01-01T00:00:00+00:00", "0001-01-01T00:00:00+00:00"),
        (vec!["-H", "-o0,+14:00"], "2024-06-15T14:00:00+02:00", "2024-06-16T02:00:00+14:00"),
    ] {
        csvdt()
            .args(&arguments)
            .write_stdin(format!("ts\n{input}\n"))
            .assert()
            .success()
            .stdout(format!("{input},{expected}\n"));
    }
}

#[test]
fn split_keeps_sub_second_precision_like_every_other_conversion() {
    // The time column was formatted with a hard-coded %H:%M:%S, so a fraction
    // the source carried was dropped without a word. Nothing else here does
    // that: -d keeps it unless --round-seconds asks otherwise, and -u/-l/-o
    // write back what came in. The rendering matches theirs -- chrono's 3, 6 or
    // 9 digits -- so a split column and a -u column agree about the same
    // instant.
    for (input, expected) in [
        ("2024-01-15T10:30:00Z", "10:30:00 +00:00"),
        ("2024-01-15T10:30:00.1Z", "10:30:00.100 +00:00"),
        ("2024-01-15T10:30:00.123Z", "10:30:00.123 +00:00"),
        ("2024-01-15T10:30:00.123456789Z", "10:30:00.123456789 +00:00"),
        ("2024-06-15T14:00:00.5+02:00", "14:00:00.500 +02:00"),
    ] {
        csvdt()
            .args(["-H", "-p", "-s0", "-i0,replace"])
            .write_stdin(format!("ts\n{input}\n"))
            .assert()
            .success()
            .stdout(format!(
                "split_date,split_time\n{},{}\n",
                &input[..10], expected,
            ));
    }
}

#[test]
fn rfc3339_replace_converts_epoch_seconds() {
    csvdt()
        .args(["-H", "-p", "-r1", "-i", "1,replace"])
        .write_stdin("id,ts,val\n1,1700000000,10\n")
        .assert()
        .success()
        .stdout("id,to_rfc3339,val\n1,2023-11-14T22:13:20+00:00,10\n");
}

#[test]
fn rfc3339_non_numeric_field_is_a_parse_error_marker() {
    csvdt()
        .args(["-H", "-p", "-r1", "-i", "1,replace"])
        .write_stdin("id,ts,val\n1,not-a-number,10\n2,1700000000,20\n")
        .assert()
        .success()
        .stdout(
            "id,to_rfc3339,val\n\
             1,parse_err,10\n\
             2,2023-11-14T22:13:20+00:00,20\n",
        );
}

#[test]
fn rfc3339_epoch_outside_chrono_range_is_a_parse_error_marker() {
    // Regression test for a fixed bug: an i64 that parses fine but is out of
    // chrono's representable timestamp range (e.g. i64::MAX) used to make
    // `from_timestamp` return None and silently pass the row through
    // unmodified, with no value written and no error marker. It must now be
    // treated the same as any other unparseable timestamp.
    csvdt()
        .args(["-H", "-p", "-r1", "-i", "1,replace"])
        .write_stdin(format!("id,ts,val\n1,{},10\n2,0,20\n", i64::MAX))
        .assert()
        .success()
        .stdout(
            "id,to_rfc3339,val\n\
             1,parse_err,10\n\
             2,1970-01-01T00:00:00+00:00,20\n",
        );
}

#[test]
fn utc_converts_offset_timestamp() {
    csvdt()
        .args(["-H", "-p", "-u1", "-i", "1,replace"])
        .write_stdin("id,ts\n1,2024-06-01T10:00:00+05:00\n")
        .assert()
        .success()
        .stdout("id,to_utc\n1,2024-06-01T05:00:00+00:00\n");
}

#[test]
fn utc_shifts_the_calendar_date_across_midnight() {
    csvdt()
        .args(["-H", "-p", "-u1", "-i", "1,replace"])
        .write_stdin("id,ts\n1,2024-01-01T01:00:00+05:00\n")
        .assert()
        .success()
        .stdout("id,to_utc\n1,2023-12-31T20:00:00+00:00\n");
}

#[test]
fn utc_accepts_an_offset_missing_its_colon() {
    // Some tools export RFC3339-ish timestamps with the offset colon
    // omitted (+0500 instead of +05:00). csvdt normalizes that before
    // parsing rather than treating it as a parse error.
    csvdt()
        .args(["-H", "-p", "-u1", "-i", "1,replace"])
        .write_stdin("id,ts\n1,2024-06-01T10:00:00+0500\n")
        .assert()
        .success()
        .stdout("id,to_utc\n1,2024-06-01T05:00:00+00:00\n");
}

#[test]
fn local_converts_to_the_process_timezone() {
    // Pin TZ so the assertion is deterministic regardless of host timezone.
    csvdt()
        .args(["-H", "-p", "-l1", "-i", "1,replace"])
        .env("TZ", "UTC")
        .write_stdin("id,ts\n1,2024-06-01T10:00:00+05:00\n")
        .assert()
        .success()
        .stdout("id,to_local\n1,2024-06-01T05:00:00+00:00\n");
}

#[test]
fn local_reflects_a_real_dst_transition_via_system_tzdata() {
    // Unlike -u/-o (pure fixed-offset arithmetic), -l goes through the
    // system's real timezone database, so the same wall-clock instant
    // resolves to a different offset depending on whether the target
    // timezone is observing DST on that date. Requires the test host to
    // have IANA tzdata for America/Chicago (present on essentially every
    // Linux distro's base system).
    csvdt()
        .args(["-H", "-p", "-l1", "-i", "1,replace"])
        .env("TZ", "America/Chicago")
        .write_stdin(
            "id,ts\n\
             1,2024-01-15T12:00:00Z\n\
             2,2024-07-15T12:00:00Z\n",
        )
        .assert()
        .success()
        .stdout(
            "id,to_local\n\
             1,2024-01-15T06:00:00-06:00\n\
             2,2024-07-15T07:00:00-05:00\n",
        );
}

#[test]
fn offset_converts_to_an_explicit_target_regardless_of_host_timezone() {
    // Unlike -l, -o doesn't depend on the machine's configured timezone at
    // all -- no TZ env needed here to get a deterministic result.
    csvdt()
        .args(["-H", "-p", "-o", "1,-06:00", "-i", "1,replace"])
        .write_stdin("id,ts\n1,2024-06-15T14:00:00+02:00\n")
        .assert()
        .success()
        .stdout("id,to_offset\n1,2024-06-15T06:00:00-06:00\n");
}

#[test]
fn offset_accepts_lenient_input_and_target_syntax() {
    csvdt()
        .args(["-H", "-p", "-o", "1,-0600", "-i", "1,replace"])
        .write_stdin("id,ts\n1,2024-06-15T14:00:00+0200\n")
        .assert()
        .success()
        .stdout("id,to_offset\n1,2024-06-15T06:00:00-06:00\n");
}

#[test]
fn offset_accepts_z_as_the_target() {
    csvdt()
        .args(["-H", "-p", "-o", "1,Z", "-i", "1,replace"])
        .write_stdin("id,ts\n1,2024-06-15T14:00:00+02:00\n")
        .assert()
        .success()
        .stdout("id,to_offset\n1,2024-06-15T12:00:00+00:00\n");
}

#[test]
fn offset_invalid_timestamp_is_a_parse_err() {
    csvdt()
        .args(["-H", "-p", "-o", "1,-06:00", "-i", "1,replace"])
        .write_stdin("id,ts\n1,not-a-date\n2,2024-01-01T00:00:00Z\n")
        .assert()
        .success()
        .stdout("id,to_offset\n1,parse_err\n2,2023-12-31T18:00:00-06:00\n");
}

#[test]
fn offset_rejects_a_malformed_target_offset() {
    csvdt()
        .args(["-o", "1,banana"])
        .write_stdin("id,ts\n1,2024-06-15T14:00:00+02:00\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("+02:00, -0600, or Z"));
}

#[test]
fn duration_single_column_is_computed_row_over_row() {
    csvdt()
        .args(["-H", "-p", "-d1", "-i", "1,replace"])
        .write_stdin(
            "id,ts\n\
             1,2024-01-01T00:00:00Z\n\
             2,2024-01-02T00:00:00Z\n\
             3,2024-01-01T12:00:00Z\n",
        )
        .assert()
        .success()
        .stdout("id,duration\n1,PT0S\n2,P1D\n3,PT12H\n");
}

#[test]
fn duration_cache_survives_a_parse_error_row() {
    // Regression test for a fixed bug: a parse error used to reset the
    // running duration baseline to the Unix epoch, corrupting every
    // duration computed after it. The row following the bad one must be
    // computed against the last valid timestamp (row 1), not 1970.
    csvdt()
        .args(["-H", "-p", "-d1", "-i", "1,replace"])
        .write_stdin(
            "id,ts\n\
             1,2024-01-01T00:00:00Z\n\
             2,not-a-date\n\
             3,2024-01-02T00:00:00Z\n",
        )
        .assert()
        .success()
        .stdout("id,duration\n1,PT0S\n2,parse_err\n3,P1D\n");
}

#[test]
fn duration_correct_across_a_dst_spring_forward() {
    // US clocks jump 02:00 -> 03:00 on 2024-03-10: these two timestamps'
    // wall-clock difference is 2h, but the real elapsed time is 1h. If
    // duration used naive wall-clock subtraction instead of the absolute
    // instant, this would come out as PT2H.
    csvdt()
        .args(["-H", "-p", "-d1", "-i", "1,replace"])
        .write_stdin(
            "id,ts\n\
             1,2024-03-10T01:30:00-05:00\n\
             2,2024-03-10T03:30:00-04:00\n",
        )
        .assert()
        .success()
        .stdout("id,duration\n1,PT0S\n2,PT1H\n");
}

#[test]
fn duration_two_columns_correct_between_two_countries_own_dst_offsets() {
    // US Eastern Daylight Time (-04:00) vs Central European Summer Time
    // (+02:00): each side carries its own country's DST-adjusted offset.
    // 12:00 EDT = 16:00 UTC, 20:00 CEST = 18:00 UTC; real difference is 2h.
    csvdt()
        .args(["-H", "-p", "-d", "0,1", "-i", "1,replace"])
        .write_stdin("us,eu\n2024-07-04T12:00:00-04:00,2024-07-04T20:00:00+02:00\n")
        .assert()
        .success()
        .stdout("us,row_duration\n2024-07-04T12:00:00-04:00,PT2H\n");
}

#[test]
fn duration_two_columns_computes_difference_on_the_same_row() {
    // -d with two columns replaces the old --row-duration action. Default
    // insert position (no -i given) is after the second column.
    csvdt()
        .args(["-H", "-p", "-d", "0,4"])
        .write_stdin(
            "start,mid1,mid2,mid3,end\n\
             2024-01-01T00:00:00Z,a,b,c,2024-01-02T12:00:00Z\n",
        )
        .assert()
        .success()
        .stdout(
            "start,mid1,mid2,mid3,end,<row_duration\n\
             2024-01-01T00:00:00Z,a,b,c,2024-01-02T12:00:00Z,P1DT12H\n",
        );
}

#[test]
fn duration_two_columns_explicit_insert_replaces_chosen_column() {
    csvdt()
        .args(["-H", "-p", "-d", "0,4", "-i", "4,replace"])
        .write_stdin(
            "start,mid1,mid2,mid3,end\n\
             2024-01-01T00:00:00Z,a,b,c,2024-01-02T12:00:00Z\n",
        )
        .assert()
        .success()
        .stdout("start,mid1,mid2,mid3,row_duration\n2024-01-01T00:00:00Z,a,b,c,P1DT12H\n");
}

#[test]
fn duration_two_columns_either_column_invalid_is_parse_err() {
    csvdt()
        .args(["-H", "-p", "-d", "0,1", "-i", "1,replace"])
        .write_stdin(
            "start,end\n\
             not-a-date,2024-01-02T12:00:00Z\n\
             2024-01-01T12:00:00Z,2024-01-02T12:00:00Z\n",
        )
        .assert()
        .success()
        .stdout(
            "start,row_duration\n\
             not-a-date,parse_err\n\
             2024-01-01T12:00:00Z,P1D\n",
        );
}

#[test]
fn duration_two_columns_out_of_bound_second_column_is_a_clean_error() {
    csvdt()
        .args(["-d", "0,9"])
        .write_stdin("2024-01-01T00:00:00Z,x\n")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("Specified column is out of bound"));
}

#[test]
fn duration_single_value_does_not_swallow_a_following_file_argument() {
    // Regression test for a fixed bug: -d used to take 1 or 2 separate
    // space-delimited values (num_args(1..=2)), so "-d 1 somefile.csv"
    // greedily consumed the filename as -d's second column, then failed
    // with a confusing "invalid digit found in string" instead of reading
    // column 1 from somefile.csv. -d now always takes exactly one value,
    // with the two-column form joined into that same token via a comma
    // (-d0,1), so a bare file argument right after -d's single value can
    // no longer be misinterpreted as part of -d.
    let dir = tempfile_dir();
    let path = dir.join("duration.csv");
    std::fs::write(&path, "id,ts\n1,2024-01-01T00:00:00Z\n2,2024-01-02T00:00:00Z\n").unwrap();

    csvdt()
        .args(["-H", "-p", "-d", "1", "-i", "1,replace"])
        .arg(&path)
        .assert()
        .success()
        .stdout("id,duration\n1,PT0S\n2,P1D\n");
}

#[test]
fn duration_extra_bare_token_is_a_clean_clap_error() {
    // -d takes exactly one value now, so a leftover bare token after it is
    // rejected outright by clap ("unexpected argument") instead of being
    // silently absorbed by the wrong argument.
    csvdt()
        .args(["-d", "0", "1", "2"])
        .write_stdin("a,b,c\n1,2,3\n")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
}

#[test]
fn remove_drops_the_named_column() {
    csvdt()
        .args(["-H", "-p", "--remove", "1"])
        .write_stdin("id,ts,val\n1,2024-01-01T00:00:00Z,10\n")
        .assert()
        .success()
        .stdout("id,val\n1,10\n");
}

#[test]
fn header_out_of_bound_column_is_a_clean_error() {
    csvdt()
        .args(["-H", "--remove", "9"])
        .write_stdin("id,ts,val\n1,2024-01-01T00:00:00Z,10\n")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("Specified column is out of bound"));
}

#[test]
fn empty_input_with_has_header_is_not_an_error_and_not_a_panic() {
    // Two fixed bugs meet here. On genuinely empty input the header record has
    // no fields, and the bounds check used to compute `vecbuff.len() - 1`,
    // underflowing and panicking with exit 101. That became a clean "out of
    // bound" error, which was still the wrong answer: it blamed the column
    // argument for the input being empty, and only when -H was given -- the
    // same input without it exits 0 and says nothing. No header means no rows
    // either, so there is nothing to convert, remove or reorder.
    for arguments in [
        vec!["-H", "--remove", "0"],
        vec!["-H", "-p", "-u0"],
        vec!["-H", "-p", "-a0"],
        vec!["-H", "-p", "-s0"],
        vec!["-H", "-p", "-d0"],
        vec!["-H", "-p", "-o0,+01:00"],
    ] {
        csvdt()
            .args(&arguments)
            .write_stdin("")
            .assert()
            .success()
            .code(0)
            .stdout("")
            .stderr("");
    }

    // Blank lines are skipped by the CSV reader, so they arrive the same way.
    csvdt()
        .args(["-H", "-p", "-u0"])
        .write_stdin("\n\n")
        .assert()
        .success()
        .stdout("");
}

#[test]
fn a_column_out_of_range_is_still_caught_once_there_is_a_header_to_measure() {
    // The empty-input case above must not have loosened this: a header with two
    // columns and an action on column 9 is a mistake, with or without rows.
    for input in ["ts,x\n", "ts,x\n2024-01-01T00:00:00Z,1\n"] {
        csvdt()
            .args(["-H", "-p", "-u9"])
            .write_stdin(input)
            .assert()
            .failure()
            .code(1)
            .stderr(predicate::str::contains("out of bound"));
    }
}

#[test]
fn record_out_of_bound_column_without_headers_is_a_clean_error() {
    csvdt()
        .args(["--remove", "9"])
        .write_stdin("1,2,3\n")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("Specified column is out of bound"));
}

// ---- insert positions -------------------------------------------------------

#[test]
fn insert_position_before_and_after_both_work_for_rfc3339() {
    csvdt()
        .args(["-H", "-p", "-r1", "-i", "1,before"])
        .write_stdin("id,ts,val\n1,1700000000,10\n")
        .assert()
        .success()
        .stdout("id,to_rfc3339>,ts,val\n1,2023-11-14T22:13:20+00:00,1700000000,10\n");

    csvdt()
        .args(["-H", "-p", "-r1", "-i", "1,after"])
        .write_stdin("id,ts,val\n1,1700000000,10\n")
        .assert()
        .success()
        .stdout("id,ts,<to_rfc3339,val\n1,1700000000,2023-11-14T22:13:20+00:00,10\n");
}

#[test]
fn insert_column_is_independent_of_the_action_own_column() {
    // -i can target any column, not just the one the action itself reads.
    csvdt()
        .args(["-H", "-p", "-r1", "-i", "0,before"])
        .write_stdin("id,ts,val\n1,1700000000,10\n")
        .assert()
        .success()
        .stdout("to_rfc3339>,id,ts,val\n2023-11-14T22:13:20+00:00,1,1700000000,10\n");
}

#[test]
fn insert_rejects_an_invalid_position_word() {
    csvdt()
        .args(["-r1", "-i", "0,sideways"])
        .write_stdin("a,b\n1700000000,x\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("before, after, or replace"));
}

#[test]
fn insert_rejects_a_value_missing_the_comma() {
    csvdt()
        .args(["-r1", "-i", "1replace"])
        .write_stdin("a,b\n1700000000,x\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("<column>,<position>"));
}

#[test]
fn offset_rejects_a_value_missing_the_comma() {
    csvdt()
        .args(["-o", "1-06:00"])
        .write_stdin("a,b\n1700000000,x\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("<column>,<offset>"));
}

#[test]
fn insert_without_an_action_is_rejected() {
    csvdt()
        .args(["-i", "0,after"])
        .write_stdin("a,b\n1,2\n")
        .assert()
        .failure()
        .code(2);
}

// ---- CSV parsing options: trim / flexible / quoting / delimiters --------

#[test]
fn trim_none_preserves_whitespace() {
    csvdt()
        .args(["--trim", "none"])
        .write_stdin("a,b\n\" 1 \",\" 2 \"\n")
        .assert()
        .success()
        .stdout("a,b\n 1 , 2 \n");
}

#[test]
fn trim_fields_strips_whitespace_from_data_rows() {
    csvdt()
        .args(["--trim", "fields"])
        .write_stdin("a,b\n\" 1 \",\" 2 \"\n")
        .assert()
        .success()
        .stdout("a,b\n1,2\n");
}

#[test]
fn each_trim_mode_touches_the_half_of_the_file_it_names() {
    // Four modes, and only two of them were tested: 'none' and 'fields'.
    // The pair that acts on headers went in unexercised, and they are the
    // two where getting it backwards is invisible in a file whose header
    // and data are whitespaced alike -- which is the file anyone reaches
    // for first.
    //
    // So the fixture is whitespaced on both, and each mode is asked for
    // both halves at once. A mode that trimmed the wrong half would pass
    // any check that looked at one.
    let file = "\" h1 \",\" h2 \"\n\" a \",\" b \"\n";
    for (mode, want) in [
        ("none", " h1 , h2 \n a , b \n"),
        ("fields", " h1 , h2 \na,b\n"),
        ("headers", "h1,h2\n a , b \n"),
        ("all", "h1,h2\na,b\n"),
    ] {
        csvdt()
            .args(["-H", "-p", "--trim", mode])
            .write_stdin(file)
            .assert()
            .success()
            .stdout(want);
    }

    // And which record is the header is what -H says, here as everywhere.
    // Without it the first record is data: 'fields' trims it and 'headers'
    // has nothing to reach.
    for (mode, want) in [
        ("fields", "h1,h2\na,b\n"),
        ("headers", " h1 , h2 \n a , b \n"),
    ] {
        csvdt()
            .args(["--trim", mode])
            .write_stdin(file)
            .assert()
            .success()
            .stdout(want);
    }
}

#[test]
fn trimming_takes_every_whitespace_unicode_knows_including_the_stubborn_one() {
    // The help says whitespace without saying whose. It is Unicode's, not
    // the space bar's, and the one worth knowing about is the non-breaking
    // space: it is usually there on purpose -- a thousands separator in a
    // good many locales, or a space something was typeset with -- and it
    // goes like any other.
    //
    // Written as bytes rather than as a character, so that what is being
    // asked about is legible in the test rather than being a space that
    // looks like every other space in this file.
    const NBSP: &str = "\u{00a0}";

    for (name, field) in [
        ("a tab", "\tx\t"),
        ("a carriage return", "\rx\r"),
        ("a newline inside a quoted field", "\nx\n"),
        ("a non-breaking space", "\u{00a0}x\u{00a0}"),
        ("a mix of them", " \t\u{00a0}x\u{00a0}\t "),
    ] {
        // Compared by hand rather than through the assertion helper, so a
        // failure says which of the five was left behind instead of only
        // showing a diff of two nearly identical lines of whitespace.
        let output = csvdt()
            .args(["-H", "-p", "--trim", "all"])
            .write_stdin(format!("h\n\"{field}\"\n"))
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&output.stdout), "h\nx\n",
            "{name} was not trimmed",
        );
    }

    // Leading and trailing only, which is what keeps a formatted number
    // readable: the pair around it go and the one between the digits stays.
    csvdt()
        .args(["-H", "-p", "--trim", "all"])
        .write_stdin(format!("h\n\"{NBSP}1{NBSP}000{NBSP}\"\n"))
        .assert()
        .success()
        .stdout(format!("h\n1{NBSP}000\n"));
}

#[test]
fn flexible_allows_ragged_rows() {
    csvdt()
        .arg("--flexible")
        .write_stdin("a,b,c\n1,2\n")
        .assert()
        .success()
        .stdout("a,b,c\n1,2\n");
}

#[test]
fn non_flexible_rejects_ragged_rows() {
    csvdt()
        .write_stdin("a,b,c\n1,2\n")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("found record with 2 fields"));
}

#[test]
fn single_quote_flag_treats_single_quotes_as_the_quote_character() {
    csvdt()
        .arg("--single-quote")
        .write_stdin("a,b\n'1,x',2\n")
        .assert()
        .success()
        .stdout("a,b\n\"1,x\",2\n");
}

#[test]
fn print_delimiter_changes_the_written_separator() {
    csvdt()
        .args(["--print-delimiter", ";"])
        .write_stdin("a,b\n1,2\n")
        .assert()
        .success()
        .stdout("a;b\n1;2\n");
}

#[test]
fn quote_style_always_quotes_every_field() {
    csvdt()
        .args(["--quote", "always"])
        .write_stdin("a,b\n1,2\n")
        .assert()
        .success()
        .stdout("\"a\",\"b\"\n\"1\",\"2\"\n");
}

#[test]
fn quote_style_never_can_produce_invalid_csv_on_purpose() {
    csvdt()
        .args(["--quote", "never"])
        .write_stdin("a,b\n\"1,x\",2\n")
        .assert()
        .success()
        .stdout("a,b\n1,x,2\n");
}

#[test]
fn comment_flag_skips_comment_lines() {
    csvdt()
        .args(["--comment", "#"])
        .write_stdin("# a comment\na,b\n1,2\n")
        .assert()
        .success()
        .stdout("a,b\n1,2\n");
}

#[test]
fn a_field_holding_the_comment_character_is_quoted_so_the_output_reads_back() {
    // The writer is told the comment character, so a field beginning with it
    // is quoted exactly as a field holding the delimiter would be, and the
    // output survives a second read with the same --comment. It used to be
    // written bare and the dropped row could only be counted and warned
    // about; now there is nothing to warn about under the default style.
    csvdt()
        .args(["-H", "-p", "--comment", "#"])
        .write_stdin("a,b\n\"#x\",2\n3,4\n\"#y\",5\n")
        .assert()
        .success()
        .stdout("a,b\n\"#x\",2\n3,4\n\"#y\",5\n")
        .stderr(predicate::str::contains("begin with").not());

    // The round trip is the point: the same options read the output back
    // whole, rows and all.
    csvdt()
        .args(["-H", "-p", "--comment", "#"])
        .write_stdin("a,b\n\"#x\",2\n3,4\n")
        .assert()
        .success()
        .stdout(predicate::function(|out: &str| {
            let mut second = std::process::Command::new(assert_cmd::cargo::cargo_bin("csvdt"))
                .args(["-H", "-p", "--comment", "#"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            use std::io::Write;
            second.stdin.take().unwrap().write_all(out.as_bytes()).unwrap();
            let reread = second.wait_with_output().unwrap();
            String::from_utf8_lossy(&reread.stdout) == *out
        }));

    // 'never' is the style the writer cannot quote under, so there the row
    // is still lost on a second read -- counted and said, as before.
    csvdt()
        .args(["-H", "-p", "--comment", "#", "--quote", "never"])
        .write_stdin("a,b\n\"#x\",2\n3,4\n\"#y\",5\n")
        .assert()
        .success()
        .stdout("a,b\n#x,2\n3,4\n#y,5\n")
        .stderr(predicate::str::contains("2 row(s) begin with '#'"));

    // Without --comment nothing is a comment: no quoting for it, nothing to
    // say.
    csvdt()
        .args(["-H", "-p"])
        .write_stdin("a,b\n\"#x\",2\n")
        .assert()
        .success()
        .stdout("a,b\n#x,2\n")
        .stderr(predicate::str::contains("begin with").not());
}

#[test]
fn the_warning_names_the_styles_that_actually_protect() {
    // It said "Every other style quotes such a field" -- but 'never' and
    // 'nonnumeric' are two independent gaps, so a user warned under one could
    // switch to the other and still lose the row. The advice must name the
    // two styles that protect it under every field, and nothing vaguer.
    for (arguments, input, gap) in [
        (vec!["--comment", "#", "--quote", "never"], "a,b\n\"#x\",2\n", "'never'"),
        (vec!["--comment", "-", "--quote", "nonnumeric"], "a,b\n\"-5\",2\n",
         "'nonnumeric'"),
    ] {
        csvdt()
            .args(["-H", "-p"])
            .args(&arguments)
            .write_stdin(input)
            .assert()
            .success()
            .stderr(predicate::str::contains(
                "'necessary', the default, and 'always'"))
            // And not the style this run is already using: the old wording
            // is gone from the binary, so asserting its absence asserted
            // nothing. What must stay true is that the style being warned
            // about is never itself offered as the way out of the warning.
            .stderr(predicate::str::contains(gap).not());
    }
}

#[test]
fn nonnumeric_still_writes_a_numeric_comment_field_bare_and_is_counted() {
    // 'nonnumeric' decides by whether the field is a number and consults
    // nothing else, so --comment '-' over the numeric field -5 is written
    // bare -- the one gap besides 'never', and still counted.
    csvdt()
        .args(["-H", "-p", "--comment", "-", "--quote", "nonnumeric"])
        .write_stdin("a,b\n\"-5\",2\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("1 row(s) begin with '-'"));

    // The same field under the default style is quoted and safe.
    csvdt()
        .args(["-H", "-p", "--comment", "-"])
        .write_stdin("a,b\n\"-5\",2\n")
        .assert()
        .success()
        .stdout("a,b\n\"-5\",2\n")
        .stderr(predicate::str::contains("begin with").not());
}

#[test]
fn nonnumeric_cannot_quote_a_number_holding_the_output_delimiter_and_says_so() {
    // The comment character is not the only thing 'nonnumeric' cannot quote a
    // number away from. A --print-delimiter a number can itself hold -- '.',
    // '-', '+', 'e', or a digit -- goes out bare inside one, and a second read
    // finds three fields where two were written. This was silent, which is the
    // same quiet damage the comment count exists to prevent, so it is counted
    // the same way.
    csvdt()
        .args(["-H", "-p", "--read-delimiter", ";",
               "--print-delimiter", ".", "--quote", "nonnumeric"])
        .write_stdin("a;b\n5.5;x\n9;y\n")
        .assert()
        .success()
        .stdout("\"a\".\"b\"\n5.5.\"x\"\n9.\"y\"\n")
        .stderr(predicate::str::contains("1 row(s) hold a number written bare"))
        .stderr(predicate::str::contains("contains '.'"));

    // Any field of the row, not only the first: a boundary invented in the
    // middle shifts every column number after it just as surely. Here the
    // first field is quoted and only the second splits.
    csvdt()
        .args(["-H", "-p", "--read-delimiter", ";",
               "--print-delimiter", ".", "--quote", "nonnumeric"])
        .write_stdin("a;b\nx;5.5\ny;z\n")
        .assert()
        .success()
        .stdout("\"a\".\"b\"\n\"x\".5.5\n\"y\".\"z\"\n")
        .stderr(predicate::str::contains("1 row(s) hold a number written bare"));

    // Counted once per row, not once per field: two split fields in one row is
    // one row that comes back the wrong shape.
    csvdt()
        .args(["-H", "-p", "--read-delimiter", ";",
               "--print-delimiter", ".", "--quote", "nonnumeric"])
        .write_stdin("a;b\n5.5;1.5\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("1 row(s) hold a number written bare"));

    // A digit is a delimiter a number can hold too, and reaches the message as
    // the character it is.
    csvdt()
        .args(["-H", "-p", "--read-delimiter", ";",
               "--print-delimiter", "5", "--quote", "nonnumeric"])
        .write_stdin("a;b\n15;x\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("contains '5'"));
}

#[test]
fn the_help_names_every_delimiter_a_number_can_hold_and_no_others() {
    // KNOWN LIMITS gives the set as the float syntax: any digit, '.', '-',
    // '+', 'e' or 'E', and the letters of inf, infinity and nan in either
    // case -- twenty-seven characters, and every other delimiter safe under
    // this style. That set is csv-core's numeric test rather than ours, so an
    // upgrade there could widen or narrow it and leave the help describing a
    // program that no longer exists. This asks the binary instead of trusting
    // the prose.
    //
    // The vocabulary is one row of numbers, each written bare by 'nonnumeric'
    // and between them holding every character a number can: if a candidate
    // delimiter is one of them it appears in this row, and the run must say
    // so. If it is not, the row is safe bare and the run must stay quiet --
    // which is what makes the negative half mean anything.
    const NUMBERS: [&str; 11] = [
        "0123456789", "5.5", "-5", "+5", "1e5", "1E5",
        "inf", "infinity", "nan", "INFINITY", "NAN",
    ];
    let named: std::collections::BTreeSet<char> = NUMBERS
        .iter()
        .flat_map(|number| number.chars())
        .collect();
    assert_eq!(named.len(), 27, "the help says twenty-seven: {named:?}");

    let mut fired = 0;
    let mut quiet = 0;
    for byte in 0x20u8..0x7f {
        let candidate = byte as char;
        // Refused as a delimiter outright, since quoting is done with it.
        if candidate == '"' {
            continue;
        }

        // The vocabulary, plus shapes built from the candidate itself in the
        // positions a character can occupy inside a number: alone, opening,
        // closing, infix, and after a leading zero, which is where a radix
        // prefix like 0x would sit. Those close the other half of the
        // question: if csv-core ever widened its test to call one of them a
        // number, the writer would leave it bare and the run would say so,
        // failing the quiet half rather than passing it by never having
        // offered the character. A syntax using none of these shapes could
        // still slip past -- the set is asked of the binary, not proved
        // exhaustively.
        //
        // Every field is quoted going in, so a candidate that is also the read
        // delimiter cannot split the input; what the writer does with it is
        // the point, not what the reader does.
        let mut fields: Vec<String> =
            NUMBERS.iter().map(|number| number.to_string()).collect();
        fields.extend([format!("5{candidate}5"), format!("{candidate}5"),
                       format!("5{candidate}"), format!("0{candidate}5"),
                       candidate.to_string()]);
        let input = format!(
            "{}\n{}\n",
            (0..fields.len()).map(|n| format!("c{n}")).collect::<Vec<_>>().join(";"),
            fields.iter()
                .map(|field| format!("\"{}\"", field.replace('"', "\"\"")))
                .collect::<Vec<_>>().join(";"));

        let says_so = predicate::str::contains(format!("contains '{candidate}'"))
            .and(predicate::str::contains("written bare"));
        let assertion = csvdt()
            .args(["-H", "-p", "--read-delimiter", ";",
                   "--print-delimiter", &candidate.to_string(),
                   "--quote", "nonnumeric"])
            .write_stdin(input)
            .assert()
            .success();
        if named.contains(&candidate) {
            fired += 1;
            assertion.stderr(says_so);
        } else {
            quiet += 1;
            assertion.stderr(predicate::str::contains("written bare").not());
        }
    }
    assert_eq!(fired, 27, "every character the help names must be counted");
    assert!(quiet > 60, "only {quiet} delimiters checked as safe");
}

#[test]
fn the_split_count_is_only_for_the_style_that_promised_to_quote() {
    // 'necessary' and 'always' quote a field holding the delimiter, so there is
    // nothing to warn about -- and the output really does read back.
    for style in ["necessary", "always"] {
        csvdt()
            .args(["-H", "-p", "--read-delimiter", ";",
                   "--print-delimiter", ".", "--quote", style])
            .write_stdin("a;b\n5.5;x\n")
            .assert()
            .success()
            .stdout(predicate::str::contains("\"5.5\"."))
            .stderr(predicate::str::contains("written bare").not());
    }

    // 'never' does write it bare, and is left alone: its whole description is
    // that it writes invalid CSV where the data needs quoting, so the warning
    // was given in advance rather than counted afterwards. Counting it here
    // would fire on almost every file the style is used on.
    csvdt()
        .args(["-H", "-p", "--read-delimiter", ";",
               "--print-delimiter", ".", "--quote", "never"])
        .write_stdin("a;b\n5.5;x\n")
        .assert()
        .success()
        .stdout("a.b\n5.5.x\n")
        .stderr(predicate::str::contains("written bare").not());

    // And the default delimiter is not one a number can hold, so the ordinary
    // use of 'nonnumeric' never sees this at all.
    csvdt()
        .args(["-H", "-p", "--quote", "nonnumeric"])
        .write_stdin("a,b\n5.5,x\n")
        .assert()
        .success()
        .stdout("\"a\",\"b\"\n5.5,\"x\"\n")
        .stderr(predicate::str::contains("written bare").not());
}

#[test]
fn a_field_that_is_not_a_number_is_quoted_however_it_holds_the_delimiter() {
    // The count follows the writer, not the mere presence of the delimiter: a
    // non-numeric field holding it is quoted by 'nonnumeric' like any other,
    // so it survives a second read and is not counted.
    csvdt()
        .args(["-H", "-p", "--read-delimiter", ";",
               "--print-delimiter", ".", "--quote", "nonnumeric"])
        .write_stdin("a;b\na.b;12\n")
        .assert()
        .success()
        .stdout("\"a\".\"b\"\n\"a.b\".12\n")
        .stderr(predicate::str::contains("written bare").not());
}

#[test]
fn a_row_that_is_one_empty_field_is_quoted_under_every_style() {
    // The one place 'never' writes a quote. Bare, the row is a blank line, and
    // a blank line is not a record any reader keeps -- so the record would
    // vanish rather than come back empty. --remove leaving a row with no
    // fields at all is written the same way.
    for style in ["never", "necessary", "always", "nonnumeric"] {
        csvdt()
            .args(["-H", "-p", "--quote", style])
            .write_stdin("a\nx\n\"\"\n")
            .assert()
            .success()
            .stdout(predicate::str::ends_with("\"\"\n"));

        csvdt()
            .args(["-H", "-p", "--remove", "0,1", "--quote", style])
            .write_stdin("a,b\n1,2\n")
            .assert()
            .success()
            .stdout("\"\"\n\"\"\n");
    }
}

#[test]
fn the_header_is_counted_among_them_and_said_separately() {
    // Under the default style the writer now quotes a column name holding the
    // comment character, so the header survives a second read like any row.
    csvdt()
        .args(["-H", "-p", "--comment", "#"])
        .write_stdin("\"#id\",ts\n1,a\n2,b\n")
        .assert()
        .success()
        .stdout("\"#id\",ts\n1,a\n2,b\n")
        .stderr(predicate::str::contains("begin with").not());

    // Under 'never' it cannot, and losing the header is worse than losing a
    // row: a second read takes the first data row for the column names, so it
    // is said separately.
    csvdt()
        .args(["-H", "-p", "--comment", "#", "--quote", "never"])
        .write_stdin("\"#id\",ts\n1,a\n2,b\n")
        .assert()
        .success()
        .stdout("#id,ts\n1,a\n2,b\n")
        .stderr(predicate::str::contains("1 row(s) begin with '#'"))
        .stderr(predicate::str::contains("The header is one of them"));

    // A data row alone does not get the header sentence, since nothing is
    // promoted -- that row is simply missing.
    csvdt()
        .args(["-H", "-p", "--comment", "#", "--quote", "never"])
        .write_stdin("id,ts\n\"#x\",a\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("1 row(s) begin with '#'"))
        .stderr(predicate::str::contains("The header is one of them").not());

    // Both at once: counted together, and the header still named.
    csvdt()
        .args(["-H", "-p", "--comment", "#", "--quote", "never"])
        .write_stdin("\"#id\",ts\n\"#x\",a\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("2 row(s) begin with '#'"))
        .stderr(predicate::str::contains("The header is one of them"));
}

#[test]
fn a_header_that_is_not_written_cannot_be_read_back_as_a_comment() {
    // Without -p/--print-header the header is not in the output at all, so
    // there is nothing there for a second read to drop, and nothing to say.
    csvdt()
        .args(["-H", "--comment", "#", "--quote", "never"])
        .write_stdin("\"#id\",ts\n1,a\n")
        .assert()
        .success()
        .stdout("1,a\n")
        .stderr(predicate::str::contains("begin with").not());

    // And a quoting style that keeps it quoted is silent either way.
    csvdt()
        .args(["-H", "-p", "--comment", "#", "--quote", "always"])
        .write_stdin("\"#id\",ts\n1,a\n")
        .assert()
        .success()
        .stdout("\"#id\",\"ts\"\n\"1\",\"a\"\n")
        .stderr(predicate::str::contains("begin with").not());
}

#[test]
fn losing_the_header_promotes_a_row_rather_than_dropping_it() {
    // Why the header is the worst of the three. Read back, the header is gone
    // and the first data row has become the column names -- and a header is
    // exempt from every conversion, so that row is not missing from the
    // output, it is sitting in it unconverted, with every column number still
    // lining up. This pins the behaviour the warning describes.
    csvdt()
        .args(["-H", "-p", "--comment", "#", "-u1"])
        .write_stdin("#id,ts\n1,2024-01-01T00:00:00Z\n2,2024-01-02T00:00:00Z\n")
        .assert()
        .success()
        // Row one became the header, so it carries the derived name rather
        // than a converted value; only row two converted.
        .stdout("1,2024-01-01T00:00:00Z,<to_utc\n\
                 2,2024-01-02T00:00:00Z,2024-01-02T00:00:00+00:00\n");
}

#[test]
fn the_row_a_comment_would_swallow_is_named_by_the_count_not_the_exit_status() {
    // A few rows like this is not a reason to fail the run -- the output is
    // written either way and may never be read back with --comment at all --
    // so it stays exit 0, like the parse_err tally it sits beside. Both can
    // be said by the same run.
    csvdt()
        .args(["-H", "-p", "--comment", "#", "--quote", "never", "-u1"])
        .write_stdin("a,ts\n\"#x\",nonsense\n3,2024-01-01T00:00:00Z\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("1 row(s) begin with '#'"))
        .stderr(predicate::str::contains("could not be converted"));
}

#[test]
fn comment_handling_is_off_until_the_option_is_given() {
    // --comment's help claimed a default of b'#', and there is none: without
    // the option a leading '#' is data. Worse, in the file above it is data of
    // the wrong width, so the run fails on the comment line rather than
    // skipping it -- which is exactly the confusion the claim would cause.
    csvdt()
        .write_stdin("# a comment\na,b\n1,2\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("found record with 2 fields"));

    // Same width, and it comes straight through as a record.
    csvdt()
        .write_stdin("#,x\na,b\n")
        .assert()
        .success()
        .stdout("#,x\na,b\n");
}

// ---- argument validation ---------------------------------------------------

#[test]
fn conflicting_actions_are_rejected() {
    csvdt()
        .args(["-H", "-r1", "-u1"])
        .write_stdin("id,ts\n1,2024-01-01T00:00:00Z\n")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn every_pair_of_actions_conflicts() {
    // The single conflicting_actions_are_rejected test above only exercises
    // one of the 21 possible pairs among the 7 actions. conflicts_with_all
    // is declared by hand on each action (not derived), so a missing or
    // one-sided entry would silently let two actions run together instead
    // of erroring -- check every pair, not just one.
    let actions: Vec<Vec<&str>> = vec![
        vec!["-s1"],
        vec!["-r1"],
        vec!["-u1"],
        vec!["-l1"],
        vec!["-d1"],
        vec!["--remove", "1"],
        vec!["-o", "1,-06:00"],
    ];
    for i in 0..actions.len() {
        for j in (i + 1)..actions.len() {
            let mut args = actions[i].clone();
            args.extend(actions[j].clone());
            csvdt()
                .args(&args)
                .write_stdin("a,b\n1,2\n")
                .assert()
                .failure()
                .code(2)
                .stderr(predicate::str::contains("cannot be used with"));
        }
    }
}

#[test]
fn duration_two_columns_still_conflicts_with_other_actions() {
    // The two-column form of -d must be just as subject to conflicts_with_all
    // as the one-column form.
    csvdt()
        .args(["-d", "0,1", "-s2"])
        .write_stdin("a,b,c\n1,2,3\n")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn remove_does_not_accept_an_insert_position() {
    // --remove is deliberately excluded from "actions_req", the group -i
    // requires -- there's no before/after/replace concept for a removed
    // column. Combining --remove with -i should fail the same way -i with no
    // action at all does.
    csvdt()
        .args(["--remove", "1", "-i", "1,after"])
        .write_stdin("a,b\n1,2\n")
        .assert()
        .failure()
        .code(2);
}

#[test]
fn duration_requires_at_least_one_column() {
    csvdt()
        .arg("-d")
        .write_stdin("a,b\n1,2\n")
        .assert()
        .failure()
        .code(2);
}

#[test]
fn insert_column_too_high_clamps_to_the_last_column() {
    // -i's column can never be out of bound: too high clamps to the last
    // column instead of erroring.
    csvdt()
        .args(["-r0", "-i", "9,after"])
        .write_stdin("a,b\n1700000000,x\n")
        .assert()
        .success()
        .stdout("a,b,parse_err\n1700000000,x,2023-11-14T22:13:20+00:00\n");
}

#[test]
fn insert_column_negative_clamps_to_the_first_column() {
    // Symmetric to the too-high case: a negative -i column clamps to 0
    // instead of erroring.
    csvdt()
        .args(["-r0", "-i", "-1,before"])
        .write_stdin("a,b\n1700000000,x\n")
        .assert()
        .success()
        .stdout("parse_err,a,b\n2023-11-14T22:13:20+00:00,1700000000,x\n");
}

#[test]
fn action_without_a_column_value_is_rejected() {
    csvdt()
        .arg("-s")
        .write_stdin("a,b\n1,2\n")
        .assert()
        .failure()
        .code(2);
}

#[test]
fn multi_character_read_delimiter_is_rejected_with_a_clean_error() {
    csvdt()
        .args(["--read-delimiter", ";;"])
        .write_stdin("a;;b\n1;;2\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("expects a single character"));
}

#[test]
fn multi_character_print_delimiter_is_rejected_with_a_clean_error() {
    csvdt()
        .args(["--print-delimiter", ";;"])
        .write_stdin("a,b\n1,2\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("expects a single character"));
}

#[test]
fn multi_character_comment_is_rejected_with_a_clean_error() {
    csvdt()
        .args(["--comment", ";;"])
        .write_stdin("a,b\n1,2\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("expects a single character"));
}

#[test]
fn duration_row_over_row_handles_a_unix_epoch_timestamp_as_real_data() {
    // The row-over-row cache used to be seeded with a sentinel equal to the
    // Unix epoch, so a real epoch timestamp in the data was mistaken for
    // "no previous row yet" and silently reported PT0S.
    csvdt()
        .args(["-H", "-p", "-d0"])
        .write_stdin("ts\n1970-01-01T00:00:00+00:00\n1970-01-03T00:00:00+00:00\n")
        .assert()
        .success()
        .stdout("ts,<duration\n1970-01-01T00:00:00+00:00,PT0S\n1970-01-03T00:00:00+00:00,P2D\n");
}

#[test]
fn duration_row_over_row_handles_an_epoch_timestamp_mid_stream() {
    csvdt()
        .args(["-H", "-p", "-d0"])
        .write_stdin("ts\n2024-01-01T00:00:00Z\n1970-01-01T00:00:00+00:00\n1970-01-02T00:00:00+00:00\n")
        .assert()
        .success()
        .stdout(
            "ts,<duration\n2024-01-01T00:00:00Z,PT0S\n\
             1970-01-01T00:00:00+00:00,P19723D\n\
             1970-01-02T00:00:00+00:00,P1D\n",
        );
}

#[test]
fn parse_error_marker_is_plain_in_data_rows_for_every_action() {
    // The `>`/`<` decorations mark which column a *header* refers to; a data
    // row's parse failure is always the plain `parse_err` token so downstream
    // tooling can filter on one value.
    // Each action gets a value it can convert alongside, since a run where
    // nothing converts is an error of its own now.
    for (action, convertible) in [
        ("-u0", "2024-01-01T00:00:00Z"),
        ("-l0", "2024-01-01T00:00:00Z"),
        ("-r0", "1700000000"),
    ] {
        csvdt()
            .args(["-H", "-p", action])
            .write_stdin(format!("ts\n{convertible}\nnot-a-date\n"))
            .assert()
            .success()
            .stdout(predicate::str::contains("not-a-date,parse_err\n"));
    }

    csvdt()
        .args(["-H", "-p", "-o0,Z"])
        .write_stdin("ts\n2024-01-01T00:00:00Z\nnot-a-date\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("not-a-date,parse_err\n"));

    csvdt()
        .args(["-H", "-p", "-s0"])
        .write_stdin("ts\n2024-01-01T00:00:00Z\nnot-a-date\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("not-a-date,parse_err,parse_err\n"));
}

#[test]
fn duration_keeps_sub_second_precision_by_default() {
    csvdt()
        .args(["-H", "-p", "-d0,1"])
        .write_stdin("a,b\n2024-01-01T00:00:00.000Z,2024-01-01T00:00:00.999Z\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(",PT0.999S\n"));

    csvdt()
        .args(["-H", "-p", "-d0,1"])
        .write_stdin("a,b\n2024-01-01T00:00:00Z,2024-01-02T03:04:05.25Z\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(",P1DT3H4M5.25S\n"));
}

#[test]
fn duration_round_seconds_rounds_half_up_and_carries() {
    csvdt()
        .args(["-H", "-p", "-d0,1", "--round-seconds"])
        .write_stdin(
            "a,b\n\
             2024-01-01T00:00:00Z,2024-01-01T00:00:00.4Z\n\
             2024-01-01T00:00:00Z,2024-01-01T00:00:00.5Z\n\
             2024-01-01T00:00:00Z,2024-01-01T00:00:59.6Z\n\
             2024-01-01T00:00:00Z,2024-01-01T23:59:59.8Z\n",
        )
        .assert()
        .success()
        .stdout(
            "a,b,<row_duration\n\
             2024-01-01T00:00:00Z,2024-01-01T00:00:00.4Z,PT0S\n\
             2024-01-01T00:00:00Z,2024-01-01T00:00:00.5Z,PT1S\n\
             2024-01-01T00:00:00Z,2024-01-01T00:00:59.6Z,PT1M\n\
             2024-01-01T00:00:00Z,2024-01-01T23:59:59.8Z,P1D\n",
        );
}

#[test]
fn duration_round_seconds_applies_to_row_over_row_too() {
    csvdt()
        .args(["-H", "-p", "-d0", "--round-seconds"])
        .write_stdin("ts\n2024-01-01T00:00:00Z\n2024-01-01T00:00:01.25Z\n")
        .assert()
        .success()
        .stdout("ts,<duration\n2024-01-01T00:00:00Z,PT0S\n2024-01-01T00:00:01.25Z,PT1S\n");
}

#[test]
fn round_seconds_requires_duration_and_has_no_short_form() {
    csvdt()
        .args(["-H", "-p", "--round-seconds"])
        .write_stdin("a\n1\n")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("--duration"));

    // The option is deliberately long-only.
    csvdt()
        .args(["-H", "-p", "-d0", "-x"])
        .write_stdin("a\n1\n")
        .assert()
        .failure()
        .code(2);
}

// ---- timezone / DST conversion accuracy ----------------------------------
// These pin down -l/-u/-o against real IANA rules. Every expected value here
// was cross-checked against both Python's zoneinfo and GNU date reading the
// same system tzdata.

#[test]
fn local_handles_dst_transitions_to_the_second() {
    // The instant one second before and one second after the US spring-forward
    // (2024-03-10 07:00Z) and fall-back (2024-11-03 06:00Z) boundaries.
    csvdt()
        .args(["-H", "-p", "-l0", "-i", "0,replace"])
        .env("TZ", "America/Chicago")
        .write_stdin(
            "ts\n\
             2024-03-10T07:59:59Z\n\
             2024-03-10T08:00:00Z\n\
             2024-11-03T06:59:59Z\n\
             2024-11-03T07:00:00Z\n",
        )
        .assert()
        .success()
        .stdout(
            "to_local\n\
             2024-03-10T01:59:59-06:00\n\
             2024-03-10T03:00:00-05:00\n\
             2024-11-03T01:59:59-05:00\n\
             2024-11-03T01:00:00-06:00\n",
        );
}

#[test]
fn local_handles_sub_hour_offsets_and_southern_hemisphere_dst() {
    // Half-hour, 45-minute, and 30-minute-DST zones, plus a southern
    // hemisphere zone whose DST season is inverted relative to the north.
    for (zone, input, expected) in [
        ("Asia/Kolkata", "2024-07-15T12:00:00Z", "2024-07-15T17:30:00+05:30"),
        ("Asia/Kathmandu", "2024-07-15T12:00:00Z", "2024-07-15T17:45:00+05:45"),
        // Lord Howe shifts by only 30 minutes for DST, not a full hour.
        ("Australia/Lord_Howe", "2024-01-15T12:00:00Z", "2024-01-15T23:00:00+11:00"),
        ("Australia/Lord_Howe", "2024-07-15T12:00:00Z", "2024-07-15T22:30:00+10:30"),
        // Chatham is +12:45 / +13:45.
        ("Pacific/Chatham", "2024-07-15T12:00:00Z", "2024-07-16T00:45:00+12:45"),
        ("Pacific/Chatham", "2024-01-15T12:00:00Z", "2024-01-16T01:45:00+13:45"),
        // Southern hemisphere: DST in January, standard in July.
        ("Australia/Sydney", "2024-01-15T12:00:00Z", "2024-01-15T23:00:00+11:00"),
        ("Australia/Sydney", "2024-07-15T12:00:00Z", "2024-07-15T22:00:00+10:00"),
        ("America/Santiago", "2024-01-15T12:00:00Z", "2024-01-15T09:00:00-03:00"),
        ("America/Santiago", "2024-07-15T12:00:00Z", "2024-07-15T08:00:00-04:00"),
    ] {
        csvdt()
            .args(["-H", "-p", "-l0", "-i", "0,replace"])
            .env("TZ", zone)
            .write_stdin(format!("ts\n{}\n", input))
            .assert()
            .success()
            .stdout(format!("to_local\n{}\n", expected));
    }
}

#[test]
fn offset_handles_the_extremes_and_sub_hour_targets() {
    // -o is pure fixed-offset arithmetic, so it must be exact at the legal
    // extremes (+14:00 / -12:00) and for 30/45-minute targets, including
    // when the conversion moves the result onto a different calendar date.
    for (target, expected) in [
        ("+14:00", "2024-06-15T04:00:00+14:00"),
        ("-12:00", "2024-06-14T02:00:00-12:00"),
        ("+05:45", "2024-06-14T19:45:00+05:45"),
        ("-09:30", "2024-06-14T04:30:00-09:30"),
        ("+12:45", "2024-06-15T02:45:00+12:45"),
        ("Z", "2024-06-14T14:00:00+00:00"),
    ] {
        csvdt()
            .args(["-H", "-p", &format!("-o0,{}", target), "-i", "0,replace"])
            .write_stdin("ts\n2024-06-14T16:00:00+02:00\n")
            .assert()
            .success()
            .stdout(format!("to_offset\n{}\n", expected));
    }
}

#[test]
fn utc_and_offset_preserve_the_instant_across_a_conversion_chain() {
    // Converting to an arbitrary offset and back to UTC must land on exactly
    // the original instant -- the property that matters most when a timestamp
    // is evidence.
    let original = "2024-11-03T05:30:00+00:00";
    csvdt()
        .args(["-H", "-p", "-o0,+05:45", "-i", "0,replace"])
        .write_stdin("ts\n2024-11-03T05:30:00Z\n")
        .assert()
        .success()
        .stdout("to_offset\n2024-11-03T11:15:00+05:45\n");

    // Feed that result back through -u and we must recover the original.
    csvdt()
        .args(["-H", "-p", "-u0", "-i", "0,replace"])
        .write_stdin("ts\n2024-11-03T11:15:00+05:45\n")
        .assert()
        .success()
        .stdout(format!("to_utc\n{}\n", original));
}

#[test]
fn there_is_no_year_zero_to_convert_into() {
    // Four digits would hold 0000, and the guard used to. But the calendar
    // runs 1 BC straight into AD 1, and ISO 8601 writes 0000 for 1 BC only
    // where both sides have agreed to read it that way -- RFC3339 makes no
    // such agreement. A value landing there came from something other than a
    // date somebody recorded: a sentinel, or an epoch counted in the wrong
    // unit. AD 1 is the floor, and it converts.
    for (arguments, input, expected) in [
        (vec!["-H", "-p", "-u0"], "0001-01-01T00:00:00+00:00", "0001-01-01T00:00:00+00:00"),
        (vec!["-H", "-p", "-u0"], "0001-01-01T00:00:00+14:00", "parse_err"),
        // -o reaches it from the other side, moving the clock rather than the
        // instant: AD 1 on a -12:00 clock is the year before there is one.
        (vec!["-H", "-p", "-o0,-12:00"], "0001-01-01T00:00:00+00:00", "parse_err"),
        (vec!["-H", "-p", "-o0,+14:00"], "0001-01-01T00:00:00+00:00", "0001-01-01T14:00:00+14:00"),
    ] {
        csvdt()
            .args(&arguments)
            .args(["-i", "0,replace"])
            .write_stdin(format!("ts\n{input}\n"))
            .assert()
            .stdout(predicate::str::contains(expected));
    }

    // -s/--split does not convert, so it neither crosses the boundary nor
    // needs the guard: it writes back the date the record already carried.
    csvdt()
        .args(["-H", "-p", "-s0", "-i", "0,replace"])
        .write_stdin("ts\n0001-01-01T00:00:00+00:00\n")
        .assert()
        .success()
        .stdout("split_date,split_time\n0001-01-01,00:00:00 +00:00\n");
}

#[test]
fn local_marks_a_zone_offset_that_is_not_whole_minutes() {
    // RFC3339 writes an offset as +HH:MM, with nowhere to put a seconds part.
    // A zone's local mean time -- what every zone used before it adopted
    // standard time -- is rarely a whole minute, and chrono renders one by
    // rounding the offset while the clock time keeps its seconds. The result
    // named a different instant from the input: Africa/Monrovia's -00:44:30
    // turned 1971-06-01T12:00:00Z into 1971-06-01T11:15:30-00:45, which reads
    // back as 12:00:30Z. Marked now, with the input still beside it.
    //
    // Monrovia rather than an 1800s date on purpose: it kept -00:44:30 until
    // January 1972, so this is inside the range of real logs.
    csvdt()
        .args(["-H", "-p", "-l0"])
        .env("TZ", "Africa/Monrovia")
        .write_stdin("ts\n1971-06-01T12:00:00Z\n")
        .assert()
        .failure()
        .stdout("ts,<to_local\n1971-06-01T12:00:00Z,parse_err\n");

    // The same zone once it is on a whole minute converts as it always did,
    // so this refuses the unwritable value rather than the zone.
    csvdt()
        .args(["-H", "-p", "-l0", "-i", "0,replace"])
        .env("TZ", "Africa/Monrovia")
        .write_stdin("ts\n1990-06-01T12:00:00Z\n")
        .assert()
        .success()
        .stdout("to_local\n1990-06-01T12:00:00+00:00\n");
}

#[test]
fn local_mean_time_is_marked_rather_than_written_off_by_seconds() {
    // Every zone has such a period. Each of these used to come out as a
    // timestamp whose two halves disagreed -- Amsterdam by 30 seconds, Oslo
    // by 28, New York by 2.
    for zone in ["Europe/Amsterdam", "Europe/Oslo", "America/New_York", "Asia/Kolkata"] {
        csvdt()
            .args(["-H", "-p", "-l0", "-i", "0,replace"])
            .env("TZ", zone)
            .write_stdin("ts\n1800-01-01T12:00:00Z\n")
            .assert()
            .failure()
            .stdout("to_local\nparse_err\n");
    }
}

#[test]
fn utc_offset_and_split_are_untouched_by_the_whole_minute_guard() {
    // The guard belongs to -l alone: the other three work from offsets that
    // are whole minutes by construction, so an 1800s timestamp still converts.
    csvdt()
        .args(["-H", "-p", "-u0", "-i", "0,replace"])
        .write_stdin("ts\n1800-01-01T12:00:00Z\n")
        .assert()
        .success()
        .stdout("to_utc\n1800-01-01T12:00:00+00:00\n");

    csvdt()
        .args(["-H", "-p", "-o0,+05:45", "-i", "0,replace"])
        .write_stdin("ts\n1800-01-01T12:00:00Z\n")
        .assert()
        .success()
        .stdout("to_offset\n1800-01-01T17:45:00+05:45\n");

    csvdt()
        .args(["-H", "-p", "-s0", "-i", "0,replace"])
        .write_stdin("ts\n1800-01-01T12:00:00Z\n")
        .assert()
        .success()
        .stdout("split_date,split_time\n1800-01-01,12:00:00 +00:00\n");
}

#[test]
fn local_still_converts_every_sub_hour_zone_that_is_whole_minutes() {
    // The guard tests the seconds, not the minutes: a zone that is half an
    // hour or three quarters off the hour is perfectly writable, and must not
    // be caught by it.
    for (zone, expected) in [
        ("Asia/Kolkata", "2024-07-15T17:30:00+05:30"),
        ("Asia/Kathmandu", "2024-07-15T17:45:00+05:45"),
        ("Pacific/Chatham", "2024-07-16T00:45:00+12:45"),
        ("Australia/Lord_Howe", "2024-07-15T22:30:00+10:30"),
    ] {
        csvdt()
            .args(["-H", "-p", "-l0", "-i", "0,replace"])
            .env("TZ", zone)
            .write_stdin("ts\n2024-07-15T12:00:00Z\n")
            .assert()
            .success()
            .stdout(format!("to_local\n{}\n", expected));
    }
}

#[test]
fn local_is_correct_for_morocco() {
    // Regression test for the reason -l uses the bundled IANA database rather
    // than chrono's own tzdata reader: that reader never applied Morocco's
    // daylight-saving rule, reporting +00:00 year-round. Verified against both
    // GNU date and Python zoneinfo -- Morocco is permanently +01:00 since
    // 2018, dropping to +00:00 only for Ramadan.
    csvdt()
        .args(["-H", "-p", "-l0", "-i", "0,replace"])
        .env("TZ", "Africa/Casablanca")
        .write_stdin("ts\n2026-07-25T12:00:00Z\n2026-03-01T12:00:00Z\n")
        .assert()
        .success()
        .stdout("to_local\n2026-07-25T13:00:00+01:00\n2026-03-01T12:00:00+00:00\n");
}

#[test]
fn version_reports_the_bundled_iana_database_release() {
    // -l's results are determined by the built-in database, so which release
    // that is has to be discoverable to tie any output to a known ruleset.
    csvdt()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("IANA tzdata"));
}

#[test]
fn local_handles_the_tz_variables_edge_cases() {
    // An empty TZ means UTC (POSIX), and a leading ':' is POSIX's
    // implementation-defined form that has to be stripped before lookup.
    for (tz, expected) in [
        ("", "2026-07-25T12:00:00+00:00"),
        ("UTC", "2026-07-25T12:00:00+00:00"),
        (":America/Chicago", "2026-07-25T07:00:00-05:00"),
        ("America/Chicago", "2026-07-25T07:00:00-05:00"),
    ] {
        csvdt()
            .args(["-H", "-p", "-l0", "-i", "0,replace"])
            .env("TZ", tz)
            .write_stdin("ts\n2026-07-25T12:00:00Z\n")
            .assert()
            .success()
            .stdout(format!("to_local\n{}\n", expected));
    }
}

#[test]
#[cfg(unix)]
fn local_falls_back_to_chrono_for_a_posix_rule_string() {
    // A TZ holding POSIX rules rather than a zone name can't be looked up in
    // the IANA database, so -l defers to chrono's own parsing. Verified to
    // match GNU date for this rule.
    //
    // Unix only: chrono reads TZ on Unix but not on Windows, where the system
    // timezone wins and a rule string like this is ignored. Zone *names* are
    // looked up by csvdt itself and so work identically on every platform --
    // this test covers the one path that genuinely differs.
    csvdt()
        .args(["-H", "-p", "-l0", "-i", "0,replace"])
        .env("TZ", "EST5EDT,M3.2.0,M11.1.0")
        .write_stdin("ts\n2026-07-25T12:00:00Z\n")
        .assert()
        .success()
        .stdout("to_local\n2026-07-25T08:00:00-04:00\n");
}

#[test]
#[ignore = "known gap in the bundled IANA database (chrono-tz 0.10 / tzdata \
            2025b): the deprecated EET and WET aliases resolve an hour late \
            for historical dates -- EET in 1975-1976, WET before 1997. The \
            geographic zones they stand in for (Europe/Athens, \
            Europe/Lisbon, ...) are correct, as are CET, MET and EST5EDT. \
            Un-ignore if a later chrono-tz corrects these aliases."]
fn local_is_correct_for_the_deprecated_eet_wet_aliases() {
    csvdt()
        .args(["-H", "-p", "-l0", "-i", "0,replace"])
        .env("TZ", "WET")
        .write_stdin("ts\n1995-07-15T12:00:00Z\n")
        .assert()
        .success()
        .stdout("to_local\n1995-07-15T13:00:00+01:00\n");
}

#[test]
fn the_manual_page_is_rendered_from_the_options_themselves() {
    // The page is generated rather than kept, so it cannot describe a build
    // it did not come out of. What a generator cannot promise is that the
    // shape is still a man page, so that is what is asked here.
    let page = String::from_utf8(
        csvdt()
            .arg("--generate-man")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .expect("the page should be UTF-8");

    // The header line a man reader needs before anything else.
    assert!(page.contains(".TH CSVDT 1"), "no .TH header:\n{}",
            &page[..page.len().min(200)]);
    // NAME is one line, not the six paragraphs 'about' carries for --help.
    let name = page
        .lines()
        .skip_while(|line| *line != ".SH NAME")
        .nth(1)
        .unwrap_or("");
    assert!(name.starts_with("csvdt \\-"), "NAME reads {name:?}");
    assert!(!name.is_empty() && name.len() < 120, "NAME is not one line: {name:?}");

    // Every section, including the three promoted out of clap_mangen's
    // catch-all. A heading renamed in the help files without the list in
    // main.rs being updated loses its section, and fails here.
    for section in [".SH NAME", ".SH SYNOPSIS", ".SH DESCRIPTION", ".SH OPTIONS",
                    ".SH TIMESTAMP FORMATS", ".SH KNOWN LIMITS",
                    ".SH EXIT STATUS", ".SH VERSION"] {
        assert!(page.contains(section), "the page has no {section}");
    }
    // And the catch-all itself is gone, since what it held now stands alone.
    assert!(!page.contains(".SH EXTRA"), "the EXTRA catch-all is still there");

    // Prose mentioning a heading mid-sentence must not have become one.
    // 'KNOWN LIMITS' appears in the option help three times, in sentences
    // pointing at the section; promoting one of those would both invent a
    // section and take the sentence apart.
    //
    // Asked as a count and a shape rather than by quoting the sentence: the
    // first version pinned the words, and failed when the sentence was
    // reworded to read correctly in a page as well as in --help, which is a
    // change the words were never the point of.
    assert_eq!(
        page.matches(".SH KNOWN LIMITS").count(),
        1,
        "KNOWN LIMITS is a section more than once, so prose was promoted",
    );
    assert!(
        page.lines().any(|line| line.contains("KNOWN LIMITS")
            && !line.starts_with(".SH")),
        "no prose mentions KNOWN LIMITS any more, so this checks nothing",
    );

    // The options are described, which is the whole reason to generate it.
    for option in ["\\-\\-flexible", "\\-\\-arrange", "\\-\\-generate\\-man"] {
        assert!(page.contains(option), "the page does not document {option}");
    }
}

#[test]
fn the_help_files_fit_the_column_the_manual_page_leaves_them() {
    // man indents an option's help by 14 columns and a section's by 7, and
    // the help files are laid out by hand -- so their own width decides
    // whether the page fits an 80-column terminal. 64 leaves room for the
    // deeper of the two indents plus the margin groff keeps.
    //
    // This is what stops the tables being readable in --help and wrapped to
    // pieces in man. It is a property of the source, so it is asked of the
    // source: rendering cannot answer it without a roff, and CI has one only
    // inside the RPM build.
    const LIMIT: usize = 64;
    // Where the 64 comes from, so that raising it fails here rather than in
    // the page. Both the corpus's README and the comment over
    // preserve_aligned_blocks once put this at 72, reasoning from the
    // 7-column indent a section's body gets and forgetting that a line in an
    // option's help sits 14 in -- and that a marked line is held with '.br',
    // which leaves filling on, so roff wraps a line too long for the terminal
    // and then justifies what it wrapped. A '--peek' example widened to the
    // 72 those two licensed reached an 80-column page as
    //
    //     $       csvdt       -H       -p        --peek        sales.csv
    //     xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
    //
    // and nothing failed, because the corpus had never used the width it was
    // told it could. A constant with its reasoning left in prose is a
    // constant the next reader may raise; this is the reasoning as an assert.
    const MAN_OPTION_INDENT: usize = 14;
    const TERMINAL: usize = 80;
    assert!(
        LIMIT + MAN_OPTION_INDENT <= TERMINAL,
        "{LIMIT} columns under man's {MAN_OPTION_INDENT}-column indent for an \
         option's help is {}, past the {TERMINAL} a page is written for",
        LIMIT + MAN_OPTION_INDENT,
    );
    let help = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/help");

    let mut over = Vec::new();
    for entry in std::fs::read_dir(&help).expect("src/help should be there") {
        let path = entry.unwrap().path();
        // The clap template is not help text: it is the frame the help is
        // poured into, and its lines are never indented by man.
        if path.extension().is_none_or(|e| e != "txt")
            || path.file_name().is_some_and(|n| n == "help_template.txt")
        {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for (number, line) in text.lines().enumerate() {
            if line.chars().count() > LIMIT {
                over.push(format!(
                    "  {}:{} is {} columns",
                    path.file_name().unwrap().to_string_lossy(),
                    number + 1,
                    line.chars().count(),
                ));
            }
        }
    }
    assert!(
        over.is_empty(),
        "help lines wider than {LIMIT} columns wrap in the manual page:\n{}",
        over.join("\n"),
    );
}

#[test]
fn the_first_line_naming_an_option_is_that_option_s_own_entry() {
    // The Emacs front end scrapes --help when --list-options is not there to
    // read, and takes the first line naming an option as that option's
    // entry, since csvdt's own prose names options and a wrapped line can
    // start with one. So a line of help text beginning '--help' becomes the
    // entry for --help, and '-h' is lost with it -- which is exactly what
    // --generate-man's help did the moment it gained a sentence beginning
    // that way, and what the front end's own checks caught.
    //
    // The test beside this one asks about the source and only about indented
    // lines, which is the cheap approximation: a flush-left line reads as an
    // option entry just as well once clap has indented the whole block. This
    // asks the rendered help the question that actually matters.
    let listed = option_list();
    let help = String::from_utf8(csvdt().arg("--help").output().unwrap().stdout)
        .expect("--help should be UTF-8");

    let mut wrong = Vec::new();
    for record in &listed {
        let (short, long) = (record[0].as_str(), record[1].as_str());
        if short.is_empty() || long.is_empty() {
            continue;
        }
        let entry = format!("{short}, {long}");
        let first = help.lines().map(str::trim_start).find(|line| {
            line.starts_with(&entry) || {
                // The long form on its own, not merely as a prefix of a
                // longer name: '--utc' must not match '--utc-something'.
                line.strip_prefix(long).is_some_and(|rest| {
                    !rest.starts_with(|c: char| c.is_alphanumeric() || c == '-')
                })
            }
        });
        match first {
            Some(line) if line.starts_with(&entry) => (),
            Some(line) => wrong.push(format!(
                "  {long}: first named by {line:?}, so a scrape of --help \
                 takes that for its entry and loses {short}")),
            None => wrong.push(format!("  {long}: not in --help at all")),
        }
    }
    assert!(
        wrong.is_empty(),
        "an option is first named by something other than its own entry:\n{}",
        wrong.join("\n"),
    );
}

#[test]
fn no_help_line_begins_with_something_that_looks_like_an_option() {
    // In the description column a line beginning '--' is indistinguishable
    // from the start of a new option entry -- to a reader skimming, and to
    // anything parsing --help, which is how rewrapping the corpus briefly
    // invented two options that do not exist:
    //
    //   left:  {"--", "--arrange", ..., "--remove", "--remove.", ...}
    //   right: {"--arrange", ..., "--remove", ...}
    //
    // That includes '--' used as a dash, which is why this asks about the
    // two characters rather than about a known option name.
    let help = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/help");

    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&help).expect("src/help should be there") {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "txt") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for (number, line) in text.lines().enumerate() {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            // A heading in the trailing sections is flush left and is not in
            // any description column, so it cannot be mistaken for an entry.
            let heading = line == line.to_uppercase() && !line.trim().is_empty();
            if line.trim_start().starts_with("--")
                && line.starts_with(' ')
                && !heading
            {
                offenders.push(format!("  {name}:{}: {line}", number + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "help lines starting with '--' read as option entries:\n{}",
        offenders.join("\n"),
    );
}

#[test]
fn the_short_help_lists_every_option_the_long_one_does() {
    // -h is a summary, not a subset. An option left out of it is an option a
    // reader has no way to learn exists, since the summary is what a reader
    // reaches for first and often only.
    let short = option_entries(&help_output("-h"));
    let long = option_entries(&help_output("--help"));

    let missing: Vec<_> =
        long.keys().filter(|name| !short.contains_key(*name)).collect();
    assert!(missing.is_empty(), "-h leaves out {missing:?}");
    let extra: Vec<_> =
        short.keys().filter(|name| !long.contains_key(*name)).collect();
    assert!(extra.is_empty(), "-h carries {extra:?}, which --help does not");
    assert!(short.len() > 20, "only {} options parsed from -h", short.len());
}

#[test]
fn every_summary_is_the_opening_of_the_help_it_summarises() {
    // The point of the split: one authored text at two depths, rather than
    // two texts. A hand-written line of one-liners would be a second thing
    // to keep true and the first to go stale -- which is this program's own
    // argument for generating its manual page instead of keeping one.
    //
    // So this asks the two rendered forms to agree, rather than working the
    // summary out again the way the program does, which would only ask
    // whether the program agrees with itself.
    let short = option_entries(&help_output("-h"));
    let long = option_entries(&help_output("--help"));

    // clap writes the help entry itself, and writes it as the one place the
    // two forms are meant to disagree: each points at the other. That is the
    // pointer this split needs, so it is asked for rather than skipped past.
    assert_eq!(short["help"], "Print help (see more with '--help')");
    assert_eq!(long["help"], "Print help (see a summary with '-h')");

    let mut shortened = 0;
    for (name, summary) in short.iter().filter(|(name, _)| *name != "help") {
        let full = without_clap_notes(&long[name]);
        let summary = without_clap_notes(summary);
        assert!(
            !summary.is_empty(),
            "--{name} has no summary in -h",
        );
        assert!(
            full.starts_with(&summary),
            "-h's --{name} is not how --help's begins:\n  -h    : {summary}\n\
             \x20 --help: {full}",
        );
        // A sentence, not a cut. Four entries end in a worked example --
        // '(e.g. -o1,-06:00)' -- whose full stop would halve the line and
        // take the syntax with it, which is the part a summary most needs.
        assert!(
            summary.ends_with('.') || summary == full,
            "--{name}'s summary does not end a sentence: {summary}",
        );
        // And a sentence rather than an abbreviation. The stop in 'e.g.' is
        // a stop like any other to the splitter, so a first sentence
        // carrying one outside brackets is cut at it -- and the cut passes
        // every check above, the one directly overhead included: it ends
        // with a full stop, it begins the long help, its brackets balance,
        // and it is shorter than what it summarises. What -h printed for
        // --remove was
        //
        //     --remove <num[,num...]>
        //         Remove a column, e.g.
        //
        // with the whole suite green. The four entries that carry an
        // abbreviation put it inside brackets, where the splitter does not
        // look, and that is what this makes a rule rather than a habit --
        // duration.txt and split.txt each carry one outside brackets today
        // and are unharmed only because it falls past their first sentence.
        for abbreviation in ["e.g.", "i.e.", "etc.", "cf.", "vs."] {
            assert!(
                !summary.ends_with(abbreviation) || summary == full,
                "--{name}'s summary is cut at '{abbreviation}': {summary}\n  \
                 put the abbreviation inside brackets, which is where the \
                 sentence splitter does not look for a full stop",
            );
        }
        assert_eq!(
            summary.matches('(').count(),
            summary.matches(')').count(),
            "--{name}'s summary was cut inside brackets: {summary}",
        );
        if summary.len() < full.len() {
            shortened += 1;
        }
    }

    // A prefix that is the whole string is still a prefix, so the rule above
    // holds perfectly well when nothing is summarised at all. Most entries
    // say more than their first sentence, and this is where that is asked:
    // a first draft counted lines instead and passed a build whose short
    // help carried every word of the long one, the two layouts differing
    // enough to keep the ratio looking right.
    assert!(
        shortened >= short.len() * 2 / 3,
        "only {shortened} of {} summaries are shorter than the help they \
         summarise",
        short.len(),
    );
}

#[test]
fn the_short_help_leaves_out_the_reference_material_and_says_where_it_went() {
    // What makes -h short is not shorter entries alone: it is that the four
    // reference sections and the second paragraph about durations belong to
    // someone reading rather than someone checking a flag.
    let short = help_output("-h");
    let long = help_output("--help");

    for heading in
        ["EXAMPLES", "TIMESTAMP FORMATS", "KNOWN LIMITS", "EXIT STATUS"]
    {
        assert!(
            long.contains(heading),
            "--help lost its {heading} section",
        );
        assert!(
            !short.contains(heading),
            "-h carries {heading}, which is not a summary",
        );
    }
    // The introduction stays; the paragraph explaining why a duration is
    // never given in months does not.
    assert!(short.contains("A command-line tool for parsing"));
    assert!(!short.contains("never years, months"));
    assert!(long.contains("never years, months"));

    // A summary that does not say it is one strands the reader who needed
    // the rest, so both forms end by saying where more is.
    assert!(
        short.contains("--help gives every option in full"),
        "-h does not say what --help adds",
    );
    for text in [&short, &long] {
        assert!(
            text.contains("https://github.com/YardQuit/csvdt"),
            "a help form names nowhere to go next",
        );
    }
    assert!(
        long.contains("DOCUMENTATION"),
        "--help lost the section naming the page and the repository",
    );
}

#[test]
fn neither_help_form_runs_past_the_column_the_corpus_is_written_to() {
    // clap fills a table to the terminal only with its wrap_help feature,
    // which this program does not take -- the corpus is folded by hand at 64
    // columns instead, so that what a reader sees is what was written and a
    // table stays a table. A summary is made rather than written, so nothing
    // folds it unless the program does: one entry ran a single line of -h out
    // to 211 columns, on a terminal that would then fold it wherever it liked.
    for flag in ["-h", "--help"] {
        let widest = help_output(flag)
            .lines()
            .map(|line| line.chars().count())
            .max()
            .expect("help should have lines");
        assert!(
            widest <= 90,
            "{flag} runs to {widest} columns",
        );
    }
}

#[test]
fn every_heading_in_the_help_is_shaped_like_the_others() {
    // Five headings had three shapes between them: TIMESTAMP FORMATS with no
    // blank line above it, EXAMPLES and DOCUMENTATION with none below. Each
    // reads fine alone, which is why it lasted -- the join between two
    // sections is only wrong next to a join done the other way.
    //
    // One shape for all of them: a blank line above, and none below, so a
    // heading sits against what it introduces and apart from what came
    // before. The five section headings were the odd ones out once the rest
    // were looked at -- the fifteen inside KNOWN LIMITS had always been
    // written the second way.
    //
    // Found rather than listed, so a heading written later is held to the
    // same shape without anyone remembering to add it here. A heading is a
    // line at the margin whose words are capitals once the option names
    // among them are set aside -- which is what separates 'A LEAP SECOND
    // ADDS NOTHING TO A SPAN' from the paragraph beginning 'Unix epoch
    // (read by -r/--rfc3339, which converts FROM this'. The blank line
    // above is asked about rather than assumed, so a heading that loses one
    // fails here instead of quietly ceasing to be found.
    let long = help_output("--help");
    let lines: Vec<&str> = long.lines().collect();

    let is_heading = |line: &str| {
        if line.is_empty() || line.starts_with(char::is_whitespace) {
            return false;
        }
        if line.ends_with('.') || line.ends_with(':') || line.ends_with(',') {
            return false;
        }
        let words: Vec<&str> = line
            .split_whitespace()
            // An option name keeps its own case, and parse_err is spelled
            // the way the output spells it.
            .filter(|word| !word.starts_with('-') && *word != "parse_err")
            .collect();
        // One word is enough: EXAMPLES and DOCUMENTATION are headings.
        !words.is_empty() && words.iter().all(|word| {
            word.chars().any(|c| c.is_ascii_uppercase())
                && !word.chars().any(|c| c.is_ascii_lowercase())
        })
    };

    let mut found = 0;
    for (at, line) in lines.iter().enumerate() {
        if !is_heading(line) {
            continue;
        }
        found += 1;
        assert!(at > 0, "'{line}' is the first line of the help");
        assert!(
            lines[at - 1].trim().is_empty(),
            "'{line}' has no blank line above it: {:?}",
            lines[at - 1],
        );
        assert!(
            lines.get(at + 1).is_some_and(|next| !next.trim().is_empty()),
            "'{line}' has a blank line below it, and no other heading does",
        );
    }

    // The finder has to have found them, or the loop above asserts nothing
    // and passes on a help with no headings left in it at all.
    assert!(
        found >= 20,
        "only {found} headings were found in the help, so this test is not \
         looking at what it thinks it is",
    );
}

#[test]
fn the_corpus_still_has_the_blank_line_the_two_forms_are_split_at() {
    // -h takes the introduction and --help goes on to the durations, which
    // is only possible while about.txt has a blank line between them. A
    // rewrap that closed it would leave both forms printing everything, with
    // nothing failing and nothing looking wrong.
    //
    // Asked the way first_paragraph asks it: a line that is empty once
    // trimmed ends the paragraph. That is what makes this true of the file
    // as checked out rather than of one set of line endings -- git hands a
    // Windows checkout CRLF, where a literal "\n\n" is nowhere in the file
    // and this failed every run on that leg while the program was fine.
    //
    // Trimmed rather than merely empty, again because first_paragraph
    // trims: a line of spaces splits the help there, so a test that missed
    // one would pass a corpus the program had already divided.
    let about = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/help/about.txt");
    let text = std::fs::read_to_string(about).expect("about.txt should be there");
    assert!(
        text.lines().any(|line| line.trim().is_empty()),
        "about.txt has no blank line, so -h and --help cannot differ",
    );
    assert!(!help_output("-h").contains("never years, months"));
    assert!(help_output("--help").contains("never years, months"));
}

#[test]
fn the_manual_page_takes_the_long_help_not_the_summary() {
    // clap_mangen prefers long_help and falls back to help, so a page built
    // from a Command whose long forms went missing would still render --
    // one sentence per option, valid roff, nothing to see. This asks for a
    // sentence that exists only past the first one.
    let page = String::from_utf8(
        csvdt().arg("--generate-man").output().unwrap().stdout,
    )
    .expect("the page should be UTF-8");
    assert!(
        page.contains("This can only be used combined with"),
        "the page is rendered from the summaries, not the help",
    );
    // And the section that tells a --help reader the page exists is not in
    // the page, which would be telling a reader where they already are.
    assert!(!page.contains("DOCUMENTATION"), "the page points at itself");
}

#[test]
fn no_part_of_the_help_runs_on_as_a_wall_of_text() {
    // Long help is read on a terminal, a screenful at a time, and prose
    // with nowhere to rest is skipped rather than read. EXAMPLES ran 47
    // lines with its eight commands packed against the prose between them,
    // TIMESTAMP FORMATS gave RFC3339 38 unbroken lines covering seven
    // separate things, and --peek and KNOWN LIMITS were as bad.
    //
    // The ceiling is a little above the longest block that survived the
    // breaking up, so it fails on a new wall rather than on the existing
    // text. What it cannot judge is whether a break falls in a sensible
    // place; it only says that one exists.
    const CEILING: usize = 30;

    let help = help_output("--help");
    let mut run = 0;
    let mut worst = (0, 0, String::new());
    for (number, line) in help.lines().enumerate() {
        if line.trim().is_empty() {
            run = 0;
            continue;
        }
        run += 1;
        if run > worst.0 {
            worst = (run, number + 2 - run, line.to_string());
        }
    }
    assert!(
        worst.0 <= CEILING,
        "{} lines of help run on without a blank line, from line {}:\n  {}",
        worst.0,
        worst.1,
        worst.2,
    );
}

#[test]
fn every_worked_example_stands_clear_of_the_prose_around_it() {
    // Each command in EXAMPLES is its own thing to try, and reads as one
    // only when the eye can find where it starts. They were written hard
    // against the sentence introducing them and against the next sentence
    // below, so eight examples came out as a single block.
    let help = help_output("--help");
    let lines: Vec<&str> = help.lines().collect();

    let mut unspaced = Vec::new();
    for (at, line) in lines.iter().enumerate() {
        if !line.trim_start().starts_with("$ csvdt")
            && !line.trim_start().starts_with("$ TZ=")
        {
            continue;
        }
        // The prose introducing an example ends with a blank line, so the
        // command block stands on its own.
        if at > 0 && !lines[at - 1].trim().is_empty() {
            unspaced.push(format!("{}: {}", at + 1, line.trim()));
        }
    }
    assert!(
        unspaced.is_empty(),
        "these examples are packed against the prose above them:\n{}",
        unspaced.join("\n"),
    );
}

#[test]
fn every_heading_reaches_the_manual_page_as_a_heading() {
    // The help gives a heading a blank line above and none below, so it sits
    // against what it introduces. --help writes the text out as it stands and
    // that reads as meant. roff fills, so a heading with nothing after it to
    // end the line is simply the first words of the paragraph below:
    //
    //     COLUMNS  ARE  ADDRESSED  BY  POSITION  Always  by number from 0
    //
    // Twelve of the fifteen under KNOWN LIMITS came out that way, leaving
    // that section one long paragraph with capitals scattered through it.
    //
    // Asked of the roff, so it holds without a groff on the machine: every
    // heading has to be a .SH or a .SS by the time the page is written, and
    // a heading left as plain text is one roff will run into its paragraph.
    let page = String::from_utf8(
        csvdt().arg("--generate-man").output().unwrap().stdout,
    )
    .expect("the page should be UTF-8");

    let mut plain = Vec::new();
    for line in page.lines() {
        if line.starts_with('.') || line.starts_with('\'') || line.is_empty() {
            continue;
        }
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        if line.ends_with('.') || line.ends_with(':') || line.ends_with(',') {
            continue;
        }
        // An apostrophe arrives as '\*(Aq', whose 'q' would answer for a
        // lowercase letter and rule out every heading holding one.
        let text = line.replace("\\*(Aq", "'");
        let mut words = text.split_whitespace().filter(|word| {
            let bare = word.strip_prefix("\\%").unwrap_or(word);
            !bare.starts_with("\\-") && !bare.starts_with('-')
                && *word != "parse_err"
        });
        let mut any = false;
        let capitals = words.all(|word| {
            any = true;
            word.chars().any(|c| c.is_ascii_uppercase())
                && !word.chars().any(|c| c.is_ascii_lowercase())
        });
        if any && capitals {
            plain.push(line.to_string());
        }
    }
    assert!(
        plain.is_empty(),
        "these headings reach the page as plain text, so roff fills them \
         into the paragraph below:\n{}",
        plain.join("\n"),
    );
}

#[test]
fn every_option_description_starts_on_its_own_line() {
    // .TP puts the description beside the tag when the tag is narrower than
    // the indent and below it when it is not. Every option is named widely
    // enough to go below except --peek, which alone read differently from
    // the rest of OPTIONS:
    //
    //     --peek Numbers  the  columns  and  shows  the top of the file
    //     --comment <char>
    //            The  comment  character  to use when parsing CSV.
    //
    // A break after every tag settles it for whatever is named next, short
    // or long, which is why this asks after all of them rather than after
    // the one that was wrong.
    let page = String::from_utf8(
        csvdt().arg("--generate-man").output().unwrap().stdout,
    )
    .expect("the page should be UTF-8");

    let lines: Vec<&str> = page.lines().collect();
    let mut unbroken = Vec::new();
    let mut tags = 0;
    for (at, line) in lines.iter().enumerate() {
        if *line != ".TP" {
            continue;
        }
        // The tag is the line after the request, and the break the line
        // after that.
        match (lines.get(at + 1), lines.get(at + 2)) {
            (Some(tag), Some(next)) if !tag.starts_with('.') => {
                tags += 1;
                if *next != ".br" {
                    unbroken.push(format!("{}: {tag}", at + 2));
                }
            }
            _ => {}
        }
    }
    assert!(
        tags >= 20,
        "only {tags} tagged paragraphs were found, so this is not looking \
         at the options",
    );
    assert!(
        unbroken.is_empty(),
        "these tags have no break after them, so a short one keeps its \
         description on its own line:\n{}",
        unbroken.join("\n"),
    );
}

#[test]
fn the_help_names_a_character_the_way_a_reader_writes_one() {
    // Seven places spelled a character as Rust spells a byte -- b'#' for
    // the comment character, b',' for the delimiter, b'"' for the quote --
    // which is the language the program is written in showing through the
    // text a user reads. Nothing else in the help is written that way; the
    // corpus quotes a literal plainly, '-H' and '-p' and the rest.
    for form in ["--help", "-h"] {
        let help = help_output(form);
        assert!(
            !help.contains("b'"),
            "{form} spells a character as a Rust byte literal",
        );
    }
}

#[test]
fn no_option_name_in_the_manual_page_can_be_split_across_lines() {
    // roff hyphenates to fill a line, and it has no idea that --duration is
    // one word rather than English. At 80 columns two names came out broken
    // -- '--du-' at the end of a line and 'ration).' at the start of the
    // next -- which names an option that does not exist and cannot be
    // pasted. '\%' opening the word is what stops it.
    //
    // Checked in the roff rather than the rendering, so it holds without a
    // groff on the machine, and so it fails on the name that has just been
    // written unprotected rather than only on the one that happens to land
    // on a line boundary at the width the test rendered at.
    let page = String::from_utf8(
        csvdt().arg("--generate-man").output().unwrap().stdout,
    )
    .expect("the page should be UTF-8");

    const FONT_MACROS: [&str; 8] =
        [".B", ".I", ".BR", ".IR", ".RB", ".RI", ".BI", ".IB"];
    let mut unprotected = Vec::new();
    for (number, line) in page.lines().enumerate() {
        // The argument of a font macro is set in the surrounding paragraph
        // and breaks with it, so it is looked at the same way; every other
        // request is roff's own business.
        let text = if line.starts_with('.') || line.starts_with('\'') {
            match line.split_once(' ') {
                Some((name, argument)) if FONT_MACROS.contains(&name) => {
                    argument
                }
                _ => continue,
            }
        } else {
            line
        };
        for word in text.split_whitespace() {
            // An option name reaches the roff with its hyphens escaped, so
            // a word opening '\-' and carrying a name after it is one. A
            // word opening '\%\-' is one that has been protected, which is
            // what should be there. A bare '\-' is the dash NAME is written
            // with, which is not a name and has nothing to break.
            let names_option = word.strip_prefix("\\-").is_some_and(|rest| {
                rest.starts_with("\\-")
                    || rest.starts_with(|c: char| c.is_ascii_alphanumeric())
            });
            if names_option {
                unprotected.push(format!("{}: {line}", number + 1));
                break;
            }
        }
    }
    assert!(
        unprotected.is_empty(),
        "these option names may be hyphenated across a line break:\n{}",
        unprotected.join("\n"),
    );
}

#[test]
fn the_manual_page_carries_the_sections_help_cannot() {
    // Everything above these is generated from the Command that parses a
    // command line, which is what keeps the page from describing options
    // this build does not have -- and is why there is nowhere in it for what
    // belongs to the program rather than to one of its options. GNU
    // coreutils merges a per-page man/prog.x for the same reason; this is
    // csvdt's one file of it, and this asks that it arrived.
    let page = String::from_utf8(
        csvdt().arg("--generate-man").output().unwrap().stdout,
    )
    .expect("the page should be UTF-8");

    for section in ["ENVIRONMENT", "FILES", "NOTES", "SEE ALSO"] {
        assert!(
            page.contains(&format!(".SH {section}")),
            "the manual page has no {section} section",
        );
    }
    // Before the two clap_mangen puts last, so the hand-written sections sit
    // where a reader expects them rather than after the author's address.
    let environment = page.find(".SH ENVIRONMENT").expect("ENVIRONMENT");
    let version = page.find(".SH VERSION").expect("VERSION");
    assert!(
        environment < version,
        "the hand-written sections come after VERSION and AUTHORS",
    );
}

#[test]
fn what_the_manual_page_says_about_the_environment_is_what_csvdt_does() {
    // A hand-written section is the one part of the page that can drift, so
    // each claim in it is asked of the binary. Both variables are named
    // because the message about a temporary file has to name the one a
    // reader is meant to set, and the two families spell it differently.
    let page = String::from_utf8(
        csvdt().arg("--generate-man").output().unwrap().stdout,
    )
    .expect("the page should be UTF-8");
    let said = |text: &str| {
        assert!(page.contains(text), "the page no longer says {text:?}");
    };
    said("TZ");
    said("TMPDIR");
    said("csvdt\\-widest\\-<pid>.csv");

    // The zone a name does not name is refused rather than answered.
    csvdt()
        .args(["-H", "-p", "-l0"])
        .env("TZ", "Europe/Nowhere")
        .write_stdin("when\n2026-01-01T00:00:00Z\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a zone"));

    // And the variable the page names is the one the message names, which is
    // the whole reason the section names it. Set through with_temp_dir rather
    // than by hand: TMPDIR alone moves nothing on Windows, so the run would
    // spool to the real system directory and succeed -- which is exactly the
    // difference this half of the test exists to pin.
    let temporary = tempfile_dir().join("man-environment");
    std::fs::create_dir_all(&temporary).unwrap();
    with_temp_dir(csvdt(), &temporary.join("nowhere-at-all"))
        .args(["-f=widest"])
        .write_stdin("a,b\n1,2,3\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains(if cfg!(windows) {
            "TMP or TEMP"
        } else {
            "TMPDIR"
        }));
}

#[test]
fn what_the_manual_page_says_about_files_is_what_csvdt_does() {
    // FILES is hand-written, and hand-written drifts. It called the widest
    // spool "the only file csvdt creates" for a whole release after
    // --fill-log made that false, and the only test touching the section
    // asked that its heading was present.
    //
    // So the section is enumerated rather than read. --list-options reports
    // each option's value name, and a destination csvdt opens because it was
    // told to is spelled 'file', so an option added later that takes one has
    // to be written down there as well or this fails. The spool is asked for
    // by name instead, since no option names it.
    let page = String::from_utf8(
        csvdt().arg("--generate-man").output().unwrap().stdout,
    )
    .expect("the page should be UTF-8");

    let start = page.find(".SH FILES").expect("the page has no FILES section");
    let end = page.find(".SH NOTES").expect("the page has no NOTES section");
    assert!(start < end, "FILES comes after NOTES");
    let files = &page[start..end];

    let listed = String::from_utf8(
        csvdt().arg("--list-options").output().unwrap().stdout,
    )
    .expect("the option list should be UTF-8");

    let mut asked = 0;
    for line in listed.lines().filter(|line| !line.starts_with('#')) {
        let column: Vec<&str> = line.split('\t').collect();
        if column.get(4) != Some(&"file") {
            continue;
        }
        let long = column[1];
        // roff escapes a leading hyphen, so the page spells --fill-log as
        // \-\-fill\-log. Matching the page's own spelling rather than
        // normalising it keeps this test reading what a reader reads.
        let spelled = long.replace('-', "\\-");
        assert!(
            files.contains(&spelled),
            "--list-options says {long} takes a file and FILES does not name it:\n{files}",
        );
        asked += 1;
    }
    assert!(
        asked > 0,
        "no option reports a value of 'file', so this test asked nothing of FILES",
    );

    assert!(
        files.contains("csvdt\\-widest"),
        "FILES no longer names the spool, which no option names for it:\n{files}",
    );

    // And the claim that is behaviour rather than a name: an existing
    // destination is replaced, so a run's log is that run's and no other's.
    let directory = tempfile_dir().join("man-files");
    std::fs::create_dir_all(&directory).unwrap();
    let log = directory.join("fill.log");
    std::fs::write(&log, "STALE,FROM,AN,EARLIER,RUN\n").unwrap();
    csvdt()
        .args(["-H", "-p", "--flexible", "--fill-log"])
        .arg(&log)
        .write_stdin("a,b,c\n1,2\n")
        .assert()
        .success();
    let written = std::fs::read_to_string(&log).unwrap();
    assert!(
        !written.contains("STALE"),
        "an existing --fill-log destination was added to rather than replaced:\n{written}",
    );
    assert!(
        written.starts_with("line,record,"),
        "the replaced log does not begin with the header a log begins with:\n{written}",
    );
}

#[test]
fn the_manual_page_is_ascii() {
    // roff is not told the input encoding, so a byte above 7-bit is read as
    // whatever the reader's locale says and warned about either way:
    //
    //   troff:csvdt.1:374: warning: invalid input character code 128
    //
    // One typographic apostrophe in one help file did that, and 'aren't' came
    // out of man as 'arenat'. roff has escapes for the characters worth
    // having -- \(aq, \(oq, \(cq, \(em -- so the help text stays ASCII and
    // anything beyond it is written as an escape rather than a raw byte.
    let page = String::from_utf8(
        csvdt().arg("--generate-man").output().unwrap().stdout,
    )
    .expect("the page should be UTF-8");

    let offenders: Vec<_> = page
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.is_ascii())
        .map(|(number, line)| format!("  line {}: {line}", number + 1))
        .collect();
    assert!(
        offenders.is_empty(),
        "the manual page carries bytes roff cannot read as written:\n{}",
        offenders.join("\n"),
    );
}

#[test]
fn the_manual_page_keeps_the_line_breaks_the_help_was_written_with() {
    // roff fills: it joins the lines of a paragraph and rewraps them. Applied
    // to a value table that is what it did to '--quote',
    //
    //   The quoting style to use when writing CSV.  always     This puts
    //   quotes  around every field. Always.  necessary  This puts quotes
    //   around fields only when necessary.
    //              They are necessary when fields contain a  quote,  de-
    //   limiter or record
    //
    // which is not a table, a paragraph, or readable -- and every section
    // check passed over it, because the headings were all present.
    //
    // So a run of help lines with any indented line among them is laid out on
    // purpose and gets a .br between its lines. Every table is asked, not
    // just --quote: they were laid out one at a time and are the kind of
    // thing that gets laid out one at a time again.
    let page = String::from_utf8(
        csvdt().arg("--generate-man").output().unwrap().stdout,
    )
    .expect("the page should be UTF-8");

    for tag in [
        "-f, --flexible", "-f=fix, --flexible=fix",
        "always", "necessary", "never", "nonnumeric",
        "none", "fields", "headers", "all",
        "value", "separator", "value-name",
    ] {
        // Each on a line of its own with its description nested under it:
        // that is what the tables were re-laid as, and what roff must not
        // undo. Compared with the escapes undone, since a page spells a
        // hyphen '\-' so that roff does not read it as one it may break at,
        // and opens an option name '\%' so that roff does not break the name
        // itself. Neither sets anything -- both are instructions about
        // breaking -- and this is a question about layout.
        //
        // The nesting is a '.RS' region rather than an indent written into
        // the text: the tag sits in one and its description in another, so
        // the description fills to the width the page is read at instead of
        // freezing at the width the help file is wrapped to. What this asks
        // is unchanged -- that the tag stands over its description and was
        // not filled into a paragraph beside it.
        //
        // The region is also what tells this table from the list of values
        // clap writes for the same option, which is '.RS 14' and a bullet
        // per value. Asked of the tag alone, flattening --trim's table back
        // to two columns still passed: 'none' appears on a line of its own
        // further down the page, among clap's values, and that line answered
        // for a table it has nothing to do with.
        let lines: Vec<&str> = page.lines().collect();
        let nests = |line: &str| line.starts_with(".RS ") && line.ends_with('n');
        let laid_out = lines.iter().enumerate().any(|(at, line)| {
            line.trim().replace("\\%", "").replace("\\-", "-") == tag
                && at > 0
                && nests(lines[at - 1])
                && {
                    let mut rest = lines
                        .iter()
                        .skip(at + 1)
                        .skip_while(|next| **next == ".br" || **next == ".RE");
                    rest.next().is_some_and(|next| nests(next))
                        && rest.next().is_some_and(|body| {
                            !body.is_empty() && !body.starts_with('.')
                        })
                }
        });
        assert!(
            laid_out,
            "'{tag}' does not stand over its description in the page, so its \
             table was filled into a paragraph",
        );
    }
}

// The text compiled into the binary is LF, whatever platform checked it out.
//
// src/help and src/man are pulled in with include_str!, so a checkout's line
// endings are the bytes --help prints and the page is rendered from. Windows
// checks text out with CRLF unless .gitattributes says otherwise, and the day
// it did, a blank line inside an option's long help was ten spaces and a
// carriage return -- which the trim that takes trailing padding off walked
// straight past, leaving 59 lines of whitespace in --help on windows-latest
// and none anywhere else.
//
// This is that root cause rather than its symptom: it fails on a checkout
// that brought CRLF in, whether or not anything downstream copes.
#[test]
fn the_text_compiled_into_the_binary_has_no_carriage_returns() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut read = 0;
    for corner in ["src/help", "src/man"] {
        let directory = root.join(corner);
        for entry in std::fs::read_dir(&directory)
            .unwrap_or_else(|why| panic!("{corner} should be readable: {why}"))
        {
            let path = entry.expect("each entry should be readable").path();
            if path.extension().is_none_or(|kind| kind != "txt" && kind != "roff") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("it should be UTF-8");
            read += 1;
            let at = text.match_indices('\r').next().map(|(at, _)| at);
            assert!(
                at.is_none(),
                "{} holds a carriage return at byte {}, so this checkout will \
                 build a --help and a manual page that differ from every other \
                 platform's. .gitattributes is meant to stop that.",
                path.display(),
                at.unwrap_or_default(),
            );
        }
    }
    assert!(read > 30, "only {read} files were read, so this proves little");
}

// Nothing csvdt says about itself ends a line with a space.
//
// clap indents every line of an option's long help by ten columns, blank
// lines included, so a blank line inside one arrived as ten spaces and a
// newline: 83 lines of --help were whitespace and nothing else. Invisible on
// a terminal, and litter in a file, a diff or a paste.
//
// --list-options is exempt from the trailing-space half, and only that half.
// Its format is tab-separated fields and the last of them is empty for an
// option that takes no value, so those lines end in a tab because the field
// is there and empty -- a wrapper splitting on tabs counts on it. It is still
// held to having no line that is whitespace alone.
#[test]
fn what_csvdt_says_about_itself_has_no_trailing_whitespace() {
    let printed = |argument: &str| {
        String::from_utf8(csvdt().arg(argument).output().unwrap().stdout)
            .expect("csvdt should print UTF-8")
    };

    for argument in ["--help", "-h", "--version", "--generate-man"] {
        let text = printed(argument);
        let trailing: Vec<usize> = text
            .lines()
            .enumerate()
            .filter(|(_, line)| line.ends_with(' ') || line.ends_with('\t'))
            .map(|(at, _)| at + 1)
            .collect();
        assert!(
            trailing.is_empty(),
            "{argument} ends {} line(s) with whitespace, first at line {}",
            trailing.len(),
            trailing[0],
        );
    }

    for argument in ["--help", "-h", "--version", "--list-options", "--generate-man"] {
        let text = printed(argument);
        let blank: Vec<usize> = text
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.is_empty() && line.trim().is_empty())
            .map(|(at, _)| at + 1)
            .collect();
        assert!(
            blank.is_empty(),
            "{argument} has {} line(s) of whitespace and nothing else, first at \
             line {} -- a blank line should be blank",
            blank.len(),
            blank[0],
        );
    }
}

// The verbatim gutter marks a block in the help files as text laid out on
// purpose -- a command to copy, a sample of output, a diagram whose columns
// line up. It decides what the page holds as written and what it lets roff
// fill, and it is meant for the renderer, never for a reader.
//
// Two things have to hold, and they pull against each other. The gutter must
// reach none of the three things csvdt prints, and every line it marks must
// reach the page exactly as it was written -- so a strip that took too much,
// or a pass that filled a marked block, fails here.
#[test]
fn the_verbatim_gutter_marks_the_page_and_never_shows_in_it() {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/help");
    let mut marked = Vec::new();
    let mut held = Vec::new();
    for entry in std::fs::read_dir(&corpus).expect("the help corpus should be readable") {
        let path = entry.expect("each entry should be readable").path();
        if path.extension().is_none_or(|kind| kind != "txt") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a help file should be UTF-8");
        let name =
            path.file_name().expect("a file has a name").to_string_lossy().to_string();
        let lines: Vec<&str> = text.lines().collect();
        for (at, line) in lines.iter().enumerate() {
            if let Some(rest) = line.strip_prefix('|') {
                marked.push((name.clone(), rest.trim().to_string()));
                // Two marked lines in a row are a block, and a block is the
                // thing that must not be filled: the page has to keep a
                // break between them or roff runs the second onto the first.
                if let Some(next) = lines.get(at + 1).and_then(|l| l.strip_prefix('|')) {
                    held.push((name.clone(), rest.trim().to_string(),
                               next.trim().to_string()));
                }
            }
        }
    }
    assert!(
        !marked.is_empty(),
        "nothing in the help corpus is marked verbatim, so this test proves nothing",
    );

    let printed = |argument: &str| {
        String::from_utf8(csvdt().arg(argument).output().unwrap().stdout)
            .expect("csvdt should print UTF-8")
    };
    let page = printed("--generate-man");
    for (what, text) in [("the page", &page), ("--help", &printed("--help")),
                         ("-h", &printed("-h"))] {
        let leaked = text.lines().filter(|line| line.starts_with('|')).count();
        assert_eq!(
            leaked, 0,
            "{what} shows {leaked} verbatim gutter(s), which are marks for the \
             renderer and not text anyone should read",
        );
    }

    // The page spells a hyphen '\-' so roff does not read it as one it may
    // break at, opens an option name '\%' so roff does not break the name
    // itself, and writes an apostrophe '\*(Aq' because a bare one starts a
    // roff request. None of them set anything, so they come off before the
    // line is compared with the one it was written as.
    let plain: Vec<String> = page
        .lines()
        .map(|line| {
            line.replace("\\%", "")
                .replace("\\-", "-")
                .replace("\\*(Aq", "'")
                .replace("\\e", "\\")
                .trim()
                .to_string()
        })
        .collect();
    for (file, line) in &marked {
        assert!(
            plain.iter().any(|rendered| rendered == line),
            "{file} marks '{line}' as verbatim, but the page does not carry it \
             as written -- it was filled, or the strip took too much",
        );
    }

    // Carrying the words is not enough: a marked block is marked so that the
    // reader gets the lines as lines. Without the break between them roff
    // fills the second onto the end of the first, and two shell commands
    // become one that runs neither.
    assert!(!held.is_empty(), "no marked block in the corpus is more than one line");
    for (file, first, second) in &held {
        let kept = plain
            .windows(3)
            .any(|around| around[0] == *first && around[1] == ".br" && around[2] == *second);
        assert!(
            kept,
            "{file} marks '{first}' and '{second}' as one verbatim block, but the \
             page does not keep a break between them, so roff will fill the \
             second onto the first",
        );
    }
}

#[test]
fn every_verbatim_block_stands_clear_of_the_prose_around_it() {
    // Seven marked blocks were once written hard against the sentence above
    // or below, which reads as cramped in the page and in
    // --help alike and makes the same kind of block look like two kinds
    // depending on which file it is in. They were spaced alike, and the
    // corpus README says to keep them that way -- but only one half of one
    // file was ever asked.
    //
    // every_worked_example_stands_clear_of_the_prose_around_it asks the
    // rendered --help about lines beginning '$ csvdt', and only about the
    // blank line above them, which is the right question for the line it
    // finds: a command is followed by its own sample output, not by a blank.
    // The block is the thing with two sides, so the block is what this asks
    // about -- and it asks it of every marked block, not only the ones that
    // happen to be commands. Taking the blank line off the bottom of an
    // EXAMPLES block, or off the top of the sample timestamps in formats.txt,
    // left the whole suite green.
    //
    // Asked of the source. The corpus is where the spacing is decided, the
    // gutter is still on the line there, and a rendered answer would need a
    // groff the test machine need not have.
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/help");

    let mut blocks = 0;
    let mut crowded = Vec::new();
    for entry in std::fs::read_dir(&corpus).expect("the help corpus should be readable") {
        let path = entry.expect("each entry should be readable").path();
        if path.extension().is_none_or(|kind| kind != "txt") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a help file should be UTF-8");
        let name = path.file_name().expect("a file has a name").to_string_lossy().to_string();
        let lines: Vec<&str> = text.lines().collect();

        let mut at = 0;
        while at < lines.len() {
            if !lines[at].starts_with('|') {
                at += 1;
                continue;
            }
            let start = at;
            while at < lines.len() && lines[at].starts_with('|') {
                at += 1;
            }
            let end = at - 1;
            blocks += 1;
            // The start and end of a file count as clear: a block opening a
            // file has nothing above it to be crowded by, and the section it
            // is poured into supplies the break.
            if start > 0 && !lines[start - 1].trim().is_empty() {
                crowded.push(format!(
                    "  {name}:{} opens a marked block with '{}' hard above it",
                    start + 1,
                    lines[start - 1].trim(),
                ));
            }
            if end + 1 < lines.len() && !lines[end + 1].trim().is_empty() {
                crowded.push(format!(
                    "  {name}:{} closes a marked block with '{}' hard below it",
                    end + 1,
                    lines[end + 1].trim(),
                ));
            }
        }
    }

    assert!(
        blocks > 1,
        "only {blocks} marked block(s) found in the corpus, so this test proves nothing",
    );
    assert!(
        crowded.is_empty(),
        "these marked blocks are packed against the prose around them:\n{}",
        crowded.join("\n"),
    );
}

#[test]
fn every_shell_command_in_the_corpus_is_held_verbatim() {
    // The corpus cannot say what an author meant, so a block that should
    // have carried the gutter and did not reflows quietly. For most blocks
    // that is the end of it: a table is held anyway by its column gaps, and
    // everything else is prose that ought to fill. One kind is neither. A
    // line whose first non-space is '$ ' is a command a reader is meant to
    // copy, and roff filling one produces the very thing the gutter exists
    // to prevent -- the corpus README carries the specimen:
    //
    //     $       csvdt       -H       -p        --peek        sales.csv
    //
    // That class can be asked for rather than left to a rendering by eye,
    // which is what the README used to say was the only way to catch it.
    //
    // Asked of the source, like the other corpus tests: the gutter is still
    // on the line there, and a rendered answer would want a groff the test
    // machine need not have.
    let corpus =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/help");

    let mut commands = 0;
    let mut loose = Vec::new();
    for entry in std::fs::read_dir(&corpus).expect("the help corpus should be readable") {
        let path = entry.expect("each entry should be readable").path();
        if path.extension().is_none_or(|kind| kind != "txt") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a help file should be UTF-8");
        let name = path.file_name().expect("a file has a name").to_string_lossy().to_string();

        for (number, line) in text.lines().enumerate() {
            let marked = line.starts_with('|');
            let body = if marked { &line[1..] } else { line };
            // Indented, and '$' followed by a space: a prompt is written in
            // a block, and neither a bare '$' nor '$TMPDIR' is a command.
            if !body.starts_with(' ') || !body.trim_start().starts_with("$ ") {
                continue;
            }
            commands += 1;
            if !marked {
                loose.push(format!("  {name}:{}: {}", number + 1, line.trim()));
            }
        }
    }

    assert!(
        commands > 1,
        "only {commands} shell command(s) found in the corpus, so this test asked nothing",
    );
    assert!(
        loose.is_empty(),
        "these commands carry no verbatim gutter, so roff will fill them:\n{}",
        loose.join("\n"),
    );
}

#[test]
fn the_value_tables_were_not_rewrapped_into_prose() {
    // Rewrapping the corpus to fit the manual page's column treated two
    // tables as prose and reflowed them, which is how --trim came to read
    //
    //   parsing. none    Preserves fields and headers. fields  Trim
    //   whitespace from fields, but not headers. headers Trim whitespace
    //
    // in --help itself -- the one place the table had still been right. The
    // --list-options table lost a row the same way: 'separator' ended up
    // inside the description of 'value', so the format's five fields were
    // documented as four.
    //
    // A row name that no longer stands alone is the signature of it, so that
    // is what this asks. It is a property of the source, and the source is
    // where a rewrap goes wrong.
    let help = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/help");

    for (file, names) in [
        ("trim.txt", ["none", "fields", "headers", "all"].as_slice()),
        (
            "list_options.txt",
            ["value", "separator", "value-name"].as_slice(),
        ),
        (
            "quotes.txt",
            ["always", "necessary", "never", "nonnumeric"].as_slice(),
        ),
        ("exit_status.txt", ["0", "1", "2", "100"].as_slice()),
    ] {
        let text = std::fs::read_to_string(help.join(file)).unwrap();
        for name in names {
            assert!(
                text.lines().any(|line| line.trim() == *name),
                "'{name}' is not a line of its own in {file}, so its row has \
                 been rewrapped into the prose around it",
            );
        }
    }
}

#[test]
fn every_known_limit_still_has_a_heading_to_find_it_by() {
    // KNOWN LIMITS is the longest thing in the help, and the only way into
    // it is its section headings. Rewrapping the corpus treated them as
    // ordinary words and pulled each one onto the end of the paragraph
    // before it, so all fifteen disappeared at once:
    //
    //   ...only in the help for the option each one belongs to. INPUT MUST
    //   BE UTF-8 A byte-order mark is stripped if present. Anything else...
    //
    // Nothing said a word: every section was still present, still in order,
    // still saying the same thing, and unfindable in --help and in man
    // alike. The headings are listed here so that a wrap that swallows one
    // has to say so.
    const HEADINGS: [&str; 15] = [
        "INPUT MUST BE UTF-8",
        "THE INPUT FILE CANNOT ALSO BE THE OUTPUT",
        "COLUMNS ARE ADDRESSED BY POSITION",
        "-l/--local DEPENDS ON MORE THAN THE INPUT",
        "-l OUTPUT IS NOT SORTABLE AS TEXT",
        "-l WRITES parse_err WHERE THE ZONE'S OFFSET IS NOT WHOLE MINUTES",
        "-o/--offset ACCEPTS ONLY REAL OFFSETS",
        "AN UNCLOSED QUOTE IS NOT REPORTED AS ONE",
        "A CR-ONLY FILE LOSES ROWS TO --comment",
        "THE OUTPUT IS CANONICAL CSV, NOT A COPY OF THE INPUT",
        "A VALUE THAT WILL NOT PARSE IS MARKED, NOT FATAL",
        "-d/--duration WITH ONE COLUMN TRUSTS THE ROW ORDER",
        "A LEAP SECOND ADDS NOTHING TO A SPAN",
        "--flexible\'s FILLING MODES ARE THE ONLY OPTIONS THAT ADD DATA",
        "ROWS ARE PROCESSED ONE AT A TIME",
    ];
    let limits = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/help/limits.txt"),
    )
    .expect("KNOWN LIMITS should be there");

    for heading in HEADINGS {
        assert!(
            limits.lines().any(|line| line.trim_end() == heading),
            "'{heading}' is no longer a line of its own in KNOWN LIMITS, so \
             the section it names cannot be found by eye or by search",
        );
    }
}

#[test]
fn version_carries_build_metadata_only_when_stamped() {
    // A published build carries semver build metadata naming the exact build
    // -- "1.3.0+2026.08.a1b2c3d" -- set through CSVDT_BUILD_METADATA by the
    // workflow that publishes it. A build made by hand carries none, so it
    // reads plainly and cannot be mistaken for something published.
    //
    // This asked for "csvdt " and "IANA tzdata" and nothing else, which is
    // what version_reports_the_bundled_iana_database_release asks two
    // hundred lines up. It said nothing about either half of the rule it is
    // named for, and passed whether or not the stamp arrived.
    //
    // The test crate is compiled by the same cargo invocation as the binary
    // and sees the same environment, which is what lets it ask. Both halves
    // are exercised by runs that already happen: unstamped by every local
    // run and every pull request, stamped by the monthly build and by the
    // RPM build, each of which sets the variable for the whole job with
    // cargo test inside it.
    let output = csvdt().arg("--version").output().expect("--version should run");
    assert!(output.status.success(), "--version should succeed");
    let printed = String::from_utf8(output.stdout).expect("--version should be UTF-8");

    // "csvdt <version> (IANA tzdata <release>)"
    let version = printed
        .split_whitespace()
        .nth(1)
        .expect("--version should print a version after the program name");

    match option_env!("CSVDT_BUILD_METADATA") {
        Some(stamp) => assert_eq!(
            version.split_once('+').map(|(_, after)| after),
            Some(stamp),
            "this build was stamped '{stamp}' and --version says '{version}'",
        ),
        None => assert!(
            !version.contains('+'),
            "nothing stamped this build and --version carries metadata anyway: {version}",
        ),
    }
}

// ---- parsing leniency and structural robustness --------------------------

#[test]
fn lenient_parsing_accepts_lowercase_t_and_z() {
    // RFC3339 defines the 'T' and 'Z' as case-insensitive, and exports in the
    // wild use both spellings.
    csvdt()
        .args(["-H", "-p", "-u0", "-i", "0,replace"])
        .write_stdin(
            "ts\n\
             2024-01-01T00:00:00Z\n\
             2024-01-01t00:00:00z\n\
             2024-01-01T00:00:00z\n\
             2024-01-01t00:00:00Z\n",
        )
        .assert()
        .success()
        .stdout(
            "to_utc\n\
             2024-01-01T00:00:00+00:00\n\
             2024-01-01T00:00:00+00:00\n\
             2024-01-01T00:00:00+00:00\n\
             2024-01-01T00:00:00+00:00\n",
        );
}

#[test]
fn z_and_an_explicit_zero_offset_are_the_same_instant() {
    csvdt()
        .args(["-H", "-p", "-d0,1", "-i", "0,replace"])
        .write_stdin("a,b\n2024-01-01T00:00:00Z,2024-01-01T00:00:00+00:00\n")
        .assert()
        .success()
        .stdout("row_duration,b\nPT0S,2024-01-01T00:00:00+00:00\n");
}

#[test]
fn duration_cache_survives_consecutive_parse_errors() {
    // A run of unparseable rows must not advance or reset the cache: the next
    // good row still measures back to the last good timestamp.
    csvdt()
        .args(["-H", "-p", "-d0", "-i", "0,replace"])
        .write_stdin(
            "ts\n\
             2024-01-01T00:00:00Z\n\
             bad\n\
             also-bad\n\
             2024-01-04T00:00:00Z\n\
             bad-again\n\
             2024-01-05T00:00:00Z\n",
        )
        .assert()
        .success()
        .stdout(
            "duration\n\
             PT0S\n\
             parse_err\n\
             parse_err\n\
             P3D\n\
             parse_err\n\
             P1D\n",
        );
}

#[test]
fn flexible_handles_rows_wider_and_narrower_than_the_header() {
    // With -f each row is treated on its own width, so the action still lands
    // relative to that row rather than to the header.
    csvdt()
        .args(["-H", "-p", "-f", "-u0"])
        .write_stdin("a,b\n2024-01-01T00:00:00Z,x,y,z\n")
        .assert()
        .success()
        .stdout("a,<to_utc,b\n2024-01-01T00:00:00Z,2024-01-01T00:00:00+00:00,x,y,z\n");

    csvdt()
        .args(["-H", "-p", "-f", "-u0"])
        .write_stdin("a,b,c,d\n2024-01-01T00:00:00Z,x\n")
        .assert()
        .success()
        .stdout("a,<to_utc,b,c,d\n2024-01-01T00:00:00Z,2024-01-01T00:00:00+00:00,x\n");
}

#[test]
fn output_quotes_a_written_value_containing_the_print_delimiter() {
    // -s writes a time like "10:30:00 +00:00", which contains a space. With a
    // space as the output delimiter that value has to be quoted or the row
    // silently gains a field.
    csvdt()
        .args(["-H", "-p", "-s1", "--print-delimiter", " "])
        .write_stdin("a,ts\n1,2024-01-01T10:30:00Z\n")
        .assert()
        .success()
        .stdout(
            "a ts <split_date <split_time\n\
             1 2024-01-01T10:30:00Z 2024-01-01 \"10:30:00 +00:00\"\n",
        );
}

#[test]
fn rfc3339_accepts_signed_and_padded_epoch_values() {
    csvdt()
        .args(["-H", "-p", "-r0", "-i", "0,replace"])
        .write_stdin("e\n+1700000000\n-1700000000\n 1700000000 \n")
        .assert()
        .success()
        .stdout(
            "to_rfc3339\n\
             2023-11-14T22:13:20+00:00\n\
             1916-02-18T01:46:40+00:00\n\
             2023-11-14T22:13:20+00:00\n",
        );
}

#[test]
fn out_of_range_column_numbers_are_clean_errors_not_panics() {
    // Column numbers are parsed as usize and -i's as i64; values that cannot
    // fit, or are not numbers at all, must be reported rather than panic.
    for arg in [
        "-u-1",
        "-uabc",
        "-u99999999999999999999999999",
        "-u18446744073709551616",
    ] {
        csvdt()
            .args(["-H", "-p", arg])
            .write_stdin("a,b\n1,2\n")
            .assert()
            .failure()
            .code(100);
    }

    // -i saturates rather than failing: these are in range for i64 and clamp.
    for arg in ["-i9223372036854775807,after", "-i-9223372036854775808,before"] {
        csvdt()
            .args(["-H", "-p", "-u0", arg])
            .write_stdin("a,b\n2024-01-01T00:00:00Z,2\n")
            .assert()
            .success();
    }
}

#[test]
fn offset_accepts_the_real_world_extremes() {
    // -12:00 (Baker Island) and +14:00 (the Line Islands) are the outermost
    // offsets any place actually uses, so both must work.
    for (target, expected) in [
        ("+14:00", "2024-01-02T02:00:00+14:00"),
        ("-12:00", "2024-01-01T00:00:00-12:00"),
        ("+1400", "2024-01-02T02:00:00+14:00"),
        ("-1200", "2024-01-01T00:00:00-12:00"),
    ] {
        csvdt()
            .args(["-H", "-p", &format!("-o0,{}", target), "-i", "0,replace"])
            .write_stdin("ts\n2024-01-01T12:00:00Z\n")
            .assert()
            .success()
            .stdout(format!("to_offset\n{}\n", expected));
    }
}

#[test]
fn offset_rejects_a_target_no_real_place_uses() {
    // chrono would accept anything up to +/-23:59, so without a range check a
    // typo like +15:00 for +05:00 would be honoured and produce a plausible
    // timestamp carrying an impossible offset.
    for target in ["+14:01", "+15:00", "+23:59", "-12:01", "-13:00", "+1500"] {
        csvdt()
            .args(["-H", "-p", &format!("-o0,{}", target), "-i", "0,replace"])
            .write_stdin("ts\n2024-01-01T12:00:00Z\n")
            .assert()
            .failure()
            .code(100)
            .stderr(predicate::str::contains("not a real UTC offset"));
    }
}

#[test]
fn offset_range_check_does_not_reject_sub_hour_targets() {
    // The guard is about magnitude, not granularity: 30- and 45-minute
    // offsets are real and must still be accepted.
    for target in ["+05:30", "+05:45", "+12:45", "-09:30", "+10:30", "-03:30"] {
        csvdt()
            .args(["-H", "-p", &format!("-o0,{}", target), "-i", "0,replace"])
            .write_stdin("ts\n2024-01-01T12:00:00Z\n")
            .assert()
            .success();
    }
}

#[test]
fn offset_range_check_applies_only_to_the_target_not_the_data() {
    // An out-of-range offset in the input is data, not a mistake in the
    // command, so it is read rather than refused.
    csvdt()
        .args(["-H", "-p", "-u0", "-i", "0,replace"])
        .write_stdin("ts\n2024-01-01T12:00:00+15:00\n")
        .assert()
        .success()
        .stdout("to_utc\n2023-12-31T21:00:00+00:00\n");
}

// ---- --remove with several columns ---------------------------------------

#[test]
fn remove_drops_several_columns_at_once() {
    csvdt()
        .args(["-H", "-p", "--remove", "0,2,4"])
        .write_stdin("a,b,c,d,e\n0,1,2,3,4\n")
        .assert()
        .success()
        .stdout("b,d\n1,3\n");
}

#[test]
fn remove_column_numbers_refer_to_the_input_not_the_shrinking_row() {
    // --remove 0,1 must take the first two input columns. If the removals were
    // applied low-to-high the second would land on whatever had shifted into
    // position 1, dropping "a" and "c" instead of "a" and "b".
    csvdt()
        .args(["-H", "-p", "--remove", "0,1"])
        .write_stdin("a,b,c,d\n0,1,2,3\n")
        .assert()
        .success()
        .stdout("c,d\n2,3\n");
}

#[test]
fn remove_is_order_independent() {
    // The same set of columns, in any order, removes the same fields. Repeats
    // used to be accepted here too ("4,0,4,2,2" was this same list); they are
    // refused now, which remove_rejects_a_repeated_column covers.
    for spec in ["0,2,4", "4,2,0", "2,0,4"] {
        csvdt()
            .args(["-H", "-p", "--remove", spec])
            .write_stdin("a,b,c,d,e\n0,1,2,3,4\n")
            .assert()
            .success()
            .stdout("b,d\n1,3\n");
    }
}

#[test]
fn remove_rejects_the_whole_list_when_any_column_is_out_of_bound() {
    // A row is either edited fully or reported, never left partly removed.
    csvdt()
        .args(["-H", "-p", "--remove", "0,9"])
        .write_stdin("a,b,c\n0,1,2\n")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("Specified column is out of bound"));
}

#[test]
fn remove_rejects_a_malformed_column_list() {
    for spec in ["0,x", "0,,2", "0,-1"] {
        csvdt()
            .args(["-H", "-p", "--remove", spec])
            .write_stdin("a,b,c\n0,1,2\n")
            .assert()
            .failure()
            .code(100);
    }
}

#[test]
fn remove_has_no_short_form() {
    // --remove is the only spelling; -R is no longer bound to anything.
    csvdt()
        .args(["-H", "-p", "-R1"])
        .write_stdin("a,b,c\n0,1,2\n")
        .assert()
        .failure()
        .code(2);
}

#[test]
fn arrange_short_form_reorders_without_removing() {
    // -a is the short form, and it reorders rather than drops: three columns
    // in, three columns out.
    csvdt()
        .args(["-H", "-p", "-a1"])
        .write_stdin("a,b,c\n0,1,2\n")
        .assert()
        .success()
        .stdout("b,a,c\n1,0,2\n");
}

#[test]
fn remove_can_drop_every_column() {
    // Leaves an empty record, which CSV writes as a quoted empty field so it
    // stays distinguishable from a row that was never there.
    csvdt()
        .args(["-H", "-p", "--remove", "0,1"])
        .write_stdin("a,b\n0,1\n")
        .assert()
        .success()
        .stdout("\"\"\n\"\"\n");
}

#[test]
fn remove_applies_per_row_when_rows_have_differing_widths() {
    // With -f each row is handled on its own width, so the same two input
    // positions are dropped from every row regardless of how many fields it
    // has -- rather than a fixed number of trailing fields.
    csvdt()
        .args(["-H", "-p", "-f", "--remove", "0,1"])
        .write_stdin(
            "a,b,c,d\n\
             0,1,2,3\n\
             0,1,2,3,4,5\n\
             0,1,2\n",
        )
        .assert()
        .success()
        .stdout(
            "c,d\n\
             2,3\n\
             2,3,4,5\n\
             2\n",
        );
}

#[test]
fn remove_accepts_the_equals_spelling() {
    csvdt()
        .args(["-H", "-p", "--remove=0,2"])
        .write_stdin("a,b,c,d\n0,1,2,3\n")
        .assert()
        .success()
        .stdout("b,d\n1,3\n");
}

// ---- -R/--arrange -------------------------------------------------------

#[test]
fn arrange_reorders_the_named_columns() {
    csvdt()
        .args(["-H", "-p", "-a3,1,0,2"])
        .write_stdin("a,b,c,d\n0,1,2,3\n")
        .assert()
        .success()
        .stdout("d,b,a,c\n3,1,0,2\n");
}

#[test]
fn arrange_lets_unnamed_columns_follow_in_original_order() {
    // The point of the design: naming a subset moves those columns to the
    // front and keeps the rest, so nothing is lost and "move this column
    // first" is a short argument.
    csvdt()
        .args(["-H", "-p", "-a3,1"])
        .write_stdin("a,b,c,d,e\n0,1,2,3,4\n")
        .assert()
        .success()
        .stdout("d,b,a,c,e\n3,1,0,2,4\n");

    csvdt()
        .args(["-H", "-p", "-a4"])
        .write_stdin("a,b,c,d,e\n0,1,2,3,4\n")
        .assert()
        .success()
        .stdout("e,a,b,c,d\n4,0,1,2,3\n");
}

#[test]
fn arrange_preserves_the_row_width() {
    // Whatever the list, the row comes out as wide as it went in.
    for spec in ["-a0", "-a2,0", "-a3,2,1,0", "-a0,1,2,3"] {
        csvdt()
            .args(["-H", "-p", spec])
            .write_stdin("a,b,c,d\n0,1,2,3\n")
            .assert()
            .success()
            .stdout(predicate::function(|out: &str| {
                out.lines().all(|line| line.split(',').count() == 4)
            }));
    }
}

#[test]
fn arrange_identity_list_is_a_passthrough() {
    csvdt()
        .args(["-H", "-p", "-a0,1,2"])
        .write_stdin("a,b,c\n0,1,2\n")
        .assert()
        .success()
        .stdout("a,b,c\n0,1,2\n");
}

#[test]
fn remove_rejects_a_repeated_column() {
    // It used to deduplicate silently, so --remove 1,1 removed column 1 and
    // said nothing, while -a0,0 was refused -- the two lists disagreed about
    // the same mistake. A repeat means the list was miscounted, and the whole
    // point of addressing columns by position is that the numbers are exact.
    for spec in ["1,1", "2,0,2"] {
        csvdt()
            .args(["-H", "-p", "--remove", spec])
            .write_stdin("a,b,c\n0,1,2\n")
            .assert()
            .failure()
            .code(100)
            .stderr(predicate::str::contains("more than once"));
    }

    // Distinct columns in any order are still fine, and still removed by
    // position rather than by what has moved into it.
    csvdt()
        .args(["-H", "-p", "--remove", "2,0"])
        .write_stdin("a,b,c\n0,1,2\n")
        .assert()
        .success()
        .stdout("b\n1\n");
}

#[test]
fn a_repeated_column_error_names_the_column() {
    // "lists a column more than once" left you counting to find which.
    csvdt()
        .args(["-H", "-p", "-a", "2,0,2"])
        .write_stdin("a,b,c\n0,1,2\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("names column 2 more than once"));

    csvdt()
        .args(["-H", "-p", "--remove", "1,1"])
        .write_stdin("a,b,c\n0,1,2\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("names column 1 more than once"));
}

#[test]
fn arrange_rejects_a_repeated_column() {
    // The list is the new order, so a repeat would both duplicate a field and
    // leave the row wider than the input.
    for spec in ["-a0,0", "-a2,0,2"] {
        csvdt()
            .args(["-H", "-p", spec])
            .write_stdin("a,b,c\n0,1,2\n")
            .assert()
            .failure()
            .code(100)
            .stderr(predicate::str::contains("more than once"));
    }
}

#[test]
fn duration_rejects_a_column_measured_against_itself() {
    // -d1,1 was accepted and answered PT0S on every row -- a full column of
    // plausible results from a typo for -d1,2. Unlike parse_err, nothing in
    // the output said so, and PT0S is a value -d writes for real reasons
    // elsewhere. --remove and -a already refused the same shape of mistake.
    csvdt()
        .args(["-H", "-p", "-d1,1"])
        .write_stdin("x,ts1,ts2\n1,2024-01-01T00:00:00Z,2024-01-01T02:00:00Z\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("names column 1 more than once"));

    // Two distinct columns are the point of the two-column form, either way
    // round, and one column on its own is the row-over-row form.
    for spec in ["-d1,2", "-d2,1", "-d1"] {
        csvdt()
            .args(["-H", "-p", spec])
            .write_stdin("x,ts1,ts2\n1,2024-01-01T00:00:00Z,2024-01-01T02:00:00Z\n")
            .assert()
            .success();
    }
}

#[test]
fn arrange_out_of_bound_column_is_a_clean_error() {
    csvdt()
        .args(["-H", "-p", "-a9"])
        .write_stdin("a,b,c\n0,1,2\n")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("Specified column is out of bound"));
}

#[test]
fn arrange_rejects_a_malformed_list() {
    for spec in ["-a0,x", "-a-1"] {
        csvdt()
            .args(["-H", "-p", spec])
            .write_stdin("a,b,c\n0,1,2\n")
            .assert()
            .failure()
            .code(100);
    }
}

#[test]
fn arrange_conflicts_with_every_other_action_and_with_insert() {
    for other in [
        vec!["-u0"], vec!["-l0"], vec!["-r0"], vec!["-s0"],
        vec!["-d0"], vec!["-o", "0,Z"], vec!["--remove", "0"],
    ] {
        let mut args = vec!["-H", "-p", "-a1"];
        args.extend(other.iter().copied());
        csvdt()
            .args(&args)
            .write_stdin("a,b\n0,1\n")
            .assert()
            .failure()
            .code(2);
    }

    // Like --remove, rearranging writes no new value, so there is no
    // before/after/replace to position.
    csvdt()
        .args(["-H", "-p", "-a1", "-i", "0,after"])
        .write_stdin("a,b\n0,1\n")
        .assert()
        .failure()
        .code(2);
}

#[test]
fn arrange_preserves_field_content_needing_quotes() {
    csvdt()
        .args(["-H", "-p", "-a2,0"])
        .write_stdin("a,b,c\n\"x,y\",\"p\nq\",z\n")
        .assert()
        .success()
        .stdout("c,a,b\nz,\"x,y\",\"p\nq\"\n");
}

#[test]
fn arrange_applies_per_row_under_flexible_widths() {
    csvdt()
        .args(["-H", "-p", "-f", "-a2,0"])
        .write_stdin("a,b,c\n0,1,2\n0,1,2,3,4\n")
        .assert()
        .success()
        .stdout("c,a,b\n2,0,1\n2,0,1,3,4\n");
}

// ---- --flexible=fix -------------------------------------------------------

#[test]
fn flexible_fix_pads_short_rows_to_the_header_width() {
    csvdt()
        .args(["-H", "-p", "-f=fix"])
        .write_stdin("a,b,c\n0,1\n0,1,2\n0\n")
        .assert()
        .success()
        .stdout("a,b,c\n0,1,\n0,1,2\n0,,\n");
}

#[test]
fn flexible_fix_output_is_rectangular() {
    // The whole point: what comes out is valid CSV.
    csvdt()
        .args(["-H", "-p", "-f=fix"])
        .write_stdin("a,b,c,d\n0\n0,1\n0,1,2\n0,1,2,3\n")
        .assert()
        .success()
        .stdout(predicate::function(|out: &str| {
            let widths: std::collections::HashSet<usize> =
                out.lines().map(|l| l.split(',').count()).collect();
            widths == std::collections::HashSet::from([4])
        }));
}

// The log --fill-log writes, read back.
//
// Each of these names the destination and reads the file, rather than
// asking stderr, because the file is what an analyst is given and the
// header line is part of what they are given.
fn fill_log_of(arguments: &[&str], input: &str, name: &str) -> String {
    let path = tempfile_dir().join(name);
    let _ = std::fs::remove_file(&path);
    csvdt()
        .args(arguments)
        .arg("--fill-log")
        .arg(&path)
        .write_stdin(input.to_string())
        .assert();
    std::fs::read_to_string(&path).expect("the fill log should have been written")
}

#[test]
fn the_fill_log_names_every_row_a_filling_mode_padded() {
    // What the option is for: the filling modes add fields that cannot be
    // told from fields the source really held, so the run has to be able to
    // say where it did that. One row per record touched, and the record
    // named twice -- by line, which is what an editor jumps to, and by
    // record, which is a different number the moment a field holds a
    // newline.
    //
    // 'width' is what the row was measured against and 'added' what was
    // appended, so a reader can see both the shape it was brought to and how
    // much of it is new.
    let input = "a,b,c,d\n1,2\n3,4,5,6\n7\n";
    let expected = "line,record,fields,width,delta,status\n\
                    2,1,2,4,-2,padded\n\
                    4,3,1,4,-3,padded\n";
    assert_eq!(fill_log_of(&["-H", "-p", "-f=widest"], input, "widest.csv"), expected);
    // fix reaches the same account here, the header being as wide as the
    // widest row. Both are asked, so a change to either mode's bookkeeping
    // cannot hide behind the other's.
    assert_eq!(fill_log_of(&["-H", "-p", "-f=fix"], input, "fix.csv"), expected);
}

#[test]
fn the_fill_log_says_needed_where_plain_flexible_changed_nothing() {
    // Plain --flexible pads nothing, so the log is the only account there is:
    // the output keeps the ragged shape and nothing in it marks which rows
    // were short. 'needed' rather than 'padded' says the row is still short.
    //
    // Measured against the first record, which is all a single pass has.
    // That makes 'over' reachable here for a row wider than the first, where
    // widest would simply have counted it -- the two modes disagree about
    // the same file, which is why the run says on stderr which reference it
    // used.
    // The header is three fields and the first data row two, so the log can
    // only come out as it does if the header is what was measured against.
    // With them the same width -- which they were here at first -- dropping
    // the header as the reference changed nothing and the case proved
    // nothing about which record it used.
    let log = fill_log_of(&["-H", "-p", "-f"], "a,b,c\n1,2\n3,4,5,6\n7\n", "keep.csv");
    assert_eq!(
        log,
        "line,record,fields,width,delta,status\n\
         2,1,2,3,-1,short\n\
         3,2,4,3,1,over\n\
         4,3,1,3,-2,short\n",
    );
}

#[test]
fn the_fill_log_ends_on_the_row_that_ended_the_run() {
    // A row wider than the width stops fix and widest. Written to the log
    // before the error returns, so the log ends on the row the message names
    // rather than stopping one short of the row a reader most wants.
    let log = fill_log_of(&["-H", "-p", "-f=fix"], "a,b\n1,2\n3,4,5\n", "over.csv");
    assert_eq!(log, "line,record,fields,width,delta,status\n3,2,3,2,1,over\n");

    // And the run still fails, so the log is an account of a failure rather
    // than of a run that quietly went on.
    let path = tempfile_dir().join("over-status.csv");
    csvdt()
        .args(["-H", "-p", "-f=fix"])
        .arg("--fill-log")
        .arg(&path)
        .write_stdin("a,b\n1,2\n3,4,5\n")
        .assert()
        .failure();
}

#[test]
fn the_fill_log_counts_the_header_when_widest_pads_it() {
    // widest pads the header too, and the added names are empty rather than
    // invented -- an analyst looking at a column with no name needs to know
    // whether the source had none or csvdt made one. The header is read
    // before any record, so it has no position of its own: line 1, record 0,
    // which is what the reader would have said had it been asked.
    let log = fill_log_of(&["-H", "-p", "-f=widest"], "a,b\n1,2\n3,4,5\n", "header.csv");
    assert_eq!(
        log,
        "line,record,fields,width,delta,status\n\
         1,0,2,3,-1,padded\n\
         2,1,2,3,-1,padded\n",
    );
}

#[test]
#[cfg(unix)]
fn the_fill_log_refuses_to_be_the_input_or_the_output() {
    // The log is created, and creating truncates. Pointed at the input, it
    // emptied the file before the run read it, then read the empty file it
    // had just made, wrote nothing, and exited 0 -- 75 bytes of somebody's
    // data replaced by a header line, with a successful status on top. That
    // is the loss refuse_input_that_is_also_the_output was written for,
    // reached through a door it does not watch.
    //
    // Asked by identity rather than by path, which is why the symlink case is
    // here: comparing the two strings would have let it straight through.
    let directory = tempfile_dir();
    let data = "id,when,level\n7,2023-11-14T22:13:20+01:00,info\n8,2023-11-15T09:00:00+01:00\n";

    let write = |name: &str| {
        let path = directory.join(name);
        std::fs::write(&path, data).expect("the fixture should be written");
        path
    };

    // Named as both the input and the log.
    let input = write("clash.csv");
    csvdt()
        .args(["-H", "-p", "-f=widest"])
        .arg("--fill-log")
        .arg(&input)
        .arg(&input)
        .assert()
        .failure()
        .stderr(predicate::str::contains("both the input and where"));
    assert_eq!(
        std::fs::read_to_string(&input).unwrap(),
        data,
        "the input was written to anyway",
    );

    // The same file reached by another name. A symbolic link is the case a
    // string comparison cannot see, and the one a person is likeliest to have
    // lying about.
    let target = write("real.csv");
    let link = directory.join("link.csv");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&target, &link).expect("a symlink");
    csvdt()
        .args(["-H", "-p", "-f=widest"])
        .arg("--fill-log")
        .arg(&link)
        .arg(&target)
        .assert()
        .failure()
        .stderr(predicate::str::contains("both the input and where"));
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        data,
        "the input was written to through the link",
    );

    // And with no input path at all: standard input redirected from the very
    // file the log would replace.
    let piped = write("piped.csv");
    let script = format!(
        "exec '{}' -H -p -f=widest --fill-log '{}' < '{}'",
        assert_cmd::cargo::cargo_bin("csvdt").display(),
        piped.display(),
        piped.display(),
    );
    let done = std::process::Command::new("sh")
        .args(["-c", &script])
        .output()
        .expect("sh should run");
    assert!(!done.status.success(), "the run should have refused");
    assert_eq!(
        std::fs::read_to_string(&piped).unwrap(),
        data,
        "the input was emptied by the redirection csvdt was asked to write",
    );

    // And the other end: the log named as the file the output is being
    // redirected into. Nothing of the input is lost here, but the log's rows
    // land in the middle of the conversion, which is the corruption the
    // option was given a destination of its own to avoid.
    let shared = directory.join("shared.csv");
    let source = write("source.csv");
    let script = format!(
        "exec '{}' -H -p -f=widest --fill-log '{}' '{}' > '{}'",
        assert_cmd::cargo::cargo_bin("csvdt").display(),
        shared.display(),
        source.display(),
        shared.display(),
    );
    let done = std::process::Command::new("sh")
        .args(["-c", &script])
        .output()
        .expect("sh should run");
    assert!(!done.status.success(), "the run should have refused");
    assert!(
        String::from_utf8_lossy(&done.stderr)
            .contains("where both the output and"),
        "said: {}",
        String::from_utf8_lossy(&done.stderr),
    );

    // A destination that is nothing to do with either is the ordinary case
    // and still works, so the guard refuses a clash rather than the option.
    let clean = write("ordinary.csv");
    let log = directory.join("ordinary.log");
    let _ = std::fs::remove_file(&log);
    csvdt()
        .args(["-H", "-p", "-f=widest"])
        .arg("--fill-log")
        .arg(&log)
        .arg(&clean)
        .assert()
        .success();
    assert!(
        std::fs::read_to_string(&log).unwrap().starts_with("line,record,"),
        "the ordinary case should still write a log",
    );
}

#[test]
fn the_fill_log_is_refused_where_it_would_have_nothing_to_say() {
    // Without a filling mode there is nothing to account for, and an empty
    // log reads as a clean file -- the one conclusion it must never invite.
    csvdt()
        .args(["-H", "--fill-log", "-"])
        .write_stdin("a,b\n1,2\n")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--flexible"));

    // --peek writes no CSV, so it pads nothing either. It refuses the option
    // rather than writing a log of a run that did not happen.
    csvdt()
        .args(["--peek", "--fill-log", "-"])
        .write_stdin("a,b\n1,2\n")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn a_fill_log_that_cannot_be_written_is_reported_before_the_run() {
    // Opened before the counting pass, so a destination that cannot be
    // written costs nothing: a widest run over a large file would otherwise
    // read the whole of it before finding out it had nowhere to report.
    //
    // The message names the option and offers '-', since a reader who cannot
    // write a file can still read the log on stderr.
    let missing = std::env::temp_dir()
        .join(format!("csvdt-absent-{}", std::process::id()))
        .join("log.csv");
    csvdt()
        .args(["-H", "-p", "-f=widest"])
        .arg("--fill-log")
        .arg(&missing)
        .write_stdin("a,b\n1,2\n")
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains("--fill-log"))
        .stderr(predicate::str::contains("'-' for standard error"));
}

#[test]
fn a_filling_mode_that_fails_partway_has_already_written_what_it_wrote() {
    // flexible.txt used to end the spool paragraph with "nothing incomplete
    // is ever sent to the output", which is true of the counting pass and
    // not of the run. Once a filling mode starts writing it writes as it
    // reads, so a failure partway leaves valid rectangular CSV that simply
    // stops early -- and a filled file is the one kind a reader cannot tell
    // is short by looking, since every row in it is the right width.
    //
    // Measured on a widest run whose file grew between its two reads: 1.5
    // million rows on standard output, then exit 1. That race needs a moving
    // file and is not what a test should chase; fix reaches the same state
    // with a fixed input, because a row wider than the first is the error it
    // has in common with widest.
    //
    // The other half is the part that was true, and is asked here so the
    // sentence keeps both halves: a run that fails before the counting pass
    // is done writes nothing at all.
    csvdt()
        .args(["-f=fix"])
        .write_stdin("a,b,c\n1,2\n3,4,5,6\n")
        .assert()
        .failure()
        .stdout("a,b,c\n1,2,\n")
        .stderr(predicate::str::contains("width of the first record"));

    // A path that is not there, so the spool cannot be created and the run
    // fails while counting. Nothing is made, so there is nothing to clean up.
    let missing = std::env::temp_dir()
        .join(format!("csvdt-absent-{}", std::process::id()));
    with_temp_dir(csvdt(), &missing)
        .args(["-f=widest"])
        .write_stdin("a,b,c\n1,2\n")
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains("could not create a temporary file"));
}

#[test]
fn flexible_fix_without_headers_uses_the_first_data_row() {
    csvdt()
        .args(["-f=fix"])
        .write_stdin("0,1,2\n0,1\n0\n")
        .assert()
        .success()
        .stdout("0,1,2\n0,1,\n0,,\n");
}

#[test]
fn flexible_fix_long_form_is_accepted() {
    csvdt()
        .args(["-H", "-p", "--flexible=fix"])
        .write_stdin("a,b,c\n0\n")
        .assert()
        .success()
        .stdout("a,b,c\n0,,\n");
}

#[test]
fn flexible_bare_still_passes_ragged_rows_through_unchanged() {
    // The default mode is unchanged: no padding, ragged output.
    csvdt()
        .args(["-H", "-p", "-f"])
        .write_stdin("a,b,c\n0,1\n0\n")
        .assert()
        .success()
        .stdout("a,b,c\n0,1\n0\n");
}

#[test]
fn flexible_fix_refuses_a_row_wider_than_the_reference() {
    // Padding cannot shorten a row, and cutting it down would discard fields.
    csvdt()
        .args(["-H", "-p", "-f=fix"])
        .write_stdin("a,b\n0,1,2,3\n")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("would discard data"));
}

#[test]
fn widest_still_pads_a_file_that_holds_still() {
    // The companion to the unit test on the two too-wide messages: the mode's
    // ordinary path is unaffected, and a file whose widest record is not its
    // first is exactly what 'widest' exists for.
    let dir = tempfile_dir();
    let path = dir.join("widest-steady.csv");
    std::fs::write(&path, "a,b\n0,1\n2,3,4,5\n6,7\n").unwrap();

    csvdt()
        .args(["-H", "-p", "-f=widest"])
        .arg(&path)
        .assert()
        .success()
        .stdout("a,b,,\n0,1,,\n2,3,4,5\n6,7,,\n");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn help_says_widest_assumes_the_input_holds_still() {
    // Reading twice assumes the same input both times, which a file still
    // being appended to is not. That assumption was nowhere in the help, so
    // the one mode that promises no row can be too wide could report a row
    // that was, with nothing to say why.
    csvdt()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("assumes the same input both times"));
}

#[test]
fn help_bounds_memory_by_the_widest_record_not_the_file() {
    // It claimed memory "does not grow with the file, however large the
    // input" -- measured false for the case its own unclosed-quote section
    // documents: 200 MB of ordinary rows peak at 9 MB, the same bytes behind
    // one unclosed quote become a single field and cost 260 MB. The claim
    // now states the true bound, and this holds it to that.
    //
    // Whitespace collapsed, for the reason the --comment test gives: what is
    // promised is that --help says this, not where the line breaks fall in it.
    let help = help_words();

    assert!(
        help.contains("grows with the widest single record"),
        "--help no longer states the real memory bound",
    );
    assert!(
        !help.contains("does not grow with the file"),
        "--help has gone back to the claim that was measured false",
    );
}

#[test]
fn flexible_fix_does_not_swallow_a_following_file_argument() {
    // --flexible's mode must be attached with '=' precisely so that a bare -f
    // followed by a path cannot read the path as the mode -- the same trap -d
    // was fixed for.
    let dir = tempfile_dir();
    let path = dir.join("flex.csv");
    std::fs::write(&path, "a,b,c\n0,1\n").unwrap();

    csvdt()
        .args(["-H", "-p", "-f"])
        .arg(&path)
        .assert()
        .success()
        .stdout("a,b,c\n0,1\n");
}

#[test]
fn flexible_mode_written_separately_explains_itself() {
    // "-f fix" leaves "fix" where the file goes; the error should say so
    // rather than only reporting a missing file.
    csvdt()
        .args(["-H", "-p", "-f", "fix"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("attach it: -f=fix"));
}

#[test]
fn flexible_fix_pads_before_an_action_runs() {
    // A row missing the action's column entirely gets an empty field there, so
    // the action reports a parse error for that row instead of the whole run
    // failing as out of bound.
    csvdt()
        .args(["-H", "-p", "-f=fix", "-u1", "-i", "1,replace"])
        .write_stdin("a,ts,c\n0,2024-01-01T00:00:00Z,x\n1\n")
        .assert()
        .success()
        .stdout("a,to_utc,c\n0,2024-01-01T00:00:00+00:00,x\n1,parse_err,\n");

    // Without fix, that same row is out of bound.
    csvdt()
        .args(["-H", "-p", "-f", "-u1", "-i", "1,replace"])
        .write_stdin("a,ts,c\n0,2024-01-01T00:00:00Z,x\n1\n")
        .assert()
        .failure()
        .code(1);
}

#[test]
fn flexible_fix_rejects_an_unknown_mode() {
    csvdt()
        .args(["-H", "-p", "-f=wat"])
        .write_stdin("a,b\n0\n")
        .assert()
        .failure()
        .code(2);
}

#[test]
fn help_warns_that_fix_adds_data_not_in_the_input() {
    csvdt()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("add data that was not in the"));
}

// ---- input the tool must handle, or refuse cleanly ------------------------

#[test]
fn non_utf8_input_is_a_clean_error_naming_where_it_failed() {
    // Fields are read as text, so a byte sequence that isn't UTF-8 cannot be
    // represented. That has to fail clearly rather than corrupt or truncate.
    // Note this applies even when the offending bytes sit in a column no
    // action touches.
    //
    // Every coordinate the message gives is pinned here, because the docs
    // describe them and only the words "invalid utf-8" were ever checked --
    // which let the description say the byte offset was the first bad byte's
    // for as long as nobody looked. It is the record's. In this file:
    //
    //   ts,note\n            bytes 0..7
    //   2024-...Z,caf\xe9\n  record 1, starting at byte 8
    //
    // so the bad byte is at 32, the record starts at 8, and 3 is how far into
    // field 1 the bad byte lies. All three numbers are useful and none of them
    // is the other.
    let dir = tempfile_dir();
    let path = dir.join("latin1.csv");
    let bytes = b"ts,note\n2024-01-01T00:00:00Z,caf\xe9\n";
    std::fs::write(&path, bytes).unwrap();

    assert_eq!(bytes.iter().position(|byte| *byte == 0xe9), Some(32));

    csvdt()
        .args(["-H", "-p", "-u0"])
        .arg(&path)
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("invalid utf-8"))
        // The record and the line it is in, the field, and the byte the
        // record begins at -- not the byte the bad one is at.
        .stderr(predicate::str::contains(
            "record 1 (line 2, field: 1, byte: 8)"))
        // And how far into that field it lies.
        .stderr(predicate::str::contains("field 1 near byte index 3"));
}

#[test]
fn nul_bytes_inside_a_field_are_preserved() {
    // A NUL is valid UTF-8 and carries no CSV meaning, so it must survive
    // untouched rather than terminating the field.
    let dir = tempfile_dir();
    let path = dir.join("nul.csv");
    std::fs::write(&path, b"a,b\nx\x00y,z\n").unwrap();

    let out = csvdt()
        .args(["-H", "-p"])
        .arg(&path)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(out, b"a,b\nx\x00y,z\n");
}

#[test]
fn input_without_a_trailing_newline_is_read_fully() {
    let dir = tempfile_dir();
    let path = dir.join("nonl.csv");
    std::fs::write(&path, "a,b\n0,1").unwrap();

    csvdt()
        .args(["-H", "-p"])
        .arg(&path)
        .assert()
        .success()
        .stdout("a,b\n0,1\n");
}

#[test]
fn duplicate_header_names_are_positional_not_matched_by_name() {
    // Columns are addressed by position throughout, so repeated header names
    // are not ambiguous.
    csvdt()
        .args(["-H", "-p", "-a2,0"])
        .write_stdin("a,a,a\n0,1,2\n")
        .assert()
        .success()
        .stdout("a,a,a\n2,0,1\n");
}

// ---- properties of the column operations ---------------------------------

#[test]
fn arrange_with_the_inverse_permutation_restores_the_original() {
    // -a3,1,0,2 moves column 0 to position 2, 1 to 1, 2 to 3, 3 to 0; the
    // inverse of that is 2,1,3,0.
    csvdt()
        .args(["-H", "-p", "-a3,1,0,2"])
        .write_stdin("a,b,c,d\nv0,v1,v2,v3\n")
        .assert()
        .success()
        .stdout("d,b,a,c\nv3,v1,v0,v2\n");

    csvdt()
        .args(["-H", "-p", "-a2,1,3,0"])
        .write_stdin("d,b,a,c\nv3,v1,v0,v2\n")
        .assert()
        .success()
        .stdout("a,b,c,d\nv0,v1,v2,v3\n");
}

#[test]
fn flexible_fix_is_idempotent() {
    // Running fix over already-fixed output must change nothing, since every
    // row is already the reference width.
    let once = "a,b,c\n0,1,\n0,,\n";
    csvdt()
        .args(["-H", "-p", "-f=fix"])
        .write_stdin("a,b,c\n0,1\n0\n")
        .assert()
        .success()
        .stdout(once);

    csvdt()
        .args(["-H", "-p", "-f=fix"])
        .write_stdin(once)
        .assert()
        .success()
        .stdout(once);
}

// ---- UTF-16 input ---------------------------------------------------------

#[test]
fn utf16_with_a_byte_order_mark_is_named_in_the_error() {
    // Without this the csv reader reports "invalid utf-8 at byte 0", which
    // doesn't tell anyone what to do about it.
    for (name, bytes) in [
        ("le.csv", b"\xff\xfe".to_vec()),
        ("be.csv", b"\xfe\xff".to_vec()),
    ] {
        let dir = tempfile_dir();
        let path = dir.join(name);
        let mut content = bytes;
        content.extend_from_slice(b"t\0s\0\n\0");
        std::fs::write(&path, &content).unwrap();

        csvdt()
            .args(["-H", "-p"])
            .arg(&path)
            .assert()
            .failure()
            .code(1)
            .stderr(predicate::str::contains("looks like UTF-16"))
            .stderr(predicate::str::contains("iconv"));
    }
}

#[test]
fn utf16_without_a_byte_order_mark_is_refused_not_silently_mangled() {
    // Pure-ASCII UTF-16 has a NUL after every character, and a NUL is valid
    // UTF-8 -- so nothing else objects. Read with --flexible this used to exit
    // 0 having written NUL-interleaved output, which is the one failure mode
    // this tool must not have. The alternating pattern is the signature.
    let dir = tempfile_dir();
    let path = dir.join("nobom.csv");
    let utf16: Vec<u8> = "ts,name\n2024-01-01T00:00:00Z,bob\n"
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    std::fs::write(&path, &utf16).unwrap();

    // -f is the case that used to pass silently.
    csvdt()
        .args(["-H", "-p", "-f"])
        .arg(&path)
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("every second byte is NUL"));
}

#[test]
fn the_utf16_check_does_not_catch_fields_that_really_contain_nuls() {
    // The test is the full alternating pattern, not merely the presence of a
    // NUL, so genuine NUL bytes -- including a run of them -- still pass
    // through untouched.
    for (name, content) in [
        ("one.csv", b"a,b\nx\0y,z\n".to_vec()),
        ("many.csv", b"a,b\nx\0\0\0\0\0y,z\n".to_vec()),
    ] {
        let dir = tempfile_dir();
        let path = dir.join(name);
        std::fs::write(&path, &content).unwrap();

        let out = csvdt()
            .args(["-H", "-p"])
            .arg(&path)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        assert_eq!(out, content, "{name}");
    }
}

#[test]
fn utf8_input_with_and_without_a_bom_still_works() {
    // The encoding probe reads ahead and puts the bytes back, so the UTF-8 BOM
    // must still be stripped by the reader as before.
    csvdt()
        .args(["-H", "-p"])
        .write_stdin("\u{feff}a,b\n0,1\n")
        .assert()
        .success()
        .stdout("a,b\n0,1\n");

    csvdt()
        .args(["-H", "-p"])
        .write_stdin("a,b\n0,1\n")
        .assert()
        .success()
        .stdout("a,b\n0,1\n");
}

// ---- --flexible=widest ----------------------------------------------------

#[test]
fn flexible_widest_pads_everything_to_the_widest_record() {
    // The header here is the SHORT one, which is exactly what fix cannot
    // handle: it pads out too, with empty names rather than invented ones.
    csvdt()
        .args(["-H", "-p", "-f=widest"])
        .write_stdin("a,b\n0,1,2,3\n0\n")
        .assert()
        .success()
        .stdout("a,b,,\n0,1,2,3\n0,,,\n");
}

#[test]
fn flexible_widest_succeeds_where_fix_refuses() {
    let input = "a,b\n0,1,2,3\n0\n";

    // fix measures the first record and so calls the complete row too wide.
    csvdt()
        .args(["-H", "-p", "-f=fix"])
        .write_stdin(input)
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("would discard data"));

    // widest measures the file and pads to it.
    csvdt()
        .args(["-H", "-p", "-f=widest"])
        .write_stdin(input)
        .assert()
        .success();
}

#[test]
fn flexible_widest_works_from_a_pipe_and_from_a_file_alike() {
    // A pipe cannot be rewound, so stdin is spooled to a temporary file. The
    // result must be identical either way.
    let expected = "a,b,,\n0,1,2,3\n0,,,\n";

    csvdt()
        .args(["-H", "-p", "-f=widest"])
        .write_stdin("a,b\n0,1,2,3\n0\n")
        .assert()
        .success()
        .stdout(expected);

    let dir = tempfile_dir();
    let path = dir.join("widest.csv");
    std::fs::write(&path, "a,b\n0,1,2,3\n0\n").unwrap();

    csvdt()
        .args(["-H", "-p", "-f=widest"])
        .arg(&path)
        .assert()
        .success()
        .stdout(expected);
}

#[test]
fn flexible_widest_output_is_rectangular() {
    csvdt()
        .args(["-H", "-p", "-f=widest"])
        .write_stdin("a\n0,1\n0,1,2,3,4\n0\n0,1,2\n")
        .assert()
        .success()
        .stdout(predicate::function(|out: &str| {
            let widths: std::collections::HashSet<usize> =
                out.lines().map(|l| l.split(',').count()).collect();
            widths == std::collections::HashSet::from([5])
        }));
}

#[test]
fn flexible_widest_is_idempotent() {
    let once = "a,b,,\n0,1,2,3\n0,,,\n";
    csvdt()
        .args(["-H", "-p", "-f=widest"])
        .write_stdin(once)
        .assert()
        .success()
        .stdout(once);
}

#[test]
fn flexible_widest_leaves_no_temporary_file_behind() {
    // The spool is removed by its Drop, so a run must not litter the temp
    // directory -- on success or on failure. Each run gets a private TMPDIR so
    // this cannot see, or be confused by, a spool belonging to another test
    // running at the same time.
    let count_in = |dir: &std::path::Path| {
        std::fs::read_dir(dir)
            .map(|entries| entries.filter_map(Result::ok).count())
            .unwrap_or(0)
    };

    let ok_dir = tempfile_dir().join("tmp-widest-ok");
    std::fs::create_dir_all(&ok_dir).unwrap();
    with_temp_dir(csvdt(), &ok_dir)
        .args(["-H", "-p", "-f=widest"])
        .write_stdin("a,b\n0,1,2\n")
        .assert()
        .success();
    assert_eq!(count_in(&ok_dir), 0, "temp file left after a successful run");

    let err_dir = tempfile_dir().join("tmp-widest-err");
    std::fs::create_dir_all(&err_dir).unwrap();
    with_temp_dir(csvdt(), &err_dir)
        .args(["-H", "-p", "-f=widest", "-u9"])
        .write_stdin("a,b\n0,1\n")
        .assert()
        .failure();
    assert_eq!(count_in(&err_dir), 0, "temp file left after a failed run");
}

#[test]
fn flexible_widest_still_refuses_bad_encodings() {
    // The counting pass reads bytes without validating them, so the check has
    // to still happen -- it does, when the file is opened for either pass.
    let dir = tempfile_dir();
    let path = dir.join("w-latin1.csv");
    std::fs::write(&path, b"a,b\ncaf\xe9\n").unwrap();

    csvdt()
        .args(["-H", "-p", "-f=widest"])
        .arg(&path)
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("invalid utf-8"));
}

// ---- --peek: the column numbers, above the rows they belong to -----------

#[test]
fn peek_numbers_the_columns_above_the_header_and_the_first_row() {
    csvdt()
        .args(["-H", "-p", "--peek"])
        .write_stdin(
            "when,who,level\n\
             2026-01-01T00:00:00Z,ada,info\n\
             2026-01-02T00:00:00Z,bob,warn\n",
        )
        .assert()
        .success()
        // Padded so each number sits over its own field, and only the first
        // data row: the rest of the file is not read.
        .stdout(
            "0                     1    2\n\
             when                  who  level\n\
             2026-01-01T00:00:00Z  ada  info\n",
        );
}

#[test]
fn peek_follows_the_ordinary_meaning_of_has_header_and_print_header() {
    // -H says a header is there and the reader takes that record as one, so
    // what is shown is the first *data* row -- the file's second line. It is
    // -p that asks for a header to be written, here as everywhere else.
    csvdt()
        .args(["-H", "--peek"])
        .write_stdin("when,who\n2026-01-01T00:00:00Z,ada\n2026-01-02T00:00:00Z,bob\n")
        .assert()
        .success()
        .stdout("0                     1\n2026-01-01T00:00:00Z  ada\n");
    // And claiming -H over a file that has no header shows the second line,
    // for the same reason a run would convert from there: the option says a
    // header is present and is believed rather than second-guessed.
    csvdt()
        .args(["-H", "--peek"])
        .write_stdin("1,2,3\n4,5,6\n")
        .assert()
        .success()
        .stdout("0  1  2\n4  5  6\n");
}

#[test]
fn peek_without_a_header_shows_the_first_row_as_the_data_it_is() {
    // Two rows, not three with an invented header: without -H the first
    // record of the file is data, and calling it a header here would be
    // csvdt guessing at exactly the thing -H exists to be told.
    csvdt()
        .arg("--peek")
        .write_stdin("2026-01-01T00:00:00Z,ada\n2026-01-02T00:00:00Z,bob\n")
        .assert()
        .success()
        .stdout("0                     1\n2026-01-01T00:00:00Z  ada\n");
}

#[test]
fn peek_shows_a_ragged_file_rather_than_refusing_it() {
    // The refusal exists to stop uneven records reaching an output claiming
    // to be CSV. There is no such output here, and somebody peeking is often
    // peeking because the file is ragged. Both directions, since the
    // numbering has to cover the widest row shown either way.
    csvdt()
        .args(["-H", "-p", "--peek"])
        .write_stdin("a,b\n1,2,3,4\n")
        .assert()
        .success()
        .stdout("0  1  2  3\na  b\n1  2  3  4\n");
    csvdt()
        .args(["-H", "-p", "--peek"])
        .write_stdin("a,b,c,d\n1,2\n")
        .assert()
        .success()
        .stdout("0  1  2  3\na  b  c  d\n1  2\n");
}

#[test]
fn peek_keeps_a_field_that_holds_a_newline_on_one_line() {
    // A quoted field may hold a record terminator, and left alone one field
    // turns a three-row table into five with the numbering standing above
    // nothing -- the one thing this mode exists to get right.
    csvdt()
        .args(["-H", "-p", "--peek"])
        .write_stdin("a,b\n\"x\ny\",\"p\tq\"\n")
        .assert()
        .success()
        .stdout("0     1\na     b\nx\\ny  p\\tq\n");
}

#[test]
fn peek_reads_the_file_the_way_a_run_would() {
    // The columns numbered have to be the columns a run sees, so the options
    // that decide what a record is all apply.
    csvdt()
        .args(["-H", "-p", "--peek", "--read-delimiter", ";",
               "--comment", "#"])
        .write_stdin("#a note\nwhen;who\n2026-01-01T00:00:00Z;ada\n")
        .assert()
        .success()
        .stdout(
            "0                     1\n\
             when                  who\n\
             2026-01-01T00:00:00Z  ada\n",
        );
}

#[test]
fn peek_sees_the_same_fields_a_run_sees_under_every_reading_option() {
    // The help names four options that decide what a record is, and says all
    // four apply here. The test above pins what --peek prints under two of
    // them, which is a different question: it would pass just as well if a
    // run disagreed, since it never asks a run. --single-quote and --trim
    // were not asked about at all.
    //
    // So each dialect is put to both, and the answers compared. A run with
    // no action writes the records back as csvdt read them, which is the
    // list of fields to beat; --peek writes the same fields padded into a
    // table. Every fixture is built from tokens holding no space, comma or
    // quote, so both forms come apart again on nothing more than whitespace
    // and commas -- the padding is not what is under test here.
    //
    // Each dialect is one that changes the answer, so a build ignoring it
    // fails rather than passing on a file it would have read the same way:
    // without --single-quote the middle field keeps its quotes, without
    // --trim its spaces, without --comment the note is a record of its own,
    // and without --read-delimiter there is one field where there are three.
    for (dialect, input) in [
        (vec!["--read-delimiter", ";"], "a;b;c\n1;2;3\n"),
        (vec!["--single-quote"], "a,'bee',c\n1,2,3\n"),
        (vec!["--trim", "all"], "a , b ,c\n 1 , 2 ,3\n"),
        (vec!["--comment", "#"], "#a note\na,b,c\n1,2,3\n"),
    ] {
        let fields = |text: &str, split_on: &[char]| -> Vec<Vec<String>> {
            text.lines()
                .map(|line| {
                    line.split(split_on)
                        .filter(|field| !field.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .collect()
        };

        // What a run sees: the records written back, with no action asked
        // for, so nothing is added or moved.
        let run = csvdt()
            .args(&dialect)
            .args(["-H", "-p"])
            .write_stdin(input)
            .assert()
            .success();
        let seen = fields(
            std::str::from_utf8(&run.get_output().stdout).unwrap(),
            &[','],
        );

        // What --peek shows, less its numbering line.
        let peeked = csvdt()
            .args(&dialect)
            .args(["-H", "-p", "--peek"])
            .write_stdin(input)
            .assert()
            .success();
        let shown: Vec<Vec<String>> = fields(
            std::str::from_utf8(&peeked.get_output().stdout).unwrap(),
            &[' '],
        )
        .into_iter()
        .skip(1)
        .collect();

        assert_eq!(
            shown, seen,
            "with {dialect:?}, --peek numbers fields a run does not see",
        );
    }
}

#[test]
fn peek_tells_an_escape_in_the_data_from_one_it_wrote_itself() {
    // A tab is shown as '\t' so that one field stays one row. That leaves
    // two different fields with one spelling -- a real tab, and the two
    // characters a reader would type to mean one -- unless the backslash
    // that starts an escape is escaped too.
    //
    // Both are asked for at once, in neighbouring columns, since what is
    // wanted is not that either has some spelling but that they differ:
    // an escaper that stopped doubling the backslash would print the same
    // thing for both and no reader could tell which field held which.
    csvdt()
        .args(["-H", "-p", "--peek"])
        .write_stdin("real,typed\n\"x\ty\",\"x\\ty\"\n")
        .assert()
        .success()
        .stdout("0     1\nreal  typed\nx\\ty  x\\\\ty\n");
}

#[test]
fn peek_says_so_when_there_is_nothing_to_number() {
    // Not an error: an empty input is not one anywhere else here. Said
    // rather than left as an empty screen, which reads as a broken run.
    csvdt()
        .args(["-H", "--peek"])
        .write_stdin("")
        .assert()
        .success()
        .stdout("")
        .stderr(predicate::str::contains("nothing to peek at"));
}

#[test]
fn peek_refuses_the_options_that_write() {
    // There is no CSV output here for any of them to act on, so each is a
    // command line that cannot mean what it says.
    for argument in [
        vec!["-u0"],
        vec!["-r0"],
        vec!["-l0"],
        vec!["-s0"],
        vec!["-d0"],
        vec!["-o0,+02:00"],
        vec!["--remove", "0"],
        vec!["-a", "0"],
        vec!["-i", "0,after"],
        vec!["--round-seconds"],
        vec!["-f=fix"],
        vec!["--quote", "always"],
        vec!["--print-delimiter", ";"],
    ] {
        csvdt()
            .arg("--peek")
            .args(&argument)
            .write_stdin("a,b\n1,2\n")
            .assert()
            .code(2)
            .stderr(predicate::str::contains("cannot be used with"));
    }
}

#[test]
fn peek_numbers_the_columns_a_run_would_see() {
    // peek.txt: "Reads the file the way a run would: '--read-delimiter',
    // '--comment', '--single-quote' and '--trim' all apply, so the columns
    // are the ones a run would see." That is the claim the mode exists for --
    // somebody reads a column number off --peek and hands it to a run, and a
    // number that meant something else there converts the wrong column and
    // returns a plausible answer.
    //
    // True by construction today: peek_stream is handed the reader the run
    // built, so there are not two readers to disagree. Which is exactly why
    // it is worth asking. scan_widest_record had the same shape of guarantee
    // and nothing holding it, and every way of pulling that pair apart passed
    // the whole suite. A refactor giving peek a reader of its own -- to look
    // at a file without consuming the run's, say -- would diverge in silence.
    //
    // Asked as exact numbering rather than as agreement between the two. A
    // test that only compared them would pass a build whose reader was
    // misconfigured for both, which is the failure they would share.
    for (options, input, numbering, row) in [
        (vec!["--read-delimiter", ";"], "a;b;c\n1;2;3\n", "0  1  2", "a  b  c"),
        (vec!["--comment", "#"], "# note\na,b\n1,2\n", "0  1", "a  b"),
        (vec!["--single-quote"], "a,'b,c',d\n1,'2,3',4\n", "0  1    2", "a  b,c  d"),
        (vec!["--trim", "all"], "a , b , c\n1 , 2 , 3\n", "0  1  2", "a  b  c"),
    ] {
        let mut command = csvdt();
        command.arg("--peek");
        command.args(&options);
        let shown = String::from_utf8(
            command.write_stdin(input.to_string()).output().unwrap().stdout,
        )
        .expect("--peek should be UTF-8");
        let mut lines = shown.lines();
        assert_eq!(
            lines.next(), Some(numbering),
            "--peek {options:?} numbered the columns differently",
        );
        assert_eq!(
            lines.next(), Some(row),
            "--peek {options:?} read the row differently",
        );

        // And the count is the one a run produces from the same input under
        // the same options, which is the half that would catch peek keeping
        // its own reader while the run moved on.
        let mut run = csvdt();
        run.arg("-f");
        run.args(&options);
        let output = String::from_utf8(
            run.write_stdin(input.to_string()).output().unwrap().stdout,
        )
        .expect("a run should be UTF-8");
        // Counted by the writer's own quoting rules rather than by splitting
        // on commas: '--single-quote' turns a,'b,c',d into three fields, and
        // the middle one is written back quoted, where a naive split finds
        // four. Reading the output back through csvdt is the honest count.
        let widest = String::from_utf8(
            csvdt().arg("--peek").write_stdin(output).output().unwrap().stdout,
        )
        .expect("--peek should be UTF-8")
        .lines()
        .next()
        .expect("a numbering line")
        .split_whitespace()
        .count();
        assert_eq!(
            widest, numbering.split_whitespace().count(),
            "--peek {options:?} numbered {} columns, the run produced {widest}",
            numbering.split_whitespace().count(),
        );
    }
}

#[test]
fn every_option_is_either_welcome_with_peek_or_refused_by_it() {
    // The list above is written out by hand, which pins the message but leaves
    // a fourteenth option free to arrive and be neither refused nor thought
    // about. So the set comes from --list-options -- the binary's own account
    // of what it takes -- and everything in it must land on one side or the
    // other. A new option is refused by default here, and saying it should be
    // welcome means saying so below.
    let listed = String::from_utf8(
        csvdt().arg("--list-options").output().unwrap().stdout,
    )
    .expect("the option list should be UTF-8");

    // What --peek reads the file with, plus the two that say which record is
    // the header. Everything else writes, and has no CSV here to write to.
    //
    // Each comes with an input the option changes the reading of, and what
    // --peek must show for it. Exit 0 is not the question: an option accepted
    // and then ignored would pass that, and being accepted is worth nothing
    // here -- the whole promise is that the columns numbered are the ones a
    // run would see, which only holds if the option applied. So the same input
    // is peeked at again without it, and the two must differ.
    const WELCOME: [(&str, &[&str], &str, &str); 6] = [
        // With -H the first record is the header, so what is shown is the
        // first data row instead of it.
        ("--has-header", &["-H"], "a,b\n1,2\n3,4\n", "0  1\n1  2\n"),
        // And -p puts the header back above it, three rows rather than two.
        ("--print-header", &["-H", "-p"], "a,b\n1,2\n", "0  1\na  b\n1  2\n"),
        ("--trim", &["--trim", "all"], "  a  ,  b  \n  1  ,  2  \n",
         "0  1\na  b\n"),
        ("--read-delimiter", &["--read-delimiter", ";"], "a;b\n1;2\n",
         "0  1\na  b\n"),
        ("--single-quote", &["--single-quote"], "'a,b',c\n'1,2',3\n",
         "0    1\na,b  c\n"),
        ("--comment", &["--comment", "#"], "#note\na,b\n", "0  1\na  b\n"),
    ];
    // A value that any of them will accept, so a refusal is the conflict with
    // --peek rather than a value the option would have rejected anyway.
    const VALUES: [(&str, &str); 16] = [
        ("--flexible", "widest"), ("--insert", "0,replace"),
        ("--rfc3339", "0"), ("--utc", "0"), ("--local", "0"),
        ("--offset", "0,+02:00"), ("--split", "0"), ("--duration", "0,1"),
        ("--remove", "0"), ("--arrange", "1,0"), ("--trim", "all"),
        ("--read-delimiter", ";"), ("--comment", "#"),
        ("--print-delimiter", ";"), ("--quote", "always"),
        ("--fill-log", "-"),
    ];

    let mut welcome = 0;
    let mut refused = 0;
    for line in listed.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut columns = line.split('\t');
        let _short = columns.next();
        let long = columns.next().expect("a long name").to_string();
        let takes = columns.next().expect("a value column");
        // The modes that answer and exit are not part of a run at all, and
        // --peek is the one being asked about. --help and --version are among
        // them and win over whatever else is on the line rather than refusing
        // it, which is clap's convention and every other tool's.
        if matches!(long.as_str(),
                    "--peek" | "--list-options" | "--generate-man"
                    | "--help" | "--version") {
            continue;
        }

        if let Some((_, extra, input, expected)) =
            WELCOME.iter().find(|(name, ..)| *name == long)
        {
            welcome += 1;
            // Shown the way the option says, not merely accepted.
            let with = csvdt()
                .arg("--peek")
                .args(*extra)
                .write_stdin(*input)
                .assert()
                .success()
                .get_output()
                .stdout
                .clone();
            assert_eq!(
                String::from_utf8_lossy(&with), *expected,
                "--peek {extra:?} showed something else",
            );
            // And the same file without it, which must look different --
            // otherwise the case proves nothing about the option applying.
            let without = csvdt()
                .arg("--peek")
                .write_stdin(*input)
                .assert()
                .success()
                .get_output()
                .stdout
                .clone();
            assert_ne!(
                with, without,
                "{long} changed nothing about what --peek showed, so this \
                 case cannot tell an option that applies from one ignored",
            );
            continue;
        }

        let mut arguments = vec!["--peek".to_string(), long.clone()];
        if takes != "none" {
            let value = VALUES
                .iter()
                .find(|(name, _)| *name == long)
                .map(|(_, value)| *value)
                // Not skipped: an option needing a value with none given here
                // would fail for that reason and be counted as refused, which
                // is a pass this test has not earned.
                .unwrap_or_else(|| panic!(
                    "{long} takes a value and this test has none for it"));
            arguments.push(value.to_string());
        }
        refused += 1;
        csvdt()
            .args(&arguments)
            .write_stdin("a,b\n1,2\n")
            .assert()
            .code(2)
            .stderr(predicate::str::contains("cannot be used with"));
    }

    assert_eq!(welcome, WELCOME.len(), "not every welcome option was reached");
    assert_eq!(
        refused, 14,
        "--peek's conflict list has changed; peek.txt names the options it \
         refuses and needs to say the same",
    );
}

#[test]
fn peek_reads_no_more_of_the_file_than_it_shows() {
    // A row beyond the first that no reader could finish: reaching it would
    // be an error, and not reaching it is the claim -- the cost of peeking
    // at a huge file is its first record, not the file.
    csvdt()
        .args(["-H", "-p", "--peek"])
        .write_stdin("a,b\n1,2\n\"unclosed,3\n")
        .assert()
        .success()
        .stdout("0  1\na  b\n1  2\n");
}

// ---- --list-options: the machine-readable option contract ----------------

// Splits --list-options' output into its records, dropping the comments.
fn option_list() -> Vec<Vec<String>> {
    let output = csvdt().arg("--list-options").output().unwrap();
    assert!(output.status.success(), "--list-options failed");
    String::from_utf8(output.stdout)
        .expect("--list-options output is not UTF-8")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split('\t').map(str::to_string).collect())
        .collect()
}

#[test]
fn list_options_names_its_format_version_first() {
    // A consumer reads the fields positionally, so it needs a way to tell that
    // they still mean what it expects.
    csvdt()
        .arg("--list-options")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("# csvdt-option-list 1\n"));
}

#[test]
fn list_options_covers_every_long_option_in_help() {
    // The real point of the test: --help and --list-options are rendered
    // independently, so comparing them catches an option that reaches one and
    // not the other. Nothing here lists the options by name -- they come from
    // the binary, which is what makes this a check rather than a restatement.
    let help = csvdt().arg("--help").output().unwrap();
    let help = String::from_utf8(help.stdout).unwrap();

    // clap indents every option entry and begins it with the short form when
    // there is one, then the long. Wrapped description lines are indented much
    // further and begin with prose, so a '-' immediately after the indent is
    // what marks an entry -- which is also why the description bodies here can
    // mention --help without being mistaken for one.
    let mut in_help = std::collections::BTreeSet::new();
    for line in help.lines() {
        if !line.starts_with("  ") {
            continue;
        }
        let entry = line.trim_start();
        let long = match entry.strip_prefix("--") {
            Some(rest) => Some(rest),
            None if entry.starts_with('-') => entry.split_once(", --").map(|(_, rest)| rest),
            None => None,
        };
        if let Some(long) = long {
            in_help.insert(format!(
                "--{}",
                long.split([' ', '=', '<', '[']).next().unwrap()
            ));
        }
    }

    let listed: std::collections::BTreeSet<String> = option_list()
        .iter()
        .filter(|fields| !fields[1].is_empty())
        .map(|fields| fields[1].clone())
        .collect();

    assert!(!in_help.is_empty(), "found no long options in --help");
    assert_eq!(
        in_help, listed,
        "--help and --list-options disagree about which options exist"
    );
}

#[test]
fn list_options_records_are_five_well_formed_fields() {
    for fields in option_list() {
        assert_eq!(fields.len(), 5, "wrong field count in {:?}", fields);
        let (short, long, value, separator, name) =
            (&fields[0], &fields[1], &fields[2], &fields[3], &fields[4]);

        assert!(
            !short.is_empty() || !long.is_empty(),
            "record with neither form: {:?}",
            fields
        );
        if !short.is_empty() {
            assert!(short.starts_with('-') && short.len() == 2, "bad short {:?}", short);
        }
        if !long.is_empty() {
            assert!(long.starts_with("--"), "bad long {:?}", long);
        }
        assert!(
            ["none", "required", "optional"].contains(&value.as_str()),
            "bad value field {:?}",
            value
        );
        // A flag has no value, so it can have neither a separator nor a
        // placeholder; anything taking a value must report both.
        if value == "none" {
            assert_eq!(separator, "none", "flag with a separator: {:?}", fields);
            assert!(name.is_empty(), "flag with a value name: {:?}", fields);
        } else {
            assert!(
                ["equals", "either"].contains(&separator.as_str()),
                "bad separator {:?}",
                separator
            );
            assert!(!name.is_empty(), "value option with no value name: {:?}", fields);
        }
    }
}

#[test]
fn list_options_describes_how_a_value_must_be_attached() {
    // --flexible is the one option whose value must be attached with '=', and
    // the one whose value is optional. A wrapper that offered "-f fix" would
    // build a command line csvdt reads as a filename, so this is exactly the
    // kind of thing the list exists to convey.
    let flexible = option_list()
        .into_iter()
        .find(|fields| fields[1] == "--flexible")
        .expect("--flexible missing from the option list");

    assert_eq!(flexible[2], "optional");
    assert_eq!(flexible[3], "equals");
    assert_eq!(flexible[4], "keep|fix|widest");

    // Everything else takes its value either way, so the distinction is real.
    for fields in option_list() {
        if fields[1] != "--flexible" {
            assert_ne!(fields[3], "equals", "unexpected equals-only option: {:?}", fields);
        }
    }
}

#[test]
fn list_options_includes_the_automatic_help_and_version() {
    // clap adds these rather than the source declaring them, but a wrapper
    // completing option names has no reason to care about the difference.
    let listed: Vec<String> = option_list().iter().map(|f| f[1].clone()).collect();
    assert!(listed.contains(&"--help".to_string()));
    assert!(listed.contains(&"--version".to_string()));
    assert!(listed.contains(&"--list-options".to_string()));
}

#[test]
fn list_options_stands_alone_and_reads_no_input() {
    // It must answer on a bare invocation, since a wrapper calls it before it
    // knows anything -- and combining it with a run would be ambiguous.
    csvdt()
        .args(["--list-options", "-H"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));

    // Given a file it would otherwise process, it still just answers.
    csvdt()
        .args(["--list-options"])
        .write_stdin("a,b\n0,1\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("--has-header"))
        .stdout(predicate::str::contains("0,1").not());
}

#[test]
fn help_documents_the_option_list_format() {
    // The format is a contract, so it is described where the tool describes
    // everything else rather than only in a commit message.
    csvdt()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("machine-readable"))
        .stdout(predicate::str::contains("tab-separated"));
}

#[test]
fn empty_input_produces_no_header_line() {
    // The CSV writer renders a record with no fields as `""`, so writing a
    // header that was never read put a field in the output that the input did
    // not have. Empty input and input that is nothing but blank lines both
    // reach the reader as no record at all.
    for input in ["", "\n", "\n\n\n"] {
        csvdt()
            .args(["-H", "-p"])
            .write_stdin(input)
            .assert()
            .success()
            .stdout("");
    }
}

#[test]
fn a_header_of_one_empty_field_still_round_trips() {
    // The distinction the fix rests on: one empty field is a field, and has to
    // survive, where no fields at all is nothing to print.
    csvdt()
        .args(["-H", "-p"])
        .write_stdin("\"\"\n")
        .assert()
        .success()
        .stdout("\"\"\n");

    csvdt()
        .args(["-H", "-p"])
        .write_stdin(",\n1,2\n")
        .assert()
        .success()
        .stdout(",\n1,2\n");
}

#[test]
fn empty_input_is_still_empty_under_every_filling_mode() {
    // The filling modes pad records out to a width, and an absent header must
    // not become a padded one.
    for mode in ["-f=keep", "-f=fix", "-f=widest"] {
        csvdt()
            .args(["-H", "-p", mode])
            .write_stdin("")
            .assert()
            .success()
            .stdout("");
    }
}

#[test]
fn rfc3339_refuses_epochs_outside_the_formats_own_range() {
    // RFC3339's date-fullyear is four digits, so 9999 is the last year there
    // is. chrono reaches far beyond and falls back to ISO 8601's expanded form
    // outside that -- "+57534-06-16T00:00:00+00:00" -- which is a different
    // format wearing the same shape: csvdt cannot read it back, and nor can
    // anything else expecting RFC3339.
    //
    // The floor is AD 1 rather than 0000, which four digits would hold: there
    // is no year zero to record, so a value landing there came from something
    // other than a date.
    for (epoch, expected) in [
        ("253402300799", "9999-12-31T23:59:59+00:00"),  // last representable
        ("253402300800", "parse_err"),                  // one second past it
        ("-62135596800", "0001-01-01T00:00:00+00:00"),  // first representable
        ("-62135596801", "parse_err"),                  // one second before it
        ("-62167219200", "parse_err"),                  // 0000-01-01, no such year
    ] {
        csvdt()
            .args(["-H", "-p", "-r0", "-i", "0,replace"])
            // Epoch 0 alongside, so a case whose own value is out of range is
            // still a run that converted something.
            .write_stdin(format!("ts\n0\n{}\n", epoch))
            .assert()
            .success()
            .stdout(format!(
                "to_rfc3339\n1970-01-01T00:00:00+00:00\n{}\n", expected));
    }
}

#[test]
fn the_help_names_the_epoch_range_it_actually_has() {
    // The floor was named and explained and the ceiling was not, so a caller
    // scripting against -r had one end documented and had to find the other by
    // trial. Both are named now, and both numbers are taken out of the help
    // rather than written here again: the pair cannot drift apart, because a
    // help that named the wrong number would fail this on the wrong number.
    // Collapsed, because the corpus is wrapped by hand: the phrase anchoring
    // each number sits across a line break whenever the wrapping moves.
    let help = help_words();

    // Anchored on the whole clause rather than on "up to ", which is ordinary
    // English and turns up elsewhere in the corpus. It did: a paragraph added
    // to --flexible said "up to that point", --flexible is printed before
    // --rfc3339, and this test then read that phrase instead of the number --
    // failing, correctly, but on a bare ParseIntError that says nothing about
    // what happened. The clause is what makes the anchor the sentence's own.
    let named = |after: &str| -> i64 {
        let tail = help
            .split(after)
            .nth(1)
            .unwrap_or_else(|| panic!("the help no longer says {after:?}"));
        let word = tail
            .trim_start()
            .split(|c: char| !(c.is_ascii_digit() || c == '-'))
            .find(|word| !word.is_empty())
            .unwrap_or_else(|| panic!("no number follows {after:?}"));
        word.parse().unwrap_or_else(|_| {
            panic!("{after:?} is followed by {word:?}, which is not a number")
        })
    };
    let floor = named("are accepted, down to ");
    let ceiling = named(", and up to ");
    assert!(floor < 0 && ceiling > 0, "floor {floor}, ceiling {ceiling}");

    // Each end converts, and one step past it does not. Epoch 0 rides along so
    // a row whose own value is out of range is still a run that converted.
    for (epoch, converts) in
        [(floor, true), (floor - 1, false), (ceiling, true), (ceiling + 1, false)]
    {
        let marker = predicate::str::contains("parse_err");
        let assertion = csvdt()
            .args(["-H", "-p", "-r0", "-i", "0,replace"])
            .write_stdin(format!("ts\n0\n{epoch}\n"))
            .assert()
            .success();
        if converts {
            assertion.stdout(marker.not());
        } else {
            assertion.stdout(marker);
        }
    }

    // And the dates the help pairs them with are the dates they really are.
    for (epoch, date) in [(floor, "0001-01-01T00:00:00"), (ceiling, "9999-12-31T23:59:59")] {
        assert!(help.contains(date), "the help no longer pairs {epoch} with {date}");
        csvdt()
            .args(["-H", "-p", "-r0", "-i", "0,replace"])
            .write_stdin(format!("ts\n{epoch}\n"))
            .assert()
            .success()
            .stdout(format!("to_rfc3339\n{date}+00:00\n"));
    }
}

#[test]
fn the_t_and_the_z_are_read_in_either_case() {
    // RFC3339 permits either case outright. The help documented the two
    // leniencies csvdt adds -- an offset missing its colon, a space where the
    // 'T' belongs -- and said nothing about the one the format itself grants.
    for written in ["2024-06-15T14:00:00Z", "2024-06-15t14:00:00Z",
                    "2024-06-15T14:00:00z", "2024-06-15t14:00:00z"] {
        csvdt()
            .args(["-H", "-p", "-u0", "-i", "0,replace"])
            .write_stdin(format!("ts\n{written}\n"))
            .assert()
            .success()
            // Upper-case 'T' on the way out, and the offset spelled out rather
            // than written Z, so a column of results is one width.
            .stdout("to_utc\n2024-06-15T14:00:00+00:00\n");
    }
}

#[test]
fn an_offset_is_read_wider_than_it_can_be_written() {
    // A file may carry an offset no zone uses, and reading it is exact
    // arithmetic either way -- so it converts. Naming the same offset as a
    // target for -o does not, since that would write a record no real clock
    // reads. The two ranges differ, which the help now says.
    for (offset, utc) in [("+14:00", "2024-06-15T00:00:00+00:00"),
                          ("+23:59", "2024-06-14T14:01:00+00:00"),
                          ("-23:59", "2024-06-16T13:59:00+00:00")] {
        csvdt()
            .args(["-H", "-p", "-u0", "-i", "0,replace"])
            .write_stdin(format!("ts\n\"2024-06-15T14:00:00{offset}\"\n"))
            .assert()
            .success()
            .stdout(format!("to_utc\n{utc}\n"));
    }

    // A whole day is not an offset, on either side.
    for offset in ["+24:00", "-24:00"] {
        csvdt()
            .args(["-H", "-p", "-u0", "-i", "0,replace"])
            .write_stdin(format!("ts\n\"2024-06-15T14:00:00{offset}\"\n"))
            .assert()
            .failure()
            .stdout("to_utc\nparse_err\n");
    }

    // And the narrower range -o holds a target to, one step outside each end.
    for target in ["+14:01", "-12:01", "+23:59"] {
        csvdt()
            .args(["-H", "-p", &format!("-o0,{target}")])
            .write_stdin("ts\n2024-06-15T14:00:00Z\n")
            .assert()
            .failure()
            .stderr(predicate::str::contains("is not a real UTC offset"))
            .stderr(predicate::str::contains("-12:00 to +14:00"));
    }
}

#[test]
fn a_fraction_is_read_to_nine_digits_and_written_at_three_six_or_nine() {
    // The width changes on the way out -- .5 comes back .500 -- which is
    // visible in every such row and was stated nowhere. Nine digits is the
    // limit, and past it the value is refused rather than quietly shortened,
    // which is the difference between a timestamp and a rounded one.
    for (given, written) in [
        (".5", ".500"), (".50", ".500"), (".500", ".500"),
        (".1234", ".123400"), (".123456", ".123456"),
        (".1234567", ".123456700"), (".123456789", ".123456789"),
        (".000000001", ".000000001"),
        // A fraction of nothing is not a fraction, so none is written.
        (".0", ""),
    ] {
        csvdt()
            .args(["-H", "-p", "-u0", "-i", "0,replace"])
            .write_stdin(format!("ts\n2024-06-15T14:00:00{given}Z\n"))
            .assert()
            .success()
            .stdout(format!("to_utc\n2024-06-15T14:00:00{written}+00:00\n"));
    }

    // One digit past nine, and a fraction that is only a point.
    for given in [".1234567891", ".000000000001", "."] {
        csvdt()
            .args(["-H", "-p", "-u0", "-i", "0,replace"])
            .write_stdin(format!("ts\n2024-06-15T14:00:00{given}Z\n"))
            .assert()
            .failure()
            .stdout("to_utc\nparse_err\n");
    }
}

#[test]
fn rfc3339_reports_epoch_milliseconds_rather_than_answering_with_a_useless_year() {
    // Handing epoch milliseconds to an option expecting seconds is an easy
    // mistake with log data, and it used to produce a year-57534 timestamp
    // that exited 0 and looked like an answer.
    csvdt()
        .args(["-H", "-p", "-r0", "-i", "0,replace"])
        .write_stdin("ts\n1753444800000\n1753444800\n")
        .assert()
        .success()
        .stdout("to_rfc3339\nparse_err\n2025-07-25T12:00:00+00:00\n");

    // A whole column of them fails the run, and the message sends the reader to
    // TIMESTAMP FORMATS -- which for a long time said nothing about the
    // likeliest cause of getting there. It names it now, so the pointer leads
    // somewhere.
    csvdt()
        .args(["-H", "-p", "-r0", "-i", "0,replace"])
        .write_stdin("ts\n1753444800000\n1753444801000\n")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("nothing converted"))
        .stderr(predicate::str::contains("TIMESTAMP FORMATS"));

    let help = String::from_utf8(csvdt().arg("--help").output().unwrap().stdout)
        .expect("the help should be UTF-8");
    assert!(help.contains("Millisecond"), "TIMESTAMP FORMATS no longer names it");
    assert!(help.contains("Divide by 1000"), "nor says what to do about it");
}

#[test]
fn every_timestamp_rfc3339_writes_can_be_read_back() {
    // The property the range check exists for: whatever -r emits, -u must be
    // able to parse. Spans the whole representable range and past both ends.
    let epochs = [
        i64::MIN, -377705116800, -62167219201, -62167219200, -1, 0, 1,
        1753444800, 253402300799, 253402300800, 1753444800000, i64::MAX,
    ];
    let input: String = std::iter::once("ts".to_string())
        .chain(epochs.iter().map(|e| e.to_string()))
        .collect::<Vec<_>>()
        .join("\n");

    let written = csvdt()
        .args(["-H", "-p", "-r0", "-i", "0,replace"])
        .write_stdin(format!("{}\n", input))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let written = String::from_utf8(written).unwrap();

    // Feeding that straight back must leave every non-marker value intact.
    let reread = csvdt()
        .args(["-H", "-p", "-u0", "-i", "0,replace"])
        .write_stdin(written.clone())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let reread = String::from_utf8(reread).unwrap();

    for (out, back) in written.lines().skip(1).zip(reread.lines().skip(1)) {
        if out == "parse_err" {
            continue;
        }
        assert_eq!(
            out, back,
            "csvdt wrote {:?} but reading it back gave {:?}", out, back
        );
    }
}

#[test]
fn an_input_that_cannot_be_read_is_named_in_the_error() {
    // The bare io error said only "No such file or directory (os error 2)". A
    // script handed the wrong path had nothing to tell it which path that was.
    let dir = tempfile_dir().join("named-input-errors");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let missing = dir.join("nope.csv");

    csvdt()
        .args(["-H", "-p"])
        .arg(&missing)
        .assert()
        .failure()
        .code(1)
        // The path is what csvdt adds and so what this pins. The reason after
        // it is the operating system's own wording -- "No such file or
        // directory" on Unix, "The system cannot find the file specified" on
        // Windows -- and not ours to assert.
        .stderr(predicate::str::contains(format!(
            "cannot read '{}'", missing.display())));

    // A directory used to fail at the first read, as "Is a directory" from
    // somewhere inside the CSV reader, also without naming it. The two
    // platforms differ underneath -- Unix opens a directory and fails on the
    // read, Windows refuses the open as access denied -- so the diagnosis is
    // made on this side and the message is the same on both.
    csvdt()
        .args(["-H", "-p"])
        .arg(&dir)
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("it is a directory, not a file"));

    // --flexible=widest opens the input on its own path, through the spool, so
    // it has to name it too.
    csvdt()
        .args(["-H", "-p", "-f=widest"])
        .arg(&missing)
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(format!(
            "cannot read '{}'", missing.display())));

    // csvdt does not take '-' for standard input -- it reads standard input
    // when no file is named -- so this is a file that is not there, and the
    // message now says which one rather than leaving it to be guessed.
    csvdt()
        .args(["-H", "-p", "-"])
        .write_stdin("a,b\n1,2\n")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("cannot read '-'"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_output_is_canonical_csv_rather_than_a_copy_of_the_input() {
    // Records are parsed and written again, so a run that changes nothing about
    // the data still normalises how it is written. KNOWN LIMITS says so, since
    // it means the output is not byte-comparable with the input.
    for (input, expected) in [
        ("a,b\r\n1,2\r\n", "a,b\n1,2\n"),          // CRLF becomes LF
        ("a,b\n1,2", "a,b\n1,2\n"),                  // a final newline is added
        ("\"a\",\"b\"\n\"1\",\"2\"\n", "a,b\n1,2\n"),  // needless quotes go
    ] {
        csvdt()
            .args(["-H", "-p"])
            .write_stdin(input)
            .assert()
            .success()
            .stdout(expected)
            .stderr("");
    }

    // Quoting is how a CSV says whitespace is data, and it is kept. This used
    // to be the one normalisation that changed a value rather than its
    // spelling: --trim defaulted to all and reached inside the quotes, so " 1 "
    // arrived as 1 and a field of nothing but spaces arrived empty.
    csvdt()
        .write_stdin("\" 1 \",x\n")
        .assert()
        .success()
        .stdout(" 1 ,x\n");

    // The spaces survive; the quotes go, being unnecessary around a field that
    // holds no delimiter, quote or newline. That round-trips now -- reading it
    // back gives the three spaces again -- which it would not have while the
    // reader trimmed by default.
    csvdt()
        .write_stdin("\"   \",x\n")
        .assert()
        .success()
        .stdout("   ,x\n");

    csvdt()
        .write_stdin("   ,x\n")
        .assert()
        .success()
        .stdout("   ,x\n");

    // Trimming is still there for anyone who wants it.
    csvdt()
        .args(["--trim", "all"])
        .write_stdin("\" 1 \",x\n")
        .assert()
        .success()
        .stdout("1,x\n");
}

#[test]
fn a_field_holding_a_newline_survives_being_written_again() {
    // The normalisations above must not reach a newline that is data: it stays,
    // and the field stays quoted because that is the only way to write it.
    csvdt()
        .args(["-H", "-p", "-u0"])
        .write_stdin("ts,b\n2024-01-01T00:00:00Z,\"x\ny\"\n")
        .assert()
        .success()
        .stdout("ts,<to_utc,b\n2024-01-01T00:00:00Z,2024-01-01T00:00:00+00:00,\"x\ny\"\n");
}

#[test]
fn round_seconds_without_duration_is_refused_rather_than_ignored() {
    // clap is told --round-seconds requires --duration, and holds it to that
    // when the flag is alone -- but not alongside another action, because that
    // action conflicts with --duration and the requirement is dropped with it.
    // So `-u0 --round-seconds` was accepted and did nothing at all: -u wrote
    // .600 back out, and the run looked like it had rounded.
    for arguments in [
        vec!["-H", "-p", "-u0", "--round-seconds"],
        vec!["-H", "-p", "-l0", "--round-seconds"],
        vec!["-H", "-p", "-s0", "--round-seconds"],
        vec!["-H", "-p", "-o0,+01:00", "--round-seconds"],
        vec!["-H", "-p", "--remove", "0", "--round-seconds"],
    ] {
        csvdt()
            .args(&arguments)
            .write_stdin("ts\n2024-01-01T00:00:00.6Z\n")
            .assert()
            .failure()
            .code(100)
            .stderr(predicate::str::contains("there is no '-d' here"));
    }

    // With -d it is what it says it is.
    csvdt()
        .args(["-H", "-p", "-d0", "--round-seconds"])
        .write_stdin("ts\n2024-01-01T00:00:00.6Z\n2024-01-01T00:00:02.1Z\n")
        .assert()
        .success()
        .stdout("ts,<duration\n2024-01-01T00:00:00.6Z,PT0S\n2024-01-01T00:00:02.1Z,PT2S\n");
}

#[test]
fn a_column_number_that_will_not_parse_says_which_option_and_what_it_got() {
    // These reported Rust's own ParseIntError -- "invalid digit found in
    // string", "cannot parse integer from empty string" -- which named neither
    // the option nor the value. `-a "1, 2"` is an easy thing to type, and being
    // told a digit was invalid does not point at the space.
    for (option, value) in [
        ("-a", "1, 2"),
        ("--remove", "0, 2"),
        ("-u", " 0"),
        ("-d", "0, 1"),
    ] {
        csvdt()
            .args(["-H", "-p", option, value])
            .write_stdin("a,b,c\n0,1,2\n")
            .assert()
            .failure()
            .code(100)
            .stderr(predicate::str::contains("contains a space"));
    }

    // A stray comma, and an unset shell variable, both arrive as empty.
    for value in ["1,", "", ",", "0,,2"] {
        csvdt()
            .args(["-H", "-p", "-a", value])
            .write_stdin("a,b,c\n0,1,2\n")
            .assert()
            .failure()
            .code(100)
            .stderr(predicate::str::contains("empty column number"))
            .stderr(predicate::str::contains("unset shell variable"));
    }

    // And the option is named in each, whichever one it was.
    for (option, expected) in [
        ("-s", "'--split'"),
        ("-r", "'--rfc3339'"),
        ("-u", "'--utc'"),
        ("-l", "'--local'"),
        ("--remove", "'--remove'"),
        ("-a", "'--arrange'"),
        ("-d", "'--duration'"),
    ] {
        csvdt()
            .args(["-H", "-p", option, "x"])
            .write_stdin("a,b,c\n0,1,2\n")
            .assert()
            .failure()
            .code(100)
            .stderr(predicate::str::contains(expected))
            .stderr(predicate::str::contains("not a column number"));
    }

    // -o and -i take the column as part of a pair, and name themselves too.
    csvdt()
        .args(["-H", "-p", "-o", "x,+01:00"])
        .write_stdin("a,b,c\n0,1,2\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("'--offset' was given \"x\""));

    csvdt()
        .args(["-H", "-p", "-u0", "-i", "x,replace"])
        .write_stdin("a,b,c\n0,1,2\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("'--insert' was given \"x\""));
}

#[test]
fn a_column_number_too_large_says_so_rather_than_naming_a_target_type() {
    // "number too large to fit in target type" describes a usize, which is not
    // something the command line mentions anywhere.
    csvdt()
        .args(["-H", "-p", "-u", "99999999999999999999"])
        .write_stdin("a,b\n0,1\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("too large to be a column number"));
}

#[test]
fn a_negative_column_says_columns_start_at_zero_except_for_insert() {
    // -i clamps a negative column into the row rather than refusing it, so its
    // message must not claim there is no such thing.
    csvdt()
        .args(["-H", "-p", "-u=-1"])
        .write_stdin("a,b\n0,1\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("counted from 0 upwards"));

    csvdt()
        .args(["-H", "-p", "-u0", "-i=-1,before"])
        .write_stdin("ts,b\n2024-01-01T00:00:00Z,1\n")
        .assert()
        .success()
        .stdout("to_utc>,ts,b\n2024-01-01T00:00:00+00:00,2024-01-01T00:00:00Z,1\n");
}

#[test]
fn a_single_non_ascii_character_is_refused_as_being_non_ascii() {
    // The check used to count bytes rather than characters, so a single
    // multi-byte character was reported as a string -- which is not what is
    // wrong with it. It cannot be used because the CSV reader and writer work
    // in bytes, and the message now says so.
    for option in ["--read-delimiter", "--comment", "--print-delimiter"] {
        csvdt()
            .args([option, "é"])
            .write_stdin("a,b\n1,2\n")
            .assert()
            .failure()
            .code(100)
            .stderr(predicate::str::contains("expects an ASCII character"));
    }
}

#[test]
fn an_empty_delimiter_is_refused_rather_than_silently_ignored() {
    // An empty value used to leave the default in place and carry on, so
    // `--read-delimiter "$DELIM"` with DELIM unset read the file as
    // comma-separated and said nothing. That is noticed several steps later,
    // if at all.
    for option in ["--read-delimiter", "--comment", "--print-delimiter"] {
        csvdt()
            .args([option, ""])
            .write_stdin("a;b\n1;2\n")
            .assert()
            .failure()
            .code(100)
            .stderr(predicate::str::contains("empty value"));
    }
}

#[test]
fn trim_is_matched_case_insensitively_like_every_other_value() {
    // --trim ALL passed clap, which is told to ignore case, and then matched
    // none of the hand-written lowercase arms behind it -- so it fell through
    // and left the reader's own default, which is Trim::None. Asking for ALL
    // therefore got the opposite: whitespace kept, in silence, on its way into
    // timestamp parsing. --quote had the to_lowercase its twin was missing.
    for value in ["all", "ALL", "All", "aLL"] {
        csvdt()
            .args(["-H", "-p", "--trim", value])
            .write_stdin("a , b\n 1 , 2 \n")
            .assert()
            .success()
            .stdout("a,b\n1,2\n");
    }

    for (value, expected) in [
        ("FIELDS", "a , b\n1,2\n"),
        ("Headers", "a,b\n 1 , 2 \n"),
        ("NONE", "a , b\n 1 , 2 \n"),
    ] {
        csvdt()
            .args(["-H", "-p", "--trim", value])
            .write_stdin("a , b\n 1 , 2 \n")
            .assert()
            .success()
            .stdout(expected);
    }
}

#[test]
fn abbreviations_are_accepted_for_trim_and_quote_but_not_ambiguous_ones() {
    // Both options take any abbreviation that belongs to one value only.
    for (value, expected) in [
        ("a", "a,b\n1,2\n"),        // all
        ("f", "a , b\n1,2\n"),      // fields
        ("h", "a,b\n 1 , 2 \n"),    // headers
        ("n", "a , b\n 1 , 2 \n"),  // none
    ] {
        csvdt()
            .args(["-H", "-p", "--trim", value])
            .write_stdin("a , b\n 1 , 2 \n")
            .assert()
            .success()
            .stdout(expected);
    }

    // 'n' belongs to three of --quote's values, so it is refused rather than
    // resolved to whichever one happens to be listed first.
    csvdt()
        .args(["-H", "-p", "--quote", "n"])
        .write_stdin("a,b\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value 'n'"));

    // 'nev' belongs to never alone, so it is fine.
    csvdt()
        .args(["-H", "-p", "--quote", "nev"])
        .write_stdin("a,b\nx;y,2\n")
        .assert()
        .success()
        .stdout("a,b\nx;y,2\n");
}

#[test]
fn every_option_taking_a_word_takes_it_in_any_case_and_abbreviated() {
    // --trim and --quote did, -i did, --flexible did not: -f=FIX was refused
    // while --trim ALL was accepted and then quietly meant something else.
    // All four now resolve the same way, against one list each.
    for mode in ["fix", "FIX", "Fix", "f"] {
        csvdt()
            .args(["-H", "-p", &format!("-f={mode}")])
            .write_stdin("a,b,c\n0,1\n")
            .assert()
            .success()
            .stdout("a,b,c\n0,1,\n");
    }

    for position in ["replace", "REPLACE", "Replace", "rep", "r"] {
        csvdt()
            .args(["-H", "-p", "-u0", &format!("-i0,{position}")])
            .write_stdin("ts,b\n2024-01-01T00:00:00Z,2\n")
            .assert()
            .success()
            .stdout("to_utc,b\n2024-01-01T00:00:00+00:00,2\n");
    }

    // And a word that is not one of them is still refused, not defaulted.
    csvdt()
        .args(["-H", "-p", "-u0", "-i0,sideways"])
        .write_stdin("ts,b\n2024-01-01T00:00:00Z,2\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("before, after, or replace"));

    csvdt()
        .args(["-H", "-p", "-f=nonsense"])
        .write_stdin("a,b,c\n0,1\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value 'nonsense'"));
}

#[test]
fn a_record_terminator_is_refused_as_a_delimiter_or_comment_character() {
    // A newline or carriage return ends a record, so nothing within one can be
    // marked by it. `--read-delimiter` with a newline used to make the whole
    // file a single record, and `--print-delimiter` with one wrote fields and
    // rows the same way, so the output could not be read back at all.
    for option in ["--read-delimiter", "--print-delimiter", "--comment"] {
        for character in ["\n", "\r"] {
            csvdt()
                .args(["-H", "-p", option, character])
                .write_stdin("a,b\n1,2\n")
                .assert()
                .failure()
                .code(100)
                .stderr(predicate::str::contains("ends a record"));
        }
    }
}

#[test]
fn a_delimiter_that_is_also_the_quote_character_is_refused() {
    // Quoting is the only way to write a field that holds the delimiter, so if
    // they are the same character there is no way to write one. It showed:
    // `--print-delimiter '"'` turned the row x"y,2 into "x""y""2, which reads
    // back as one field, not two.
    csvdt()
        .args(["-H", "-p", "--print-delimiter", "\""])
        .write_stdin("a,b\nx\"y,2\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("quote character in use for writing"));

    csvdt()
        .args(["-H", "-p", "--read-delimiter", "\""])
        .write_stdin("a\"b\n1\"2\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("quote character in use"));
}

#[test]
fn which_quote_character_is_refused_follows_single_quote() {
    // --single-quote changes what the reader quotes with, so it changes which
    // character cannot also be the delimiter -- and frees the other one.
    csvdt()
        .args(["-H", "-p", "--single-quote", "--read-delimiter", "'"])
        .write_stdin("a,b\n1,2\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("because of '--single-quote'"));

    // '"' is just a character to the reader now, so it can separate fields.
    csvdt()
        .args(["-H", "-p", "--single-quote", "--read-delimiter", "\""])
        .write_stdin("a\"b\n1\"2\n")
        .assert()
        .success()
        .stdout("a,b\n1,2\n");

    // And without the flag it is the single quote that is free.
    csvdt()
        .args(["-H", "-p", "--read-delimiter", "'"])
        .write_stdin("a'b\n1'2\n")
        .assert()
        .success()
        .stdout("a,b\n1,2\n");
}

#[test]
fn a_comment_character_that_would_swallow_data_rows_is_refused() {
    // A comment character shares the start of a line with the data. As the
    // delimiter it made a comment of every row with an empty first field; as
    // the quote character, of every row starting with a quoted field. Both
    // dropped rows and said nothing -- `--comment ,` on a,b/,2/3,4 returned
    // two rows out of three.
    csvdt()
        .args(["-H", "-p", "--comment", ","])
        .write_stdin("a,b\n,2\n3,4\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("first field is empty"));

    // Named for the read side, so asserted on the read side's own words:
    // "starting with a quoted field" is in the write-side message too, and
    // with the read-side branch gone this invocation simply falls through to
    // that one -- still exit 100, still that phrase, nothing tested.
    csvdt()
        .args(["-H", "-p", "--comment", "\""])
        .write_stdin("a,b\n\"1\",2\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("the quote character being read"));
}

#[test]
fn a_comment_that_is_the_read_quote_is_refused_by_the_read_side_check() {
    // Neither of these reaches a write-side refusal, so the read side is on
    // its own here. Without it both are accepted and the quoted row is read
    // as a comment: exit 0, one row short, and nothing said about it.
    csvdt()
        .args(["-H", "-p", "--single-quote", "--comment", "'"])
        .write_stdin("a,b\n'q',2\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("the quote character being read"));

    // '"' as the comment character, with the write-side refusal stood down:
    // 'never' quotes nothing, so no printed row can gain a leading '"' the
    // field did not hold. The way in is still lost, which is this branch.
    csvdt()
        .args(["-H", "-p", "--comment", "\"", "--quote", "never"])
        .write_stdin("a,b\n\"q\",2\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("the quote character being read"));
}

#[test]
fn a_comment_character_that_would_comment_out_written_rows_is_refused() {
    // The same two collisions exist on the way out, and neither is caught by
    // the tally: it inspects the first field, and these lines gain the
    // comment character from the delimiter or quote written in front of it.
    // `--comment '#' --print-delimiter '#'` wrote the row ,2 as #2 -- a line
    // that starts with the comment character though no field holds one -- so
    // the run exited 0, warned about nothing, and reading the output back
    // with the same --comment dropped the row.
    csvdt()
        .args(["-H", "-p", "--comment", "#", "--print-delimiter", "#"])
        .write_stdin("a,b\n,2\n3,4\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("delimiter being written"));

    // --single-quote frees '"' for reading, but the writer always quotes
    // with it, so any printed row starting with a quoted field would begin
    // with the comment character.
    csvdt()
        .args(["-H", "-p", "--single-quote", "--comment", "\""])
        .write_stdin("a,b\n1,2\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("quote character used for writing"));
}

#[test]
fn the_write_side_refusals_stand_down_under_a_quoting_style_without_the_gap() {
    // The refusals above exist because a written line can begin with the
    // comment character without any field holding one. Whether it can depends
    // on the quoting style: 'always' and 'nonnumeric' write an empty first
    // field as "", so no line begins with the bare delimiter, and 'never'
    // quotes nothing, so none begins with a quote the data did not hold.
    // Those pairings worked before the refusal existed and must keep working.
    csvdt()
        .args(["-H", "-p", "--comment", "#", "--print-delimiter", "#",
               "--quote", "always"])
        .write_stdin("a,b\n,2\n3,4\n")
        .assert()
        .success()
        .stdout("\"a\"#\"b\"\n\"\"#\"2\"\n\"3\"#\"4\"\n");

    csvdt()
        .args(["-H", "-p", "--comment", "#", "--print-delimiter", "#",
               "--quote", "nonnumeric"])
        .write_stdin("a,b\n,2\n3,4\n")
        .assert()
        .success()
        .stdout("\"a\"#\"b\"\n\"\"#2\n3#4\n");

    // The shape the regression was reported in: the comment collides with
    // the default write delimiter only.
    csvdt()
        .args(["-H", "-p", "--read-delimiter", ";", "--comment", ",",
               "--quote", "always"])
        .write_stdin("a;b\n1;2\n")
        .assert()
        .success()
        .stdout("\"a\",\"b\"\n\"1\",\"2\"\n");

    csvdt()
        .args(["-H", "-p", "--single-quote", "--comment", "\"",
               "--quote", "never"])
        .write_stdin("a,b\n1,2\n")
        .assert()
        .success()
        .stdout("a,b\n1,2\n");

    // A field that itself starts with the comment character under 'never' is
    // the tally's case, not a refusal: the run succeeds and warns. The field
    // reaches first position through -a -- in the input it cannot be first,
    // since the reader would take that line for a comment.
    csvdt()
        .args(["-H", "-p", "--single-quote", "--comment", "\"",
               "--quote", "never", "-a1"])
        .write_stdin("a,b\n1,\"x\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("row(s) begin with"));

    // The read side does not stand down -- --quote shapes only the output,
    // and the reader drops these rows no matter how the writer quotes.
    csvdt()
        .args(["-H", "-p", "--comment", ",", "--quote", "always"])
        .write_stdin("a,b\n1,2\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("delimiter being read"));
}

#[test]
fn a_tab_delimiter_is_still_accepted_since_that_is_how_tsv_is_read() {
    // The refusals above are for characters that cannot work, not for ones that
    // look unusual. A tab is the ordinary way to read and write TSV.
    csvdt()
        .args(["-H", "-p", "--read-delimiter", "\t"])
        .write_stdin("a\tb\n1\t2\n")
        .assert()
        .success()
        .stdout("a,b\n1,2\n");

    csvdt()
        .args(["-H", "-p", "--print-delimiter", "\t"])
        .write_stdin("a,b\n1,2\n")
        .assert()
        .success()
        .stdout("a\tb\n1\t2\n");
}

#[test]
fn a_closed_output_pipe_is_not_an_error() {
    // `csvdt file | head` closes the pipe as soon as it has what it wants.
    // Rust leaves SIGPIPE ignored, so the write comes back as a broken-pipe
    // error rather than ending the process -- and reporting it put a failure
    // on the screen, and a non-zero status into any `set -o pipefail` script,
    // for one of the most ordinary things anyone does with a command-line
    // tool.
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut input = String::from("ts,x\n");
    for row in 0..20_000 {
        input.push_str(&format!("2026-07-25T12:00:00Z,{}\n", row));
    }

    let mut child = Command::new(assert_cmd::cargo::cargo_bin("csvdt"))
        .args(["-H", "-p", "-u0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // Fed from its own thread: the input is larger than the pipe buffer, so
    // writing it blocks until csvdt reads -- and csvdt cannot produce the
    // output this test then closes until it has been fed. Doing both from
    // here would wait on each other.
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || {
        // Fails partway once the child has gone, which is the point.
        let _ = stdin.write_all(input.as_bytes());
    });

    // Take what a `head` would, then close the pipe with csvdt still writing.
    {
        let mut stdout = child.stdout.take().unwrap();
        let mut first = [0u8; 64];
        let _ = std::io::Read::read(&mut stdout, &mut first);
    }

    let output = child.wait_with_output().unwrap();
    let _ = writer.join();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "a closed output pipe should not fail: status {:?}, stderr {:?}",
        output.status, stderr
    );
    assert!(stderr.is_empty(), "should say nothing, said {:?}", stderr);
}

#[test]
fn a_genuine_write_failure_is_still_reported() {
    // The broken-pipe case is excused by kind, not by being an io error, so
    // everything else still reports and still exits non-zero.
    csvdt()
        .args(["-H", "-p", "-u9"])
        .write_stdin("a\n1\n")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("out of bound"));
}

#[test]
#[cfg(unix)]
fn widest_reads_a_named_pipe_correctly_rather_than_draining_it() {
    // --flexible=widest reads its input twice. A regular file gives the same
    // thing both times; a fifo does not. Reading one twice drained it on the
    // counting pass and left the real pass with nothing, so the file came back
    // EMPTY with a success status -- and process substitution, `csvdt -f=widest
    // <(...)`, is a fifo wearing a filename.
    let dir = tempfile_dir().join("widest-fifo");
    std::fs::create_dir_all(&dir).unwrap();
    let fifo = dir.join("input.csv");
    let _ = std::fs::remove_file(&fifo);
    let writer = fifo_writer(fifo.clone(), "a,b\n0,1,2,3\n0\n");

    csvdt()
        .args(["-H", "-p", "-f=widest"])
        .arg(&fifo)
        .env("TMPDIR", &dir)
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout("a,b,,\n0,1,2,3\n0,,,\n");

    let _ = writer.join();

    // Spooled, so the temporary file has to have gone again.
    let left: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path() != fifo)
        .collect();
    assert!(left.is_empty(), "temporary file left behind: {:?}", left);
    let _ = std::fs::remove_file(&fifo);
}

#[test]
fn widest_does_not_spool_a_file_it_can_simply_read_twice() {
    // The spooling is for inputs that cannot be re-read. A regular file can,
    // and copying it would double the cost of the mode for no reason.
    let dir = tempfile_dir().join("widest-regular");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("data.csv");
    std::fs::write(&path, "a,b\n0,1,2,3\n0\n").unwrap();

    with_temp_dir(csvdt(), &dir)
        .args(["-H", "-p", "-f=widest"])
        .arg(&path)
        .assert()
        .success()
        .stdout("a,b,,\n0,1,2,3\n0,,,\n");

    let left: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path() != path)
        .collect();
    assert!(left.is_empty(), "should not have spooled: {:?}", left);
}

// Creates a fifo and writes csv into it from its own thread, because opening a
// fifo to read blocks until a writer arrives. Returns the join handle so the
// caller can wait for the write to finish.
#[cfg(unix)]
fn fifo_writer(
    fifo: std::path::PathBuf,
    contents: &'static str,
) -> std::thread::JoinHandle<()> {
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(status.success(), "could not create a fifo to test with");

    std::thread::spawn(move || {
        let _ = std::fs::write(&fifo, contents);
    })
}

#[test]
#[cfg(unix)]
fn widest_does_not_write_through_a_symlink_left_at_the_name_it_wanted() {
    // The temporary directory is shared with every other account on the
    // machine, and the spool used to be created with plain create, which
    // follows a symlink. Anyone able to write that directory could leave one at
    // 'csvdt-widest-<pid>.csv' and have csvdt overwrite whatever it pointed at
    // -- anything the user could write -- and the cleanup then removed the
    // symlink behind it, leaving nothing to find. A pid is guessable; here it
    // is exact, because the shell execs csvdt and so hands over the very pid it
    // planted the name under.
    let dir = tempfile_dir().join("widest-spool-symlink");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let victim = dir.join("victim");
    std::fs::write(&victim, "PRECIOUS\n").unwrap();
    let fifo = dir.join("input.csv");
    let writer = fifo_writer(fifo.clone(), "a,b\n0,1,2,3\n0\n");

    let script = format!(
        "ln -s '{}' \"$TMPDIR/csvdt-widest-$$.csv\"; exec '{}' -H -p -f=widest '{}'",
        victim.display(),
        assert_cmd::cargo::cargo_bin("csvdt").display(),
        fifo.display(),
    );
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&script)
        .env("TMPDIR", &dir)
        .output()
        .unwrap();
    let _ = writer.join();

    // The run still works: a taken name costs a second try, not the mode.
    assert!(
        output.status.success(),
        "should have spooled under another name: {:?}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "a,b,,\n0,1,2,3\n0,,,\n",
    );
    assert_eq!(
        std::fs::read_to_string(&victim).unwrap(),
        "PRECIOUS\n",
        "csvdt wrote through the symlink",
    );
    // And it is not csvdt's to delete either, so the trail stays visible.
    let planted: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("csvdt-"))
        .collect();
    assert_eq!(
        planted.len(),
        1,
        "the planted symlink should still be there, and the spool gone: {:?}",
        planted,
    );
    assert!(
        std::fs::symlink_metadata(planted[0].path())
            .unwrap()
            .file_type()
            .is_symlink(),
        "the file left behind is csvdt's spool, not the planted symlink",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[cfg(unix)]
fn widest_says_so_when_every_name_it_could_spool_under_is_taken() {
    // Refusing a taken name means there is a way to leave csvdt with nowhere to
    // write, so that has to be a clear error naming the directory rather than a
    // bare 'File exists'. It is also the only outcome available to whoever
    // plants the names: no overwrite, and nothing silently unpadded.
    let dir = tempfile_dir().join("widest-spool-exhausted");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let fifo = dir.join("input.csv");
    let writer = fifo_writer(fifo.clone(), "a,b\n0,1,2,3\n0\n");

    let script = format!(
        "for n in '' -2 -3 -4 -5 -6 -7 -8; do : > \"$TMPDIR/csvdt-widest-$$$n.csv\"; done; \
         exec '{}' -H -p -f=widest '{}'",
        assert_cmd::cargo::cargo_bin("csvdt").display(),
        fifo.display(),
    );
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&script)
        .env("TMPDIR", &dir)
        .output()
        .unwrap();
    let _ = writer.join();

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(!output.status.success(), "should have failed: {:?}", stderr);
    assert!(
        stderr.contains("--flexible=widest")
            && stderr.contains(&dir.display().to_string())
            && stderr.contains("TMPDIR"),
        "should name the mode, the directory and the way out: {:?}",
        stderr,
    );
    assert!(
        output.stdout.is_empty(),
        "should not have written a half-padded file: {:?}",
        String::from_utf8_lossy(&output.stdout),
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[cfg(unix)]
fn widest_spool_is_readable_only_by_the_user_who_ran_it() {
    // The spool holds the whole input, in a directory every account on the
    // machine can read. Created with the default permissions it came out
    // 0644 under the usual umask, which published the input -- often a log --
    // to all of them for as long as the run lasted.
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile_dir().join("widest-spool-mode");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let fifo = dir.join("input.csv");

    // Holds the fifo open after writing, so the copy cannot finish and the
    // spool is still there to be looked at -- until this test has looked,
    // rather than for a fixed three seconds. Sleeping that long while the
    // search below is allowed six left a window for a loaded machine to
    // stall in: the input ended, the run finished, the spool was cleaned up
    // the ordinary way, and the search found nothing to read a mode from.
    // The failure was then about permissions, which were never the trouble.
    let (release, released) = std::sync::mpsc::channel::<()>();
    let writer = {
        let fifo = fifo.clone();
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(status.success(), "could not create a fifo to test with");
        std::thread::spawn(move || {
            use std::io::Write;
            let mut handle = std::fs::OpenOptions::new()
                .write(true)
                .open(&fifo)
                .unwrap();
            handle.write_all(b"a,b\n0,1,2,3\n0\n").unwrap();
            handle.flush().unwrap();
            let _ = released.recv();
        })
    };

    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("csvdt"))
        .args(["-H", "-p", "-f=widest"])
        .arg(&fifo)
        .env("TMPDIR", &dir)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    let mut mode = None;
    for _ in 0..600 {
        let spool = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .find(|entry| entry.file_name().to_string_lossy().starts_with("csvdt-"));
        if let Some(spool) = spool {
            mode = Some(spool.metadata().unwrap().permissions().mode() & 0o777);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    // Only now may the input end, and the run with it.
    drop(release);
    let _ = writer.join();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        mode,
        Some(0o600),
        "the spool should be readable by its owner only",
    );
}

#[test]
fn the_documented_exit_statuses_are_what_a_caller_actually_gets() {
    // EXIT STATUS in the help is a contract a script keys on -- several of
    // the KNOWN LIMITS turn on it, since a run that converted nothing is
    // told from one that worked by the status alone. Pinned here so the
    // section cannot drift away from what the binary does.
    let good = "a,b\n2024-06-15T14:00:00Z,2\n";

    // 0: finished, and where an action was given something converted.
    csvdt().write_stdin("a,b\n1,2\n").assert().code(0);
    csvdt().arg("-u0").write_stdin(good).assert().code(0);
    // Still 0 with some values unconverted: a few bad lines are ordinary.
    csvdt()
        .arg("-u0")
        .write_stdin("a,b\nnope,2\n2024-06-15T14:00:00Z,3\n")
        .assert()
        .code(0);

    // 1: began and could not finish, or converted nothing at all.
    csvdt().arg("/no/such/file/at/all").assert().code(1);
    csvdt().arg("-u9").write_stdin(good).assert().code(1);
    csvdt()
        .arg("-u0")
        .write_stdin("a,b\nnope,2\n")
        .assert()
        .code(1);

    // 2: the command line did not parse.
    csvdt().arg("--no-such-option").assert().code(2);
    csvdt().args(["--trim", "sideways"]).assert().code(2);
    csvdt().args(["-u0", "--remove", "1"]).assert().code(2);

    // 100: it parsed, and then a value in it was refused.
    csvdt().arg("-o0,+99:00").assert().code(100);
    csvdt().args(["--read-delimiter", ",,"]).assert().code(100);
    csvdt().arg("-d0,0").assert().code(100);
    csvdt().args(["-a", "1,1"]).assert().code(100);
}

#[test]
#[cfg(unix)]
fn a_signal_removes_the_widest_spool_rather_than_leaving_it() {
    // Drop covers every ending Rust knows about, but a signal is not one of
    // them: the process stops where it stands and no destructor runs. So
    // Ctrl-C during the copy -- or the SIGTERM a scheduler sends when a run
    // overruns -- left the spool holding the whole input behind, and nothing
    // ever came back for it. The copy is the long part of the run, which is
    // exactly when someone reaches for Ctrl-C.
    //
    // This test signals the moment the spool appears, which made it the worst
    // possible prober of the gap between the file existing and the handler
    // knowing its name: a signal landing in there took the default action and
    // left the copy behind. It failed perhaps one run in a hundred, on CI
    // rather than here, and read as a flake. It was not one. The two steps are
    // done under a held signal mask now, so a signal cannot arrive between
    // them, and widening that gap to fifty milliseconds no longer fails this
    // -- where before it failed every time.
    use std::os::unix::process::ExitStatusExt;

    let dir = tempfile_dir().join("widest-spool-signal");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let fifo = dir.join("input.csv");

    // Held open after writing so the copy cannot finish on its own and the
    // spool is certainly still there when the signal arrives.
    //
    // Held until this test says so rather than for a fixed few seconds. The
    // writer used to sleep three, while the wait for the spool below is
    // allowed rather longer: a machine loaded enough to stall in between
    // closed the fifo early, so the copy reached the end of its input and
    // the run finished of its own accord before the signal was ever sent.
    //
    // Which assertion then failed depended only on where the stall landed.
    // Late enough and the spool had already been cleaned up the ordinary
    // way, so the search below found nothing at all; earlier, and it was
    // found but the process behind it had exited, leaving a status with no
    // signal in it. Neither reads like what it was. Both look like the
    // handler misbehaving, which is the program this test exists to defend,
    // and both happened seldom enough to be believed.
    //
    // The channel closes that gap by construction: no duration anywhere,
    // and the input cannot end until the signal has been sent and the child
    // reaped. If this test panics first, the sender drops with it, the
    // receive fails rather than blocking, and the thread still ends.
    let (release, released) = std::sync::mpsc::channel::<()>();
    let writer = {
        let fifo = fifo.clone();
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(status.success(), "could not create a fifo to test with");
        std::thread::spawn(move || {
            use std::io::Write;
            let mut handle = std::fs::OpenOptions::new()
                .write(true)
                .open(&fifo)
                .unwrap();
            let _ = handle.write_all(b"a,b\n0,1,2,3\n0\n");
            let _ = handle.flush();
            let _ = released.recv();
        })
    };

    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("csvdt"))
        .args(["-H", "-p", "-f=widest"])
        .arg(&fifo)
        .env("TMPDIR", &dir)
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();

    // Waited for rather than slept past, so a loaded machine cannot turn this
    // into a test that signals before there is anything to remove. The wait
    // can be as long as it needs to be now that nothing else is on a clock:
    // the input stays open until the signal has been sent, whenever that is.
    let mut spool = None;
    for _ in 0..600 {
        spool = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .find(|entry| entry.file_name().to_string_lossy().starts_with("csvdt-"))
            .map(|entry| entry.path());
        if spool.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let spool = spool.expect(
        "no spool appeared in twelve seconds, so the run never reached the \
         copy this test is about",
    );

    // SAFETY: the pid is this test's own child, which has not been waited for.
    unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGINT) };
    let status = child.wait().unwrap();

    let left_behind = spool.exists();
    // Only now may the input end. Everything this test asks about has
    // already happened, so the run cannot finish on its own instead.
    drop(release);
    let _ = writer.join();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        !left_behind,
        "the spool was left behind at {}",
        spool.display(),
    );
    // And the run still ends of the signal it was sent rather than of
    // anything the handler decided, so a caller's shell reports it as it
    // always has -- 130 for Ctrl-C, and a wait status that still says
    // "interrupted" to anything reading one.
    assert_eq!(
        status.signal(),
        Some(libc::SIGINT),
        "the run should still die of the signal it was sent",
    );
}

#[test]
fn the_deprecated_zone_names_carry_their_geographic_zone_s_history() {
    // KNOWN LIMITS described these as a gap: zones holding a modern rule and
    // no history, answering for dates before that rule began as though it had
    // always applied. That was true of the data IANA once shipped and is not
    // true of the release bundled here, where each is a link -- so the
    // section warned against names that are now exactly the zones it
    // recommended instead. Nothing had ever checked it.
    //
    // 1950 is the year to ask about: mid-summer, well before the modern
    // European rules, and a whole number of minutes in every zone here, so
    // the answer is a timestamp rather than the parse_err a fractional offset
    // would give. Under the old data EET read +03:00 where Athens read
    // +02:00; under this data they agree, and the test says which.
    for (deprecated, geographic, expected) in [
        ("EET", "Europe/Athens", "1950-07-15T14:00:00+02:00"),
        ("WET", "Europe/Lisbon", "1950-07-15T13:00:00+01:00"),
        ("CET", "Europe/Brussels", "1950-07-15T13:00:00+01:00"),
        ("MET", "Europe/Brussels", "1950-07-15T13:00:00+01:00"),
        ("EST5EDT", "America/New_York", "1950-07-15T08:00:00-04:00"),
    ] {
        for zone in [deprecated, geographic] {
            csvdt()
                .arg("-l1")
                .env("TZ", zone)
                .write_stdin("x,1950-07-15T12:00:00Z\n")
                .assert()
                .success()
                .stdout(format!("x,1950-07-15T12:00:00Z,{expected}\n"));
        }
    }
}

#[test]
fn local_refuses_a_tz_that_is_not_a_zone_it_knows() {
    // A misspelt zone used to fall through to UTC, so -l reported +00:00 as
    // though it were the zone the user believed they had named. Nothing in the
    // output showed it: a plausible timestamp with a plausible offset. That is
    // the same quiet mistake -o refuses for an impossible offset, and worse
    // here, since the result looks right.
    for tz in [
        "Europe/Stockholmm",   // a letter too many
        "America/New_Yrok",    // transposed
        "Not/AZone",
        "ESTT",                // not the abbreviation it nearly is
        "Europe/",             // an area with no location
        "america/chicago",     // zone names are case-sensitive
    ] {
        csvdt()
            .args(["-H", "-p", "-l0", "-i", "0,replace"])
            .env("TZ", tz)
            .write_stdin("ts\n2026-07-25T12:00:00Z\n")
            .assert()
            .failure()
            .code(100)
            .stderr(predicate::str::contains(tz))
            .stderr(predicate::str::contains("not a zone"))
            // Naming the bundled release matters: a zone newer than this build
            // is refused for a different reason than a typo, and the fix is
            // different too.
            .stderr(predicate::str::contains("tzdata"))
            // And the blame stays with TZ, which really did name this zone.
            // The other wording -- "this system is configured for" -- belongs
            // to the branch where TZ is unset and the platform supplied the
            // name; it cannot be reached from a test, since that needs a host
            // whose configured zone the bundled database does not know, so
            // asserting its absence here is the half that can be pinned.
            .stderr(predicate::str::contains("TZ is set to"))
            .stderr(predicate::str::contains("this system is configured").not());
    }
}

#[test]
fn no_conversion_but_local_reads_the_zone() {
    // The page's ENVIRONMENT section promises the zone reaches -l and reaches
    // nothing else: "-u, -o, -d, -s and -r work from the offset the record
    // already carries, and so give the same answer on any machine". That
    // sentence is what lets an answer be quoted somewhere it will be checked
    // on another machine, which is the use this program is written for.
    //
    // Every other TZ test here asks what -l does with a zone. None asked that
    // the rest ignore it, so a conversion that started reading the host's
    // zone would have broken the promise silently -- and plausibly, since the
    // result would still be a well-formed timestamp with a well-formed
    // offset, the same shape of quiet mistake -l itself used to make.
    //
    // Eight settings, because they do not take one path: a name csvdt looks
    // up in the bundled database, a POSIX rule string chrono parses, and no
    // value at all. Half-hour and quarter-hour zones among them, so a
    // conversion reading the zone is off by something a whole-hour zone
    // could hide against the offsets in the fixtures.
    const ZONES: [Option<&str>; 8] = [
        Some("UTC"),
        Some("Asia/Kolkata"),        // +05:30
        Some("Pacific/Kiritimati"),  // +14:00
        Some("Australia/Lord_Howe"), // +10:30, and +11 in summer
        Some("America/St_Johns"),    // -03:30
        Some("Europe/Amsterdam"),
        Some("EST5EDT,M3.2.0,M11.1.0"), // rules rather than a name
        None,                           // unset
    ];

    let under = |arguments: &[&str], input: &str, zone: Option<&str>| {
        let mut command = csvdt();
        command.args(arguments);
        match zone {
            Some(name) => command.env("TZ", name),
            None => command.env_remove("TZ"),
        };
        let output = command.write_stdin(input.to_string()).output().unwrap();
        String::from_utf8(output.stdout).expect("csvdt should write UTF-8")
    };

    // A timestamp carrying Z, one carrying an offset of its own, one from
    // before any of these zones had their modern rules, one landing in the
    // hour Europe changes over, and epoch seconds -- which only -r reads.
    const ONE: [&str; 5] = [
        "ts,b\n2026-07-25T12:00:00Z,x\n",
        "ts,b\n2026-07-25T12:00:00+02:00,x\n",
        "ts,b\n1950-01-15T12:00:00Z,x\n",
        "ts,b\n2026-03-29T01:00:00Z,x\n",
        "ts,b\n1753441200,x\n",
    ];
    const TWO: &str = "a,b\n2026-01-01T00:00:00Z,2026-06-01T12:30:00+05:30\n";

    let mut cases: Vec<(Vec<&str>, &str)> = Vec::new();
    for input in ONE {
        for arguments in [
            vec!["-H", "-u", "0"],
            vec!["-H", "-o", "0,-06:00"],
            vec!["-H", "-s", "0"],
            vec!["-H", "-r", "0"],
        ] {
            cases.push((arguments, input));
        }
    }
    cases.push((vec!["-H", "-d", "0,1"], TWO));

    for (arguments, input) in &cases {
        let first = under(arguments, input, ZONES[0]);
        for zone in &ZONES[1..] {
            assert_eq!(
                under(arguments, input, *zone),
                first,
                "{arguments:?} answered differently under TZ={zone:?} than \
                 under {:?}, so it is reading the host's zone",
                ZONES[0],
            );
        }
    }

    // The control, and this test is worth nothing without it: a build that
    // read TZ nowhere at all -- -l included -- satisfies every assertion
    // above. So -l has to be seen to differ.
    //
    // Not the epoch fixture for this one. -l does not read epoch seconds, it
    // writes parse_err, so it agrees with itself under every zone and would
    // look like the very failure this control exists to rule out.
    let mut differed = 0;
    let first = under(&["-H", "-l", "0"], ONE[0], ZONES[0]);
    for zone in &ZONES[1..] {
        if under(&["-H", "-l", "0"], ONE[0], *zone) != first {
            differed += 1;
        }
    }
    assert!(
        differed >= 5,
        "-l gave the same answer under {} of the {} other zone settings, so \
         this build may not be reading TZ anywhere and the checks above \
         prove nothing",
        ZONES.len() - 1 - differed,
        ZONES.len() - 1,
    );
}

#[test]
#[cfg(unix)]
fn local_still_accepts_posix_rules_rather_than_refusing_them() {
    // Refusing an unknown name must not refuse POSIX rules, which are the
    // reason -l cannot simply demand a zone from the database -- they are just
    // as much "not a zone name" as a misspelling is.
    //
    // Unix only, for the same reason local_falls_back_to_chrono_for_a_posix_
    // rule_string is: rules are left to chrono, and chrono reads TZ on Unix but
    // not on Windows, where the system zone wins instead. Zone *names* are
    // resolved by csvdt itself and so behave identically everywhere, which is
    // what the test below covers.
    for (tz, expected) in [
        ("EST5EDT,M3.2.0,M11.1.0", "2026-07-25T08:00:00-04:00"),
        ("GMT0BST,M3.5.0/1,M10.5.0", "2026-07-25T13:00:00+01:00"),
        // Rules with an offset and nothing else. The sign is reversed from an
        // ISO offset, which is POSIX's doing rather than ours.
        ("UTC+3", "2026-07-25T09:00:00-03:00"),
        // The bracketed abbreviation form.
        ("<+04>-4", "2026-07-25T16:00:00+04:00"),
    ] {
        csvdt()
            .args(["-H", "-p", "-l0", "-i", "0,replace"])
            .env("TZ", tz)
            .write_stdin("ts\n2026-07-25T12:00:00Z\n")
            .assert()
            .success()
            .stdout(format!("to_local\n{}\n", expected));
    }
}

#[test]
fn local_accepts_a_zone_name_that_happens_to_carry_digits() {
    // These look like POSIX rules and are not: they are names in the database,
    // settled by the zone lookup before anything examines their shape. Worth
    // asserting on every platform, since csvdt resolves names itself and so
    // must agree everywhere.
    for (tz, expected) in [
        ("EST5EDT", "2026-07-25T08:00:00-04:00"),
        ("Etc/GMT+3", "2026-07-25T09:00:00-03:00"),
    ] {
        csvdt()
            .args(["-H", "-p", "-l0", "-i", "0,replace"])
            .env("TZ", tz)
            .write_stdin("ts\n2026-07-25T12:00:00Z\n")
            .assert()
            .success()
            .stdout(format!("to_local\n{}\n", expected));
    }
}

#[test]
#[cfg(unix)]
fn local_leaves_the_path_form_of_tz_alone() {
    // TZ=:/usr/share/zoneinfo/... is glibc's other implementation-defined
    // form. It is not a zone name, so it must not be refused as a misspelt
    // one; whatever chrono makes of it is chrono's business.
    csvdt()
        .args(["-H", "-p", "-l0", "-i", "0,replace"])
        .env("TZ", "/etc/localtime")
        .write_stdin("ts\n2026-07-25T12:00:00Z\n")
        .assert()
        .success();
}

#[test]
fn local_is_only_resolved_when_it_is_asked_for() {
    // A refusal has to be confined to runs that use -l. Everything else must
    // work with a TZ this build cannot resolve, since it never consults it.
    csvdt()
        .args(["-H", "-p", "-u0", "-i", "0,replace"])
        .env("TZ", "Europe/Stockholmm")
        .write_stdin("ts\n2026-07-25T12:00:00+02:00\n")
        .assert()
        .success()
        .stdout("to_utc\n2026-07-25T10:00:00+00:00\n");
}

#[test]
fn a_conversion_that_needs_no_zone_gives_the_same_answer_anywhere() {
    // The claim the documentation now makes, asserted rather than described:
    // -u and -o work from the offset the record already carries, so the
    // environment cannot reach them. -l is the one that can differ, which is
    // why the two are documented as answering different questions.
    let input = "ts,event\n2026-07-25T12:00:00+02:00,a\n2026-01-15T08:30:00+01:00,b\n";

    for arguments in [
        vec!["-H", "-p", "-u0", "-i", "0,replace"],
        vec!["-H", "-p", "-o0,+02:00", "-i", "0,replace"],
    ] {
        let mut results = Vec::new();
        for tz in ["UTC", "Asia/Tokyo", "America/Chicago", "Europe/Stockholm"] {
            let output = csvdt()
                .args(&arguments)
                .env("TZ", tz)
                .write_stdin(input)
                .assert()
                .success()
                .get_output()
                .stdout
                .clone();
            results.push(String::from_utf8(output).unwrap());
        }
        assert!(
            results.windows(2).all(|pair| pair[0] == pair[1]),
            "{:?} should not depend on TZ, got {:?}", arguments, results
        );
    }
}

#[test]
fn local_is_the_conversion_that_does_depend_on_the_environment() {
    // The other half of the same claim. If this ever stopped being true, the
    // documentation above would be wrong in the more misleading direction --
    // promising a dependency that had quietly gone away.
    let input = "ts,event\n2026-07-25T12:00:00+02:00,a\n";
    let mut results = Vec::new();
    for tz in ["UTC", "Asia/Tokyo", "America/Chicago"] {
        let output = csvdt()
            .args(["-H", "-p", "-l0", "-i", "0,replace"])
            .env("TZ", tz)
            .write_stdin(input)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        results.push(String::from_utf8(output).unwrap());
    }
    assert!(
        results.windows(2).all(|pair| pair[0] != pair[1]),
        "-l should differ per zone, got {:?}", results
    );
}

#[test]
fn known_limits_says_what_local_depends_on() {
    let help = help_words();
    for promised in [
        "DEPENDS ON MORE THAN THE INPUT",
        "differently configured hosts",
        "replaces the offset the record carried",
        "pre-1970 rules as best effort",
    ] {
        assert!(
            help.contains(promised),
            "--help no longer says '{promised}' about -l/--local",
        );
    }
}

#[test]
fn local_output_does_not_sort_chronologically_across_a_transition() {
    // Documented in KNOWN LIMITS, and asserted so the documentation cannot
    // drift from it. Sweden's autumn shift repeats an hour, so two different
    // instants read the same on the clock and differ only in offset -- and
    // '+01:00' sorts before '+02:00', putting the later event first.
    //
    // The point is not that this is a fault: it is what the clocks did. It is
    // the reason to build a timeline from -u or -o instead.
    let input = "logged,event\n\
                 2024-10-27T02:30:00+02:00,first\n\
                 2024-10-27T03:30:00+02:00,second\n";

    let local = csvdt()
        .args(["-H", "-p", "-l0", "-i", "0,replace"])
        .env("TZ", "Europe/Stockholm")
        .write_stdin(input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let local = String::from_utf8(local).unwrap();

    // Both wall-clock readings are the same; only the offset differs.
    assert!(local.contains("2024-10-27T02:30:00+02:00,first"));
    assert!(local.contains("2024-10-27T02:30:00+01:00,second"));

    let mut rows: Vec<&str> = local.lines().skip(1).collect();
    rows.sort_unstable();
    assert!(
        rows[0].ends_with(",second"),
        "sorting -l output should put the later event first, got {:?}", rows
    );

    // Whereas one offset throughout sorts into the order things happened.
    let offset = csvdt()
        .args(["-H", "-p", "-o0,+01:00", "-i", "0,replace"])
        .write_stdin(input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let offset = String::from_utf8(offset).unwrap();
    let mut rows: Vec<&str> = offset.lines().skip(1).collect();
    rows.sort_unstable();
    assert!(
        rows[0].ends_with(",first"),
        "sorting -o output should be chronological, got {:?}", rows
    );
}

// ---- --single-quote round trips, and other eighth/ninth-round repairs ----

#[test]
fn single_quote_output_that_would_misread_is_counted_on_stderr() {
    // The output of a --single-quote run is ordinary double-quoted CSV --
    // only reading uses the single quote -- so reading it back with the
    // same flags misreads every field the writer quoted: the '"' stays in
    // the data and the field splits on any delimiter inside it. The run
    // that wrote such a field has to say so; it used to say nothing, and
    // the round trip broke with exit 0.
    csvdt()
        .arg("--single-quote")
        .write_stdin("'x,y',z\n")
        .assert()
        .success()
        .stdout("\"x,y\",z\n")
        .stderr(predicate::str::contains(
            "1 row(s) hold a field that '--single-quote' would misread"))
        .stderr(predicate::str::contains(
            "Read the output back without '--single-quote'"));
}

#[test]
fn single_quote_output_that_reads_back_exactly_stays_silent() {
    // The warning is about the output, so an output with nothing quoted
    // has nothing to warn about -- a run over plain fields must not cry
    // wolf on every --single-quote invocation.
    csvdt()
        .arg("--single-quote")
        .write_stdin("a,b\n1,2\n")
        .assert()
        .success()
        .stdout("a,b\n1,2\n")
        .stderr("");
}

#[test]
fn single_quote_misreads_are_counted_per_row_not_per_field() {
    // Two quoted fields in one row are one row lost, not two.
    csvdt()
        .arg("--single-quote")
        .write_stdin("'a,a','b,b'\n'c,c',d\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("2 row(s) hold a field"));
}

#[test]
fn single_quote_lone_empty_row_is_counted_too() {
    // One quoting rule belongs to the record rather than any field: a row
    // that is a single empty field is written quoted ("") under every
    // style, 'never' included, so the record does not vanish into a blank
    // line. Read back with --single-quote, that "" is a two-character
    // field like any other quoted one -- but a per-field check cannot see
    // a rule the writer applies to the row.
    csvdt()
        .arg("--single-quote")
        .write_stdin("''\n")
        .assert()
        .success()
        .stdout("\"\"\n")
        .stderr(predicate::str::contains("1 row(s) hold a field"));

    csvdt()
        .args(["--single-quote", "--quote", "never"])
        .write_stdin("''\n")
        .assert()
        .success()
        .stdout("\"\"\n")
        .stderr(predicate::str::contains("1 row(s) hold a field"));
}

#[test]
fn comment_as_the_last_line_needs_no_trailing_newline() {
    // csv-core (0.1.13) mishandles end-of-input inside a comment: its DFA
    // treats it like mid-record and fabricates a record of one empty
    // field. A file whose last line was a comment with no trailing newline
    // was refused as ragged -- exactly the file a hand-edited comment at
    // the bottom produces.
    csvdt()
        .args(["--comment", "#"])
        .write_stdin("a,b\n#tail")
        .assert()
        .success()
        .stdout("a,b\n");

    // A comment ending in a lone \r stays in the comment the same way,
    // since only \n leaves that state.
    csvdt()
        .args(["--comment", "#"])
        .write_stdin("a,b\n#tail\r")
        .assert()
        .success()
        .stdout("a,b\n");
}

#[test]
fn widest_counts_the_file_the_way_the_run_reads_it() {
    // --flexible=widest reads the file twice: once to find the widest record,
    // then again to pad everything out to it. The two passes have to read the
    // same file the same way, and the counting pass gets its reader from
    // configure_reader for exactly that reason -- one builder, used twice.
    //
    // Nothing said so, and the ways it can come apart do not fail alike.
    // Reading with the wrong delimiter fails loudly, and blames the input for
    // having changed between the two reads when it did not. Ignoring the
    // comment character, or the quoting, fails silently: the count comes out
    // too big and every row is padded to a width no record in the file has.
    // The output is rectangular and looks correct, which is the shape of
    // damage the fill log exists to make visible -- and cannot, here, since
    // the widening is what both passes agree the file is.
    //
    // Each case below is a file the two readings disagree about, and the
    // assertion is the shape the right reading gives.

    // A comment eight fields wide over a two-column file. Counted, it sets
    // the width and the file comes out eight columns; skipped, as the writing
    // pass skips it, the width is the records'.
    csvdt()
        .args(["-H", "-p", "-f=widest", "--comment", "#"])
        .write_stdin("# a,b,c,d,e,f,g,h\nid,when\n7,2023-11-14T22:13:20+01:00\n8\n")
        .assert()
        .success()
        .stdout("id,when\n7,2023-11-14T22:13:20+01:00\n8,\n");

    // A quoted field holding the delimiter. Read as CSV it is one field and
    // the row has two; read without quoting it is three and the row has four.
    csvdt()
        .args(["-H", "-p", "-f=widest"])
        .write_stdin("id,note\n1,\"a,b,c\"\n2\n")
        .assert()
        .success()
        .stdout("id,note\n1,\"a,b,c\"\n2,\n");

    // Semicolons. Read as commas every line is one field, and the writing
    // pass then finds rows wider than the count it was given -- which it
    // reports as the input having changed, since that is the only way it
    // can happen when both passes agree.
    csvdt()
        .args(["-H", "-p", "-f=widest", "--read-delimiter", ";"])
        .write_stdin("id;when;level\n7;2023-11-14T22:13:20+01:00;info\n8;2023-11-15T09:00:00+01:00\n")
        .assert()
        .success()
        .stdout("id,when,level\n7,2023-11-14T22:13:20+01:00,info\n8,2023-11-15T09:00:00+01:00,\n");

    // The control for the first case: without --comment that line is data
    // eight fields wide, and widest does take its measure from it. So the
    // fixture can tell a reader that skips comments from one that does not.
    csvdt()
        .args(["-H", "-p", "-f=widest"])
        .write_stdin("# a,b,c,d,e,f,g,h\nid,when\n7,2023-11-14T22:13:20+01:00\n8\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("id,when,,,,,,\n"));
}
#[test]
fn comment_at_eof_grows_no_phantom_row_under_flexible() {
    // With --flexible the same fabricated record was not refused but
    // padded and kept: a phantom empty row that was never in the input.
    csvdt()
        .args(["--comment", "#", "--flexible=fix"])
        .write_stdin("a,b\n#tail")
        .assert()
        .success()
        .stdout("a,b\n");
}

#[test]
fn a_span_across_a_leap_second_is_elapsed_civil_time_not_si_seconds() {
    // What KNOWN LIMITS promises a reader, and the thing they decide by:
    // -d counts ordinary seconds, so the extra second a leap second inserts
    // is not among them. That is elapsed civil time rather than elapsed SI
    // seconds, and it is how nearly every system keeps time -- Unix time has
    // no room for the extra second either -- so the answer agrees with what
    // other tools report for the same two timestamps.
    //
    // The leap-second tests around this one grew out of faults: a sign that
    // landed inside a fraction, a rounding that truncated the wrong way, a
    // :60 folded onto the following minute. Each pins the fault it was
    // written for, and each uses 23:59:60.5Z because half a second is what
    // showed the sign. None of them says what the plain pair measures, so a
    // build that started accounting the extra second -- the reasonable
    // change, and the one somebody will propose -- would pass all of them.
    //
    // Every figure here is one second short of true UTC, deliberately.
    for (input, want) in [
        // The leap second and the moment after it: no ordinary second lies
        // between them, so the span is nothing at all.
        ("2016-12-31T23:59:60Z,2017-01-01T00:00:00Z", "PT0S"),
        // One ordinary second, where UTC counted two.
        ("2016-12-31T23:59:59Z,2017-01-01T00:00:00Z", "PT1S"),
        // A day across the boundary is a day, not a day and a second.
        ("2016-12-31T12:00:00Z,2017-01-01T12:00:00Z", "P1D"),
        // And a span reaching across two leap seconds is two short: the end
        // of June 2015 and the end of December 2016 both had one.
        ("2015-06-30T23:59:59Z,2017-01-01T00:00:00Z", "P550DT1S"),
    ] {
        csvdt()
            .args(["-d0,1"])
            .write_stdin(format!("{input}\n"))
            .assert()
            .success()
            .stdout(format!("{input},{want}\n"));
    }
}

#[test]
fn duration_across_a_leap_second_is_never_negative() {
    // chrono orders timestamps by their clock fields but accounts the leap
    // second's extra time when subtracting, so the low-to-high difference
    // could still come out negative across 23:59:60 -- and the sign of a
    // sub-second delta landed inside the fraction as a literal '-',
    // making PT0.-5S. Not a duration at all.
    csvdt()
        .arg("-d0,1")
        .write_stdin("2016-12-31T23:59:60.5Z,2017-01-01T00:00:00Z\n")
        .assert()
        .success()
        .stdout("2016-12-31T23:59:60.5Z,2017-01-01T00:00:00Z,PT0.5S\n");

    // Both column orders, since -d takes them low-to-high itself. Only
    // the insert position moves with the column naming; the value cannot.
    csvdt()
        .arg("-d1,0")
        .write_stdin("2016-12-31T23:59:60.5Z,2017-01-01T00:00:00Z\n")
        .assert()
        .success()
        .stdout("2016-12-31T23:59:60.5Z,PT0.5S,2017-01-01T00:00:00Z\n");
}

#[test]
fn duration_across_a_leap_second_rounds_away_from_zero() {
    // The same negative delta made --round-seconds truncate toward zero:
    // half a second reported as PT0S, where every other half-second gap
    // rounds up.
    csvdt()
        .args(["-d0,1", "--round-seconds"])
        .write_stdin("2016-12-31T23:59:60.5Z,2017-01-01T00:00:00Z\n")
        .assert()
        .success()
        .stdout("2016-12-31T23:59:60.5Z,2017-01-01T00:00:00Z,PT1S\n");
}

#[test]
fn a_sixty_second_field_away_from_a_leap_second_is_marked_not_folded() {
    // chrono reads :60 on any minute at all and folds it onto the second
    // that follows, so this value and 14:01:00Z arrived as one instant: no
    // conversion could tell them apart, and -d between them was PT0S. A
    // formatter that rounds 59.5 up without carrying writes exactly this,
    // so a damaged value was being answered rather than marked -- and
    // written back out still wearing a :60 that is not RFC3339 on an
    // ordinary minute, so the output did not read back anywhere else.
    csvdt()
        .arg("-u1")
        .write_stdin("x,2024-06-15T14:00:60Z\nx,2024-06-15T14:01:00Z\n")
        .assert()
        .success()
        .stdout(
            "x,2024-06-15T14:00:60Z,parse_err\n\
             x,2024-06-15T14:01:00Z,2024-06-15T14:01:00+00:00\n",
        );

    // The two are distinguishable again, which is the point: the first no
    // longer measures zero against the minute it used to fold onto.
    csvdt()
        .arg("-d0,1")
        .write_stdin("2024-06-15T14:00:60Z,2024-06-15T14:01:00Z\n")
        .assert()
        .failure()
        .stdout("2024-06-15T14:00:60Z,2024-06-15T14:01:00Z,parse_err\n");
}

#[test]
fn a_real_leap_second_is_kept_whatever_offset_the_record_carries() {
    // A leap second is the last second of a UTC month and happens at one
    // instant worldwide, so what decides it is the instant rather than the
    // clock face the record was written on. The first two are the same real
    // leap second seen from +05:30 -- where it reads on the next day's clock
    // -- and from -05:00. The third reads 23:59:60 on its own clock but
    // lands mid-evening in UTC, and is not one.
    csvdt()
        .arg("-u1")
        .write_stdin(
            "x,2017-01-01T05:29:60+05:30\n\
             x,2016-12-31T18:59:60-05:00\n\
             x,2016-12-31T23:59:60+02:00\n",
        )
        .assert()
        .success()
        .stdout(
            "x,2017-01-01T05:29:60+05:30,2016-12-31T23:59:60+00:00\n\
             x,2016-12-31T18:59:60-05:00,2016-12-31T23:59:60+00:00\n\
             x,2016-12-31T23:59:60+02:00,parse_err\n",
        );
}

#[test]
fn a_leap_second_is_allowed_at_any_month_end_rather_than_a_fixed_list() {
    // IERS has only ever declared them at the end of June and December, but
    // it may use any month, and a table of the ones declared so far would
    // refuse every future leap second until the binary was rebuilt. The end
    // of the month is structural and cannot go stale, so it is what the
    // check asks -- June's month end here, against a mid-month value that
    // wears the same 23:59:60 and is refused.
    csvdt()
        .arg("-u1")
        .write_stdin("x,2024-06-30T23:59:60Z\nx,2024-06-15T23:59:60Z\n")
        .assert()
        .success()
        .stdout(
            "x,2024-06-30T23:59:60Z,2024-06-30T23:59:60+00:00\n\
             x,2024-06-15T23:59:60Z,parse_err\n",
        );
}

#[test]
fn tenth_fractional_digit_is_refused_rather_than_truncated() {
    // chrono reads at most nine fractional digits and silently drops the
    // rest, against the promise that what the source had is what comes
    // out. A tenth digit now lands in parse_err; nine still round-trip.
    csvdt()
        .args(["-u0", "-i", "0,replace"])
        .write_stdin(
            "2024-01-01T00:00:00.1234567890Z\n\
             2024-01-01T00:00:00.123456789Z\n",
        )
        .assert()
        .success()
        .stdout(
            "parse_err\n\
             2024-01-01T00:00:00.123456789+00:00\n",
        );
}

#[test]
fn out_of_bound_column_names_the_record_it_was_measured_against() {
    // The bare message named neither the record nor its width, and it
    // bites exactly where that matters: a ragged file, where the short
    // row is the thing being hunted.
    csvdt()
        .arg("-u5")
        .write_stdin("a,b\n")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "record 0 (line: 1) has 2 field(s), counted from 0"));
}

#[test]
fn multi_byte_comment_refusal_names_the_comment_option() {
    // The refusal used to call --comment's character a delimiter.
    csvdt()
        .args(["--comment", "é"])
        .write_stdin("a\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("comment character"))
        .stderr(predicate::str::contains("'--comment'"));
}

#[test]
fn comment_advice_stands_down_when_the_comment_is_the_quote() {
    // The dropped-rows warning usually recommends the quoting styles that
    // keep such a field quoted -- but with --comment '"' those styles are
    // refused outright, and recommending them would send the reader in a
    // circle. The single quote shields the '"' from starting a comment on
    // the way in; 'never' writes it bare on the way out.
    csvdt()
        .args(["--single-quote", "--comment", "\"", "--quote", "never"])
        .write_stdin("'\"x',y\n")
        .assert()
        .success()
        .stdout("\"x,y\n")
        .stderr(predicate::str::contains("pick a different one for '--comment'"))
        .stderr(predicate::str::contains("'necessary'").not());
}

#[test]
fn widest_spool_failure_names_the_file_and_the_mode() {
    // A spool that cannot be created reported a bare OS error with no
    // path -- nothing to say what was attempted or where. Now it names
    // the mode, the directory it tried, and the way out.
    //
    // The way out is not the same everywhere, which is the other half of
    // this: std::env::temp_dir reads TMPDIR on Unix and TMP then TEMP on
    // Windows, so telling a Windows user to set TMPDIR sends them to set
    // something nothing reads. The message names whichever this platform
    // actually consults, and this runs everywhere so that both spellings
    // are exercised rather than only the one the author happened to be on.
    let missing = tempfile_dir().join("no-such-directory-for-a-spool");
    let variable = if cfg!(windows) { "Set TMP or TEMP" } else { "Set TMPDIR" };

    with_temp_dir(csvdt(), &missing)
        .arg("--flexible=widest")
        .write_stdin("a,b\n1\n")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("'--flexible=widest'"))
        .stderr(predicate::str::contains(
            missing.display().to_string()))
        .stderr(predicate::str::contains(variable));
}

// ---- tenth review round: field-less rows, and the CR-only trap -----------

#[test]
fn remove_of_every_column_still_counts_the_single_quote_misread() {
    // A record --remove leaves with no fields at all is written as "" --
    // the same bytes as a lone empty row, under every quoting style --
    // and misreads on a --single-quote re-read the same way. It used to
    // slip past the warning because the empty slice matched neither the
    // lone-empty rule nor any field.
    csvdt()
        .args(["--single-quote", "--remove", "0,1"])
        .write_stdin("a,b\n")
        .assert()
        .success()
        .stdout("\"\"\n")
        .stderr(predicate::str::contains("1 row(s) hold a field"));

    // The row itself is fine without --single-quote: the "" reads back
    // as the empty record it is, so there is nothing to say.
    csvdt()
        .args(["--remove", "0,1"])
        .write_stdin("a,b\n")
        .assert()
        .success()
        .stdout("\"\"\n")
        .stderr("");
}

#[test]
fn known_limits_name_the_cr_only_comment_trap() {
    // csv-core leaves its comment state only on \n, so in a CR-only file
    // (classic Mac line endings) the first comment line runs on through
    // every row that follows, silently. Not fixable outside csv-core's
    // DFA -- a \r inside a quoted field is data, so a byte-level shim
    // cannot tell which \r to rewrite -- which makes the promise here
    // documentation: the trap and the way out must be in KNOWN LIMITS.
    // Asked of the help with its whitespace collapsed, so that where the
    // sentence happens to break is not the question. It was asked in two
    // pieces before, to survive a break falling between them -- which held
    // until the corpus was refolded and the break moved inside one of them.
    let help = help_words();
    for phrase in [
        "A CR-ONLY FILE LOSES ROWS TO --comment",
        "Convert the line endings first",
    ] {
        assert!(help.contains(phrase), "--help no longer says {phrase:?}");
    }
}

// ---- eleventh round: the TZ forms that used to mean UTC ------------------

// TZ carries no weight on Windows, which takes its zone from the system,
// so the rule form is a Unix question and so are its refusals.
#[cfg(unix)]
#[test]
fn posix_tz_rules_this_build_cannot_read_are_refused() {
    // chrono reads the POSIX rule form itself, and falls back to UTC
    // without a word where it cannot -- a transition time past 24:00 is
    // enough, and real zones publish those: this is Asia/Jerusalem's own
    // tzdata footer. '-l' answered +00:00 for a zone three hours away,
    // exit 0, which is the mistake the unknown-name refusal exists to stop.
    csvdt()
        .args(["-H", "-l0"])
        .env("TZ", "IST-2IDT,M3.4.4/26,M10.5.0")
        .write_stdin("ts\n2024-06-15T12:00:00Z\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("POSIX rules"))
        .stderr(predicate::str::contains("Name the zone instead"));
}

#[cfg(unix)]
#[test]
fn posix_tz_rules_this_build_can_read_still_work() {
    // The same rules with a transition time chrono accepts: the refusal is
    // about what this build can read, not about the form itself.
    csvdt()
        .args(["-H", "-l0"])
        .env("TZ", "IST-2IDT,M3.4.4/24,M10.5.0")
        .write_stdin("ts\n2024-06-15T12:00:00Z\n")
        .assert()
        .success()
        .stdout("2024-06-15T12:00:00Z,2024-06-15T15:00:00+03:00\n");

    // And a southern-hemisphere rule, where January rather than July is the
    // daylight-saving half -- the check probes both, so neither can hide a
    // rule that was silently dropped.
    csvdt()
        .args(["-H", "-l0"])
        .env("TZ", "AEST-10AEDT,M10.1.0,M4.1.0/3")
        .write_stdin("ts\n2024-06-15T12:00:00Z\n")
        .assert()
        .success()
        .stdout("2024-06-15T12:00:00Z,2024-06-15T22:00:00+10:00\n");
}

#[cfg(unix)]
#[test]
fn a_tz_path_that_names_no_file_is_refused() {
    // glibc's other implementation-defined form. A mistyped path became
    // UTC where a mistyped name was refused.
    csvdt()
        .args(["-H", "-l0"])
        .env("TZ", ":/usr/share/zoneinfo/Europe/Nowhereistan")
        .write_stdin("ts\n2024-06-15T12:00:00Z\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("which does not exist"))
        .stderr(predicate::str::contains("Check the path"));
}

#[test]
fn an_empty_file_argument_is_refused_rather_than_read_as_stdin() {
    // "$file" that expanded to nothing used to be read as no file at all,
    // so the run silently consumed the pipe -- or waited on a terminal --
    // where every other unreadable path is named.
    csvdt()
        .arg("")
        .write_stdin("a,b\n1,2\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("file argument is empty"))
        .stderr(predicate::str::contains("unset shell variable"));
}

#[test]
fn known_limits_no_longer_clear_the_linked_aliases() {
    // CET, MET and EST5EDT are links exactly as EET and WET are, so they
    // carry the same historical gap -- EST5EDT applies today's US daylight
    // saving to years that had none. The help used to name them as the
    // safe ones.
    // Asked of the words rather than the lines: 'EET, WET, CET, MET, EST5EDT'
    // sat on one line until a rewrap put a break inside it, and the list was
    // no less complete for that. A Windows checkout gives the include_str!'d
    // help CRLF endings, which this collapses too.
    let help = help_words();
    assert!(
        !help.contains("are correct, as are CET, MET and"),
        "KNOWN LIMITS still clears the linked aliases as correct");
    assert!(
        help.contains("EET, WET, CET, MET, EST5EDT"),
        "KNOWN LIMITS should name all five gapped aliases together");
}

// ---- what a mutation of the source got past ------------------------------
//
// Each of these was found by breaking the source on purpose, one change at a
// time, and running the suite: a change nothing objects to is behaviour
// nothing pins. Ninety-four mutations, seventy killed outright; these are
// the ones that got through and turned out to matter.

#[test]
fn the_manual_page_still_lets_its_prose_fill() {
    // The .br treatment is for laid-out blocks, and the tests for it all ask
    // that a table keeps its lines. Nothing asked the other half -- that
    // everything else does not -- so two mutations of the rule went
    // unnoticed while wrecking the page: dropping the 'aligned' condition,
    // and stopping a blank line from ending a run. Both put a break between
    // every pair of lines in the file, which is 693 and 726 lines of the
    // rendered page respectively, and the suite said nothing.
    let page = String::from_utf8(
        csvdt().arg("--generate-man").output().unwrap().stdout,
    )
    .expect("the page should be UTF-8");

    // A line from the middle of DESCRIPTION's first paragraph: ordinary
    // prose, and the man page's job is to rewrap it to the reader's width.
    assert!(
        page.contains("provides flexible options"),
        "the paragraph this test is about is no longer in the page",
    );
    assert!(
        !page.contains(".br\nprovides flexible options"),
        "prose is being held at the line breaks the help file happens to \
         have, so the manual page no longer adapts to the width it is read \
         at",
    );
}

#[test]
fn a_name_that_looks_like_a_mode_is_only_a_hint_when_flexible_asked() {
    // 'csvdt -f fix' means a file called 'fix', and the message says so and
    // suggests -f=fix. The suggestion is guarded on --flexible having been
    // given at all; loosened to 'or', csvdt offers it to anyone whose file
    // happens to be called fix, keep or widest, which is advice about an
    // option they are not using.
    csvdt()
        .args(["-f", "fix"])
        .write_stdin("a,b\n1,2\n")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("If you meant the --flexible mode"));
    csvdt()
        .arg("fix")
        .write_stdin("a,b\n1,2\n")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("cannot read 'fix'"))
        .stderr(predicate::str::contains("--flexible").not());
}

#[test]
fn a_carriage_return_in_a_field_is_a_single_quote_misread() {
    // The writer quotes a field holding a record terminator, and a quoted
    // field is one '--single-quote' would read back wrong. '\r' is the one
    // terminator with its own clause in that test and no other reason to be
    // quoted, so dropping it was invisible: every other misread case is
    // reached by a delimiter, a quote or a newline instead.
    let input = "a,b\n\'x\ry\',2\n";
    csvdt()
        .args(["-H", "-p", "--single-quote"])
        .write_stdin(input)
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "row(s) hold a field that \'--single-quote\' would misread"));
}

// ---- twelfth round: exact bounds, and the edges of each probe ------------

#[test]
fn a_timestamp_padded_with_spaces_converts_without_the_field_being_altered() {
    // The parse skips whitespace around the value rather than trimming it out
    // of the field, which is what lets a ", "-separated file -- the README's
    // own example -- convert with no --trim at all. It used to arrive
    // pre-trimmed, and that default reached inside quoted fields and threw
    // away spaces the source had quoted to keep. Both halves are asserted
    // here: the conversion happens, and column 1 goes out exactly as it came.
    csvdt()
        .args(["-H", "-p", "-u1"])
        .write_stdin("id, ts\n1, 2024-01-01T00:00:00Z\n")
        .assert()
        .success()
        .stdout("id, ts,<to_utc\n\
                 1, 2024-01-01T00:00:00Z,2024-01-01T00:00:00+00:00\n");

    // Trailing whitespace is the same question from the other side -- a file
    // written with " ," separators, or one hand-edited.
    csvdt()
        .args(["-H", "-p", "-u1"])
        .write_stdin("id,ts \n1,2024-01-01T00:00:00Z \n")
        .assert()
        .success()
        .stdout("id,ts ,<to_utc\n\
                 1,2024-01-01T00:00:00Z ,2024-01-01T00:00:00+00:00\n");
}

#[test]
fn the_column_exactly_past_the_last_field_is_out_of_bound_for_every_action() {
    // Every out-of-bound test until now named a column far past the end
    // (-u9 against two fields), which a bound written '>' rather than '>='
    // catches just as well. Only the column exactly equal to the field count
    // tells the two apart, and it is the one a user actually reaches: on two
    // columns numbered 0 and 1, column 2 is the off-by-one everybody makes.
    // Loosened, it indexes one field past the end -- a panic in the row path,
    // and in the header path a column name inserted for a column that is not
    // there. Every action carries its own copy of the check, so every action
    // is asked.
    for action in [
        vec!["-r2"],
        vec!["-u2"],
        vec!["-l2"],
        vec!["-o2,Z"],
        vec!["-s2"],
        vec!["-d2"],
        vec!["-d0,2"],
        vec!["-a2"],
        vec!["--remove", "2"],
    ] {
        // The header path, on a header and nothing else: with a data row in
        // the file the row check would answer for it, and the header's own
        // bound would go untested however it was written.
        csvdt()
            .args(["-H", "-p"])
            .args(&action)
            .write_stdin("a,b\n")
            .assert()
            .failure()
            .code(1)
            .stderr(predicate::str::contains(
                "Specified column is out of bound"));

        // And the row path, with no header for the check to happen in first.
        csvdt()
            .args(&action)
            .write_stdin("1,2\n")
            .assert()
            .failure()
            .code(1)
            .stderr(predicate::str::contains(
                "Specified column is out of bound"));
    }
}

#[test]
fn a_single_quote_misread_is_counted_whatever_made_the_writer_quote() {
    // The warning was only ever tested on a first field quoted for holding
    // the delimiter, which leaves the walk over the whole row untested and
    // three of the four reasons the writer quotes with it. Each of these is
    // one row that a '--single-quote' re-read takes apart, and the output is
    // asserted too: it is the output the warning is about.
    for (arguments, input, output) in [
        // Quoted for the delimiter, but in a later column -- a quoted field
        // splits the row wherever it sits, not only at the front.
        (vec![], "a,'x,y'\n", "a,\"x,y\"\n"),
        // Quoted for nothing but the '--comment' character it holds.
        (vec!["--comment", "#"], "a#b,c\n", "\"a#b\",c\n"),
        // Quoted for nothing but the newline inside it.
        (vec![], "'a\nb',c\n", "\"a\nb\",c\n"),
    ] {
        csvdt()
            .arg("--single-quote")
            .args(&arguments)
            .write_stdin(input)
            .assert()
            .success()
            .stdout(output)
            .stderr(predicate::str::contains("1 row(s) hold a field"));
    }
}

#[test]
fn a_bare_field_that_begins_with_a_single_quote_is_counted_as_a_misread() {
    // The other half of the question, which the writer's quoting decision
    // cannot answer: ''''  reads as the one-character field ''' , and the
    // writer leaves it bare -- it holds no delimiter, no '"', no record
    // boundary. Read back with '--single-quote' that leading ''' opens a
    // quoted field and swallows what follows, so the row is lost though
    // nothing was quoted on the way out.
    csvdt()
        .arg("--single-quote")
        .write_stdin("''''\nx\n")
        .assert()
        .success()
        .stdout("'\nx\n")
        .stderr(predicate::str::contains("1 row(s) hold a field"));
}

// TZ carries no weight on Windows, which takes its zone from the system, so
// the rule form is a Unix question and so are its refusals.
#[cfg(unix)]
#[test]
fn a_tz_abbreviation_too_short_for_posix_is_not_read_as_a_rule() {
    // POSIX writes an abbreviation of three or more letters before the
    // offset, and that length is the whole of what separates a rule from a
    // misspelt zone name. Read with a two-letter minimum, 'CE1T' -- a
    // plausible slip for CET -- parses as the rule "CE" +1, and the run
    // answers UTC+1 for a zone nobody named, exit 0.
    csvdt()
        .args(["-H", "-l0"])
        .env("TZ", "CE1T")
        .write_stdin("ts\n2024-06-15T12:00:00Z\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("not a zone in the IANA database"));

    // The boundary is the test, so the shortest abbreviation POSIX does
    // write must still be taken for the rule it is.
    csvdt()
        .args(["-H", "-l0"])
        .env("TZ", "EST5EDT")
        .write_stdin("ts\n2024-06-15T12:00:00Z\n")
        .assert()
        .success()
        .stdout("2024-06-15T12:00:00Z,2024-06-15T08:00:00-04:00\n");
}

#[test]
fn the_utf16_probe_wants_eight_bytes_before_the_nul_pattern_counts() {
    // Two or three characters can alternate with NULs by chance, so the
    // signature means nothing until there is a few characters' worth of it.
    // These seven bytes match the pattern exactly and are ordinary UTF-8
    // holding real NULs, which KNOWN LIMITS promises passes through
    // untouched -- byte for byte, which is what is asserted.
    let short = b"a\0b\0c\0\n".to_vec();
    let passed = csvdt()
        .write_stdin(short.clone())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(passed, short, "NUL-bearing UTF-8 shorter than the probe's \
                               minimum should pass through untouched");

    // One byte more is the length the probe trusts, and there the same
    // pattern is UTF-16LE: the case that used to exit 0 having written
    // NUL-interleaved output.
    csvdt()
        .write_stdin(b"a\0b\0c\0d\0".to_vec())
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("every second byte is NUL"));
}

#[test]
fn each_byte_order_mark_is_named_as_the_endianness_it_actually_is() {
    // The BOM test above asserts only "looks like UTF-16", so the two
    // branches could name each other's endianness and pass unchanged. The
    // label is the whole of the advice -- it is what the reader hands iconv
    // -- and a file converted from the wrong end comes back as mojibake.
    for (content, label) in [
        (b"\xff\xfea\0,\0b\0\n\0".to_vec(),
         "UTF-16LE (it starts with a byte-order mark)"),
        (b"\xfe\xff\0a\0,\0b\0\n".to_vec(),
         "UTF-16BE (it starts with a byte-order mark)"),
    ] {
        csvdt()
            .write_stdin(content)
            .assert()
            .failure()
            .code(1)
            .stderr(predicate::str::contains(label));
    }
}

#[test]
fn the_final_newline_shim_stays_off_when_no_comment_character_is_set() {
    // The shim exists for csv-core's comment state, and it costs one
    // malformed shape: an input ending inside an unclosed quote takes the
    // fabricated \n into that field as data. That is documented under
    // '--comment', which is the only place it may happen -- with no comment
    // character nothing is appended, and the swallowed field ends exactly
    // where the input did. Asserted as exact bytes, since a fabricated
    // newline inside the field is the whole difference.
    csvdt()
        .arg("-f")
        .write_stdin("a,b\n\"x,2\n3,4")
        .assert()
        .success()
        .stdout("a,b\n\"x,2\n3,4\"\n");

    // The same input where the shim does fire, so the pair says the gate is
    // what decides rather than the input: here the extra \n lands inside the
    // unclosed field, which is the documented cost of closing the comment.
    csvdt()
        .args(["-f", "--comment", "#"])
        .write_stdin("a,b\n\"x,2\n3,4")
        .assert()
        .success()
        .stdout("a,b\n\"x,2\n3,4\n\"\n");
}

#[test]
fn a_control_character_is_refused_by_name_rather_than_printed_raw() {
    // These refusals are read in a terminal, where a raw tab or newline
    // printed between quotes says nothing about which character was meant --
    // and a tab is a character '--read-delimiter' is routinely given, so the
    // pairing that collides with it is one a TSV user really reaches.
    csvdt()
        .args(["--read-delimiter", "\t", "--comment", "\t"])
        .write_stdin("a\tb\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("was given a tab"));

    csvdt()
        .args(["--comment", "\n"])
        .write_stdin("a,b\n")
        .assert()
        .failure()
        .code(100)
        .stderr(predicate::str::contains("was given a newline"));
}

// ---- what happens when the output stream fails ---------------------------

#[test]
fn every_fatal_message_names_the_program_it_came_from() {
    // Every warning csvdt writes beside a successful run already opens with
    // "csvdt: ". The three messages that end a run did not, and opened with a
    // blank line besides -- so the one line a caller keeps, the one explaining
    // a non-zero status, was the only line that did not say who wrote it.
    // In a pipeline with stderr in a log that is unattributable: "No space
    // left on device (os error 28)" could have come from either half.
    for (arguments, code) in [
        // Refused after the command line parsed: exit 100.
        (vec!["--offset", "0,nonsense"], 100),
        // Began and could not finish: exit 1.
        (vec!["--rfc3339", "0", "/nonexistent-csvdt-input.csv"], 1),
    ] {
        let assertion = csvdt()
            .args(&arguments)
            .write_stdin("a\n1\n")
            .assert()
            .failure()
            .code(code);
        let stderr = String::from_utf8_lossy(&assertion.get_output().stderr).into_owned();
        assert!(
            stderr.starts_with("csvdt: "),
            "{arguments:?} opened its fatal message with {:?}",
            stderr.lines().next().unwrap_or(""),
        );
    }

    // And the run that finishes having converted nothing, which is the same
    // exit status reached by a different route.
    let assertion = csvdt()
        .args(["-H", "--rfc3339", "0"])
        .write_stdin("a\nnotatimestamp\n")
        .assert()
        .failure()
        .code(1);
    let stderr = String::from_utf8_lossy(&assertion.get_output().stderr).into_owned();
    assert!(
        stderr.starts_with("csvdt: nothing converted"),
        "the nothing-converted message opened with {stderr:?}",
    );
}

// Whether this machine has a device that answers every write with ENOSPC.
// Linux and the BSDs do; a container built without it does not, and the
// tests that need one say so rather than passing on a machine that could not
// have run them.
#[cfg(unix)]
fn dev_full() -> Option<std::fs::File> {
    std::fs::OpenOptions::new().write(true).open("/dev/full").ok()
}

#[test]
#[cfg(unix)]
fn a_write_that_failed_is_reported_rather_than_discarded() {
    let Some(_) = dev_full() else {
        eprintln!("no /dev/full on this machine; skipped");
        return;
    };

    // --generate-man and --list-options wrote through 'let _ = write_all',
    // which was meant to let a closed pipe pass and swallowed every other
    // failure with it. A package build runs '--generate-man > csvdt.1' and
    // reads the status: on a full disk it got 0, and installed the empty
    // file that was left.
    // --help and --version are printed by clap rather than by csvdt, and clap
    // loses a failed write the same way: those two reported 0 over output
    // nobody received for as long as they went out through get_matches. They
    // are here beside the other two because the four are one promise -- what
    // csvdt says about itself is written or the status says it was not.
    for arguments in [vec!["--generate-man"], vec!["--list-options"],
                      vec!["--help"], vec!["-h"],
                      vec!["--version"], vec!["-V"]] {
        let output = std::process::Command::new(assert_cmd::cargo::cargo_bin("csvdt"))
            .args(&arguments)
            .stdout(std::process::Stdio::from(dev_full().unwrap()))
            .stderr(std::process::Stdio::piped())
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(1),
            "{arguments:?} reported success for output that was never written",
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).starts_with("csvdt: No space left"),
            "{arguments:?} said {:?}",
            String::from_utf8_lossy(&output.stderr),
        );
    }

    // The conversion path already did report this; here so that the three
    // stay together and a change to one is measured against the others.
    let output = std::process::Command::new(assert_cmd::cargo::cargo_bin("csvdt"))
        .args(["-H", "--rfc3339", "0"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::from(dev_full().unwrap()))
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(b"a\n1700000000\n")?;
            child.wait_with_output()
        })
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).starts_with("csvdt: No space left"),
        "a run onto a full disk said {:?}",
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
#[cfg(unix)]
fn a_reader_that_stops_reading_is_not_a_failed_write() {
    // The other half of the promise above, and the half a too-eager check
    // would break: 'csvdt --help | head -1' is an ordinary thing to do and
    // nothing has gone wrong. --help and --version now go out through the same
    // writer as the option list and the page, so they answer to the same rule
    // -- a closed pipe passes, and only a real failure ends the run at 1.
    let dir = tempfile_dir().join("answer-through-a-closed-pipe");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    for (name, flag) in [("help", "--help"), ("short", "-h"),
                         ("version", "--version"),
                         ("options", "--list-options"),
                         ("page", "--generate-man")] {
        // csvdt's own status rather than the pipeline's, written down rather
        // than read from $?, since the shell reports the last command in a
        // pipe and 'set -o pipefail' is not in every /bin/sh.
        let status_file = dir.join(name);
        let script = format!(
            "{{ '{}' {}; echo $? > '{}'; }} | head -1 > /dev/null",
            assert_cmd::cargo::cargo_bin("csvdt").display(),
            flag,
            status_file.display(),
        );
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&script)
            .output()
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(&status_file).unwrap().trim(),
            "0",
            "{flag} through a closed pipe was reported as a failure: {:?}",
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

#[test]
#[cfg(unix)]
fn a_standard_output_that_cannot_be_written_is_refused_before_the_work() {
    // std answers Ok to a write on a standard stream that failed with EBADF:
    // a process without usable standard output is one whose output goes
    // nowhere, deliberately. So 'csvdt in.csv 1<out.csv' -- '<' where '>' was
    // meant -- read the file, converted every value, wrote none of it, and
    // exited 0 with a tally on stderr counting values it said it had written.
    // Nothing later in the run can notice: the flush that should have failed
    // returns Ok too, which is why this is asked once, up front.
    let dir = tempfile_dir().join("unwritable-stdout");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("in.csv");
    std::fs::write(&input, "a\n1700000000\n").unwrap();

    // Opened for reading, then handed over as standard output. The descriptor
    // exists, so std's own substitution for a missing one does not apply; it
    // simply cannot be written to.
    // Every route to standard output, including the two clap prints on its
    // way out of get_matches: those were the last to go on reporting success
    // over output nobody received, because there is nothing of csvdt's left
    // to ask by then. The question is asked once, before any parsing.
    for arguments in [
        vec!["-H", "--rfc3339", "0", input.to_str().unwrap()],
        vec!["--generate-man"],
        vec!["--list-options"],
        vec!["--help"],
        vec!["--version"],
    ] {
        let output = std::process::Command::new(assert_cmd::cargo::cargo_bin("csvdt"))
            .args(&arguments)
            .stdout(std::process::Stdio::from(std::fs::File::open(&input).unwrap()))
            .stderr(std::process::Stdio::piped())
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(1),
            "{arguments:?} reported success having written nothing",
        );
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("standard output cannot be written to"),
            "{arguments:?} said {:?}",
            String::from_utf8_lossy(&output.stderr),
        );
    }

    // The file it was pointed at is untouched, which is the other half of the
    // promise: a refused run writes nothing anywhere.
    assert_eq!(std::fs::read_to_string(&input).unwrap(), "a\n1700000000\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[cfg(unix)]
fn a_reader_that_stops_reading_still_ends_the_run_at_zero() {
    // The check above must not have made an ordinary pipeline into a failure:
    // a closed pipe is the one thing that ends a run at 0 regardless, and it
    // is the most ordinary thing anyone does with a command-line tool.
    let dir = tempfile_dir().join("closed-pipe");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("in.csv");
    // Larger than a pipe will hold, so head really does close on csvdt rather
    // than letting a small output through whole. Every value is unconvertible,
    // so the full run reports 1 -- which is the status the closed pipe cuts
    // short, and the reason this is worth its own paragraph in EXIT STATUS.
    let mut rows = String::from("a\n");
    for _ in 0..60_000 {
        rows.push_str("notatimestampatallhereokay\n");
    }
    std::fs::write(&input, &rows).unwrap();

    // csvdt's own status rather than the pipeline's, and written down rather
    // than read from $?, since the shell reports the last command in a pipe
    // and 'set -o pipefail' is not in every /bin/sh.
    let status_file = dir.join("status");
    let script = format!(
        "{{ '{}' -H --rfc3339 0 '{}'; echo $? > '{}'; }} | head -3 > /dev/null",
        assert_cmd::cargo::cargo_bin("csvdt").display(),
        input.display(),
        status_file.display(),
    );
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&script)
        .output()
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&status_file).unwrap().trim(),
        "0",
        "a closed pipe was reported as a failure: {:?}",
        String::from_utf8_lossy(&output.stderr),
    );

    // And the same file read to the end does report the failure, so the test
    // above is measuring the closed pipe rather than a run that never failed.
    csvdt()
        .args(["-H", "--rfc3339", "0", input.to_str().unwrap()])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("nothing converted"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_exit_statuses_say_a_closed_pipe_is_the_exception_to_them() {
    // The section documented 0, 1, 2, 100 and even 128 plus a signal, and
    // said of the nothing-converted status that "the output is written in
    // full regardless" -- with nothing anywhere about the one case that
    // overrides all of it. A caller reading it and then running the tool
    // under 'head' would have found the contradiction the hard way.
    csvdt()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("a reader that stops"))
        .stdout(predicate::str::contains("head -3"));
}

#[test]
#[cfg(unix)]
fn a_standard_input_that_cannot_be_read_is_refused_rather_than_read_as_empty() {
    // The other half of the same gap: std hides EBADF on a read too, and
    // hides it as end of input, which an input is allowed to be. So
    // 'csvdt -H -r0 0>out.csv' -- '>' where '<' was meant, which has the
    // shell empty out.csv on the way past -- read nothing, wrote nothing and
    // exited 0, exactly as a run over an empty file does.
    let dir = tempfile_dir().join("unreadable-stdin");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let sink = dir.join("out.csv");

    let output = std::process::Command::new(assert_cmd::cargo::cargo_bin("csvdt"))
        .args(["-H", "--rfc3339", "0"])
        // Opened for writing, then handed over as standard input.
        .stdin(std::process::Stdio::from(std::fs::File::create(&sink).unwrap()))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "an unreadable standard input was reported as an empty one",
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("standard input cannot be read from"),
        "said {:?}",
        String::from_utf8_lossy(&output.stderr),
    );

    // A genuinely empty input is still the ordinary success it has always
    // been, so the check is measuring the descriptor rather than the bytes.
    csvdt()
        .args(["-H", "--rfc3339", "0"])
        .write_stdin("")
        .assert()
        .success()
        .stdout("")
        .stderr("");

    let _ = std::fs::remove_dir_all(&dir);
}

// Runs BINARY with ARGUMENTS over FILE, with the shell redirecting onto that
// same file, and gives back what the run said and what the file holds after.
//
// Through a shell on purpose: the hazard is the shell opening the redirection
// before csvdt is started, which is what empties the file, and no amount of
// Command::stdout reproduces that ordering honestly.
//
// A batch file on Windows rather than 'cmd /C', whose quoting rules for a
// command line holding quoted paths are their own subject. Written out and
// run, the redirection is unambiguous and the test is about csvdt.
#[cfg(unix)]
fn through_a_shell(
    binary: &std::path::Path,
    arguments: &str,
    input: &str,
    redirection: &str,
    output: &str,
    _dir: &std::path::Path,
) -> std::process::Output {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("'{}' {arguments} '{input}' {redirection} '{output}'",
                     binary.display()))
        .output()
        .unwrap()
}

#[cfg(windows)]
fn through_a_shell(
    binary: &std::path::Path,
    arguments: &str,
    input: &str,
    redirection: &str,
    output: &str,
    dir: &std::path::Path,
) -> std::process::Output {
    let script = dir.join("redirect.bat");
    std::fs::write(&script, format!(
        "@echo off\r\n\"{}\" {arguments} \"{input}\" {redirection} \"{output}\"\r\n",
        binary.display(),
    )).unwrap();
    std::process::Command::new("cmd")
        .arg("/C")
        .arg(&script)
        .output()
        .unwrap()
}

#[test]
fn an_input_file_that_is_also_the_output_is_refused() {
    // A shell opens a redirection before csvdt is started, so
    // 'csvdt in.csv > in.csv' handed over a file already truncated to
    // nothing: csvdt read an empty file, wrote nothing and exited 0, and the
    // only sign anything had happened was that the data was gone.
    // 'csvdt in.csv >> in.csv' is the other half, and there the file is still
    // whole -- left to run it came out as the input followed by a conversion
    // of the input, also at 0.
    let dir = tempfile_dir().join("input-is-output");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let binary = assert_cmd::cargo::cargo_bin("csvdt");
    let contents = "id,ts\n1,1700000000\n2,1700000001\n";

    let refused = |name: &str, redirection: &str, arguments: &str| {
        let file = dir.join(name);
        std::fs::write(&file, contents).unwrap();
        let name = file.display().to_string();
        let output =
            through_a_shell(&binary, arguments, &name, redirection, &name, &dir);
        (output, std::fs::read_to_string(&file).unwrap())
    };

    // Appending: the file is still whole when this is asked, so refusing
    // keeps it that way.
    let (output, after) = refused("append.csv", ">>", "-H -p --rfc3339 1");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("is both the input and where the output is going"),
        "said {:?}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert_eq!(after, contents, "the input was not left as it was found");

    // Truncating: the data is already gone by the time csvdt runs, and the
    // point of the refusal is that something finally says so.
    let (output, _) = refused("truncate.csv", ">", "-H -p --rfc3339 1");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("is both the input and where the output is going"),
        "said {:?}",
        String::from_utf8_lossy(&output.stderr),
    );

    // And the counting pass of --flexible=widest reaches it too, which opens
    // the file by a route of its own.
    let (output, _) = refused("widest.csv", ">>", "-f=widest");
    assert_eq!(output.status.code(), Some(1));

    // By identity rather than by path, so another name for the same file is
    // the same file. Unix only, and not because Windows cannot do it: a
    // symbolic link there wants a privilege a test runner may not have, and
    // a test that skips where it is unprivileged reads as one that passed.
    // What the identity is made of is asked of the program on both sides --
    // device and inode here, volume serial and file index there -- and the
    // redirections above exercise that on either platform.
    #[cfg(unix)]
    {
        let target = dir.join("linked.csv");
        std::fs::write(&target, contents).unwrap();
        let link = dir.join("by-another-name.csv");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(
                "'{}' -H -p --rfc3339 1 '{}' >> '{}'",
                binary.display(), link.display(), target.display(),
            ))
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), contents);
    }

    // Two different files are the ordinary case and stay one.
    let input = dir.join("in.csv");
    std::fs::write(&input, contents).unwrap();
    let elsewhere = dir.join("out.csv");
    let output = through_a_shell(
        &binary, "-H -p --rfc3339 1", &input.display().to_string(), ">",
        &elsewhere.display().to_string(), &dir);
    assert!(output.status.success(), "an ordinary redirection was refused");
    assert!(std::fs::read_to_string(&elsewhere).unwrap().contains("2023-11-14"));

    // A character device is one inode however often it is named, and has
    // nothing to lose, so it is not the case this refuses. Named for what it
    // is on Unix; the Windows guard asks the same question of a file type and
    // answers it the same way, but there is no NUL that reports itself as a
    // regular file to try it on.
    #[cfg(unix)]
    {
        let output = through_a_shell(
            &binary, "-H --rfc3339 0", "/dev/null", ">", "/dev/null", &dir);
        assert!(output.status.success(), "/dev/null both ways was refused");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_limits_say_the_input_file_cannot_also_be_the_output() {
    // A refusal a caller meets for the first time in the middle of a job is
    // worth finding before it: there is no in-place mode, and the help now
    // says so where the rest of what csvdt will not do is collected.
    csvdt()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("THE INPUT FILE CANNOT ALSO BE THE OUTPUT"))
        .stdout(predicate::str::contains("mv out.csv in.csv"));
}
