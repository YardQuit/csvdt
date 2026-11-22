# Changelog

## Unreleased

`--lines RANGES` converts only the records on the lines named, for example
`--lines 12-40` or `--lines 3-5,9,20-22`, counting physical lines from 1 as
an editor does. The output is what a full run writes for the records that
have a line selected, each once and in input order, with the header written
once under `-p`. A record running over several lines is taken whole, and
csvdt then says on stderr which lines it took:

    csvdt: '--lines' widened to whole records, taking lines 3-4

The input is read as a full run reads it, so comment lines, blank lines and
quoted newlines fall where they would. `-d` with one column measures the
first selected row from the row before it, `--flexible=fix` and `widest`
measure against the whole input, and the fill log numbers lines as in the
whole input but logs only the records taken. Records outside the selection
do not stop the run, and reading stops after the last record taken. A
malformed value is refused with status 100; `--lines` cannot be used with
`--peek`.

## 1.1.0

The releases now carry `THIRD-PARTY-LICENSES.txt`, the licence texts of the
crates compiled into the prebuilt binaries, as those licences require. It
is one file when every platform uses the same crates and one file per
platform otherwise.

The front end for the Tuatara editor lives with Tuatara's own packages, in
[Tuatara-Editor/tuatara-packages](https://github.com/Tuatara-Editor/tuatara-packages),
loaded with `(load-package "csvdt" :source "tuatara-packages")`: csvdt run
over a buffer, a selection or Tuatara's banked lines, with the same rules as
the Emacs one about whole records and the header, and the header, quote and
delimiter option names and no others. It runs csvdt as a background job, so
a long run leaves the editor free and `M-x stop-job` ends it.

### Time zone database

The bundled IANA database moves from tzdata 2025b to 2026f, so `-l/--local`
answers differently for 49 zones at the moments listed here. Every other
zone, and every other action, answers as before. `csvdt --version` names the
release. A result produced with an earlier build is not wrong for that build:
it followed the rules IANA had published then. To reproduce one, use the
build that made it; to re-examine one, check its timestamps against this list.

Past timestamps, where IANA corrected the historical record:

- Brazil, 1965 and 1966: America/Araguaina, Bahia, Belem, Boa_Vista,
  Campo_Grande, Cuiaba, Eirunepe, Fortaleza, Maceio, Manaus, Noronha,
  Porto_Acre, Porto_Velho, Recife, Rio_Branco, Santarem and Sao_Paulo, and
  Brazil/Acre, Brazil/DeNoronha, Brazil/East and Brazil/West. Daylight saving
  ended a day later in 1965, on 1 April rather than 31 March, and an hour
  later on 1 March 1966; those hours now read an hour ahead.
- Ireland, 1925: Europe/Dublin and Eire. Summer time ended on 20 September
  rather than 4 October, so the two weeks between read +00:00, not +01:00.
- Colombia, 1992: America/Bogota reads -04:00 for one more day, 2 to 3 May.
- Iran, 1979: Asia/Tehran and Iran read +04:30 for one day, 25 to 26 May,
  where they read +03:30.
- Baja California, 1953 and 1961 to 1975: America/Tijuana, America/Ensenada,
  America/Santa_Isabel and Mexico/BajaNorte kept daylight saving in those
  summers, -07:00 where they read -08:00.
- Moldova, from 2022: Europe/Chisinau and Europe/Tiraspol change their clocks
  at 01:00 UTC, as the rest of Europe does, rather than an hour earlier, so
  the hour from 00:00 to 01:00 UTC on each day of change since March 2022
  reads differently, and so will those to come.
- EST5EDT, CST6CDT, MST7MDT and PST8PDT before 1967: they are no longer links
  to America/New_York, Chicago, Denver and Los_Angeles but zones holding the
  US rules alone, so they read standard time where those cities kept daylight
  saving, and their fixed offset where the cities kept local mean time, which
  was marked `parse_err`.

Future timestamps, where the places themselves changed their rules:

- Morocco: Africa/Casablanca and Africa/El_Aaiun keep +00:00 from 20
  September 2026, where they read +01:00 outside Ramadan.
- Alberta and the Northwest Territories: America/Edmonton, America/Inuvik,
  America/Yellowknife and Canada/Mountain stay on -06:00 from 1 November 2026.
- British Columbia: America/Vancouver and Canada/Pacific stay on -07:00 from
  1 November 2026.
- Manitoba: America/Winnipeg, America/Rainy_River and Canada/Central stay on
  -05:00 from 1 November 2026.
- Cuba: America/Havana and Cuba stay on -04:00 from 1 November 2026 to 14
  March 2027.

A name the database added, Factory, is now accepted as a zone.

### Changed

- POSIX rules in `TZ` are read in full, on every platform. Rules this build
  could not follow, transition times past 24:00 among them (Asia/Jerusalem's
  own `IST-2IDT,M3.4.4/26,M10.5.0`), used to be refused; they now convert.
  Rules that will not parse are refused, with the reason. On Windows, rules
  were ignored for the system's zone before.
- A `TZ` path naming a file that holds no zone is refused. It used to be
  passed over in silence for the system's own zone.
- `-` as FILE reads standard input, as leaving FILE out does. A file named
  `-` is given as `./-`.
- Year 0000 is outside the calendar for every action: `-s` and `-d` now write
  `parse_err` for it, as `-u`, `-l` and `-o` already did.
- The `--offset` target is read by its shape alone, `Z` or `+HH:MM`, so a
  fraction of a second or trailing blanks in it are refused.
- `--peek` shows control characters and bidirectional formatting characters by
  code point instead of passing them to the terminal.
- Rows reach a terminal as they are converted, so `tail -f log.csv | csvdt`
  shows each row when it arrives. Output to a pipe or a file is buffered as
  before.
- Plain runs take about half the time they did: the `--quote` and `--trim`
  styles are worked out once at start-up instead of for every field.
- The out-of-bound message names the record for every action and for the
  header, not only for `-a`, and `-d` quotes the whole value it could not read.
- The Emacs front end runs csvdt in the background, so Emacs stays free during
  a long run; a failed run is reported in the echo area. Placing many selected
  ranges takes one pass over the buffer instead of one per range. With
  `--comment` in the arguments a region run is refused, and the new option
  `csvdt-comment-options` lists the names that count.

### Fixed

- The fill log's line numbers are right after comment lines, blank lines,
  quoted fields holding newlines and in CRLF files; the out-of-bound message
  uses the same numbers.
- `--fill-log` naming a FIFO no longer hangs, and a bad fill-log destination
  is refused before standard input is copied to the spool.
- File names, `--fill-log` paths and a `TMPDIR` that are not UTF-8 work.
- The spool file's name carries a random part, so another account cannot
  claim it first.
- On Windows the fill log is refused when it is the input or the output, as on
  Unix.
- A `TZ` path that exists but is not a regular file, such as `/dev/null`, is
  refused as what it is rather than as a file that does not exist.
- The Emacs front end refuses any word in the arguments that is neither an
  option nor an option's value, by name, since it gives csvdt the input file
  itself. Only a file was refused before, and anything else reached csvdt,
  whose message named the temporary file. A value written apart from
  `--flexible`, as in `-f fix`, is shown joined: `-f=fix`.

### Building

- Dates, times and zones come from jiff and the database jiff-tzdb bundles,
  in place of chrono and chrono-tz, which are no longer maintained
  (RUSTSEC-2026-0543, RUSTSEC-2026-0555); chrono-tz's database stopped at
  tzdata 2025b. Reading and writing timestamps is otherwise unchanged.
- `Cargo.lock` is committed and every build uses `--locked`. The monthly build
  updates only jiff-tzdb, which carries the time zone database, and refuses to
  publish if any other crate moved. `cargo audit` runs on every change to the
  lock and before each monthly build; `.github/audit-ignore` lists the
  advisories that do not apply, each with its reason.

## 1.0.0

First release.

csvdt reads CSV, converts the timestamps in it, and writes CSV back. It is for
the point in an investigation where a tool has exported a table whose time
column is in a format nothing else accepts, and the column has to become one
that can be sorted, filtered and compared, without a spreadsheet and without a
script whose behaviour on awkward rows nobody can explain.

Two properties are deliberate.

- Nothing is approximated. Durations (`-d`) are reported in days, hours,
  minutes and seconds and never in weeks, months or years, because those units
  have no fixed length. Every value csvdt prints is exact and reproducible.
- Zone rules travel with the binary. `-l/--local` resolves zone names against
  the IANA database compiled in, not the one installed on the machine, so the
  same input converts identically on any host. `csvdt --version` names the
  database it carries. `-u`, `-o`, `-d`, `-s` and `-r` use no zone rules.

### Reading

- `-H/--has-header` treats the first row as a header, and `-p` prints
  it back with the output. Without `-H` there is no header: the first row is
  data and is converted like the rest.
- `--read-delimiter` takes a separator other than a comma, by character or
  by name. `--single-quote` reads `'` as the quote character.
- `--comment` takes a character that makes a line a comment. Comments are
  skipped, and what a comment is applies to the line rather than to a field
  inside quotes.
- `--trim` removes whitespace, headers and fields chosen separately.
- `-f/--flexible` allows rows with a different number of fields instead of
  stopping. Plain, it passes ragged rows through unchanged; `=fix` pads short
  rows out to the width of the first record in one pass; `=widest` reads the
  file twice and pads out to the widest record.
- `--fill-log` writes down which rows the filling modes padded, or would
  have padded, as CSV with a line number, a record number, the row's own field
  count, the width it was measured against, the signed difference, and where
  the row stands. The filled fields cannot be told from empty fields the source
  really contained, so a filled copy is not a faithful reproduction and the log
  is the only account of what changed.

### Converting

- `-r/--rfc3339` converts a column of Unix epoch seconds to RFC 3339.
  `-u/--utc` and `-l/--local` take RFC 3339 timestamps and write them in UTC
  and in a named zone.
- `-o/--offset` shifts a column by a fixed offset. `-s/--split` splits a
  timestamp column into date and time. `-d/--duration` reports the interval
  between two columns, or between a column and each following row.
- `--round-seconds` drops sub-second precision rather than carrying digits
  the source did not mean.
- A value that will not parse is written as the literal `parse_err` and the
  run goes on to the next row. How many failed is reported on stderr at the
  end. It is never passed through, and never replaced with something that
  looks like a timestamp. The run exits 0, unless nothing converted at all, in
  which case it exits 1, since that is indistinguishable from success
  otherwise.

### Writing

- `-i/--insert` puts a converted column at a chosen position instead of
  replacing the original; `--remove` drops columns and `-a/--arrange`
  reorders them.
- `--print-delimiter` and `--quote` choose the output separator and
  quoting style. The output is canonical CSV rather than a copy of the input:
  quoting is normalised, so a field that needed quotes gets them and one that
  did not loses them.
- `--peek` numbers a file's columns and shows what converting each would
  produce, so the column numbers the other options take can be read off the
  file rather than counted by hand.

### Around the program

- `-h` is a summary and `--help` the full text; the short form is a
  strict subset of the long one. `--generate-man` writes the manual page,
  and the packages install it.
- `--list-options` prints the option surface as a table, for wrapping csvdt
  from another program without parsing the help.
- An Emacs front end in `emacs/`, packaged separately, runs csvdt over a
  buffer or a selection and names what it resolved.

### Provenance

A released version is rebuilt every month, because the time zone database
inside it goes stale even though its source does not change. So the version
number alone does not identify a binary, and three kinds of release exist to
say which is which: the version's own release holds what that version was
published with, `release` holds the current build of it, and every distinct
build is kept permanently under a tag naming its version, commit and time zone
database, `1.0.0-2026.09.abc1234-tzdata2025b`. `csvdt --version` prints the
first two of those, and each release carries `BUILD-INFO`, `Cargo.lock` and
`VERSION` describing what it was made from. To cite the build behind a result,
keep the output of `csvdt --version` beside it.

### In this repository

- `src/`: the program. The help text is a corpus of files under `src/help/`
  rather than string literals, and the manual page is rendered from it.
- `tests/`: the test suite, run by `cargo test`.
- `checks/`: differential checks against Python's `csv` and `datetime`, and
  against this documentation, over generated input. Not the test suite; run
  before a release, by the monthly build and by hand. See `checks/README.md`.
- `emacs/`: the front end and its own suite.
- `packaging/rpm/`: the spec the Fedora packages are built from.
