# Changelog

## 1.0.0

First release.

csvdt reads CSV, converts the timestamps in it, and writes CSV back. It is
meant for the point in an investigation where a tool has exported a table
whose time column is in some format nothing else will accept, and the work is
to turn it into a column that can be sorted, filtered and compared — without
opening the file in a spreadsheet, and without a script whose behaviour on the
awkward rows is nobody's to explain.

Two things follow from being used that way, and both are decisions rather than
omissions.

**Nothing is approximated.** Durations (`-d`) are reported in days, hours,
minutes and seconds and never in weeks, months or years, because those units
have no fixed length: a month is 28 to 31 days and a year 365 or 366, so a
duration expressed in them has been silently rounded to somebody's average.
Every value csvdt prints is exact and reproducible, which is what it takes for
a number to be relied on as evidence.

**Zone rules travel with the binary.** `-l/--local` resolves zone names
against the IANA database compiled in, not against the one installed on the
machine, so the same input converts identically on any host — and `csvdt
--version` names the database it carries. `-u`, `-o`, `-d`, `-s` and `-r`
involve no zone rules at all.

### Reading

- **`-H/--has-header`** treats the first row as a header, and **`-p`** prints
  it back with the output. Without `-H` there is no header: the first row is
  data and is converted like the rest.
- **`--read-delimiter`** takes a separator other than a comma, by character or
  by name. **`--single-quote`** reads `'` as the quote character.
- **`--comment`** takes a character that makes a line a comment. Comments are
  skipped, and what a comment is applies to the line rather than to a field
  inside quotes.
- **`--trim`** removes whitespace, headers and fields chosen separately.
- **`-f/--flexible`** allows rows with a different number of fields instead of
  stopping. Plain, it passes ragged rows through unchanged; `=fix` pads short
  rows out to the width of the first record in one pass; `=widest` reads the
  file twice and pads out to the widest record.
- **`--fill-log`** writes down which rows the filling modes padded, or would
  have padded, as CSV with a line number, a record number, the row's own field
  count, the width it was measured against, the signed difference, and where
  the row stands. The filled fields cannot be told from empty fields the source
  really contained, so a filled copy is not a faithful reproduction and the log
  is the only account of what changed.

### Converting

- **`-r/--rfc3339`**, **`-u/--utc`** and **`-l/--local`** convert a column of
  epoch timestamps to RFC 3339, to UTC, and to a named zone.
- **`-o/--offset`** shifts a column by a fixed offset. **`-s/--split`** splits a
  timestamp column into date and time. **`-d/--duration`** reports the interval
  between two columns, or between a column and each following row.
- **`--round-seconds`** drops sub-second precision rather than carrying digits
  the source did not mean.
- A value that will not parse is reported with the line, the record, the column
  and the text that failed, and the run stops. It is never passed through, and
  never replaced with something that looks like a timestamp.

### Writing

- **`-i/--insert`** puts a converted column at a chosen position instead of
  replacing the original; **`--remove`** drops columns and **`-a/--arrange`**
  reorders them.
- **`--print-delimiter`** and **`--quote`** choose the output separator and
  quoting style. The output is canonical CSV rather than a copy of the input:
  quoting is normalised, so a field that needed quotes gets them and one that
  did not loses them.
- **`--peek`** numbers a file's columns and shows what converting each would
  produce, so the column numbers the other options take can be read off the
  file rather than counted by hand.

### Around the program

- **`-h`** is a summary and **`--help`** the full text; the short form is a
  strict subset of the long one. **`--generate-man`** writes the manual page,
  and the packages install it.
- **`--list-options`** prints the option surface as a table, for wrapping csvdt
  from another program without parsing the help.
- An **Emacs front end** in `emacs/`, packaged separately, runs csvdt over a
  buffer or a selection and names what it resolved.

### Provenance

A released version is rebuilt every month, because the time zone database
inside it goes stale even though its source does not change. So the version
number alone does not identify a binary, and three kinds of release exist to
say which is which: the version's own release holds what that version was
published with, `release` holds the current build of it, and every distinct
build is kept permanently under a tag naming its version, commit and time zone
database — `1.0.0-2026.09.abc1234-tzdata2025b`. `csvdt --version` prints the
first two of those, and each release carries `BUILD-INFO`, `Cargo.lock` and
`VERSION` describing what it was made from. To cite the build behind a result,
keep the output of `csvdt --version` beside it.

### In this repository

- `src/` — the program. The help text is a corpus of files under `src/help/`
  rather than string literals, and the manual page is rendered from it.
- `tests/` — the test suite, run by `cargo test`.
- `checks/` — differential checks against Python's `csv` and `datetime`, and
  against this documentation, over generated input. Not the test suite; run by
  hand before a release. See `checks/README.md`.
- `emacs/` — the front end and its own suite.
- `packaging/rpm/` — the spec the Fedora packages are built from.
