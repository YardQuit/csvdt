# csvdt

A command-line tool for parsing and working with CSV files. It provides flexible
options for handling e.g. whitespace, quote styles, and is mainly focusing on
handling timestamps making data analysis easier and more efficient.

## Install

### Fedora, RHEL, CentOS Stream

```bash
sudo dnf install https://github.com/YardQuit/csvdt/releases/download/rpm-release/csvdt.x86_64.rpm
```

That is the whole install: `/usr/bin/csvdt`, and `man csvdt` works.

The Emacs front end is a separate and optional package, `emacs-csvdt.noarch.rpm`
from the same release, which autoloads itself rather than costing every
start-up a load. Name it in the same command as `csvdt`: it requires that
exact version-release, and given as a URL rather than out of a repository
there is nowhere for `dnf` to resolve that from.

```bash
sudo dnf install \
  https://github.com/YardQuit/csvdt/releases/download/rpm-release/csvdt.x86_64.rpm \
  https://github.com/YardQuit/csvdt/releases/download/rpm-release/emacs-csvdt.noarch.rpm
```

The packages are built from [`packaging/rpm/`](packaging/rpm/) in whatever
Fedora is current that day, and the release holds one generation:
`csvdt.x86_64.rpm` is a stable name for a file that is really
`csvdt-1.0.0-1.fc44.x86_64.rpm` today and `.fc45` after Fedora 45 ships.
`rpm-release` is the newest released version, rebuilt when a version is
tagged and again each month so the time zone database compiled into it does
not go stale.

There is no package built from `master`. What an analyst installs to produce a
result should be a release, so a release is the only thing that gets packaged.

Only the current Fedora is served; [the packaging
notes](packaging/rpm/README.md) say what that means if you are on the older
one, and how to build the spec yourself against any release.

Every released version also keeps the packages it was published with, on its
own release page — written once and never replaced. Those are what that
version was released as; the link above is what is current.

### Prebuilt binary — any Linux, macOS, Windows

Published as a rolling `release` release, so these URLs always point at the
current build of the newest version and are safe to script against.

Every released version also keeps the binaries it was published with, on its
own release page, which is the release GitHub designates "Latest" — so
`releases/latest/download/csvdt-x86_64-unknown-linux-musl` gets the binaries
that version shipped with, where `releases/download/release/…` below gets the
current build of it. The two forms mean different things on purpose.

Reach for a version's own binaries when reproducing an earlier result rather
than building the source tarball: the binary carries the IANA database it was
compiled with, and a build today resolves a newer one, so the same version
rebuilt can answer `-l` differently for any zone whose rules have changed
since. The trade runs the other way for a machine being set up now — a
version's own binaries are frozen at release time, so `release` is where the
current zone rules are.

**Linux is the intended platform.** It is what csvdt is developed and packaged
on, what every differential check runs against before a release, and where any
behaviour described here is expected to hold. macOS and Windows are supported
on a best-effort basis: they are built and tested on every commit, and a
report about either is worth making, but they get less of everything —
attention, coverage, and platform-specific work. Where a behaviour cannot be
had on one of them, that is written down rather than left to be discovered.

| platform | file |
| --- | --- |
| Linux x86-64 (static, any distro) | `csvdt-x86_64-unknown-linux-musl` |
| macOS Apple Silicon | `csvdt-aarch64-apple-darwin` |
| macOS Intel | `csvdt-x86_64-apple-darwin` |
| Windows x86-64 | `csvdt-x86_64-pc-windows-msvc.exe` |

Those names carry a target triple because one release page holds every
platform. **The installed command is `csvdt`** — every example below calls it
that, as does the Emacs front end, whose `csvdt-executable` looks up `csvdt`
on `PATH` by default. So rename it on the way in, after checking it, since
`SHA256SUMS` lists the assets under their published names:

```bash
base=https://github.com/YardQuit/csvdt/releases/download/release
curl -LO $base/csvdt-x86_64-unknown-linux-musl
curl -LO $base/SHA256SUMS
sha256sum -c SHA256SUMS --ignore-missing

mkdir -p ~/.local/bin
mv csvdt-x86_64-unknown-linux-musl ~/.local/bin/csvdt
chmod +x ~/.local/bin/csvdt
```

That URL is `releases/download/release`, **not** `releases/latest/download`.
The two look alike and behave differently: `latest/download` follows whatever
release GitHub currently designates "Latest", which is the newest version's
own release and holds the binaries that version was published with.
`download/release` names the rolling release's tag, so it resolves to the
current build of that same version.

`~/.local/bin` has to be on your `PATH`; for a machine-wide install use
`/usr/local/bin` and `sudo`. On macOS the checksum tool is `shasum -a 256`
rather than `sha256sum`. On Windows, rename the download to `csvdt.exe` and
put it in a directory on `PATH`.

### The manual page, from any install

The RPM installs one. Every other route can write its own, because csvdt
renders the page itself — `--generate-man` prints roff to standard output and
does nothing else, so the whole of it is a redirect:

```bash
mkdir -p ~/.local/share/man/man1
csvdt --generate-man > ~/.local/share/man/man1/csvdt.1
man csvdt
```

`~/.local/share/man` is on the default manpath on most systems; where it is
not, `MANPATH` covers it. System-wide is the same line into
`/usr/local/share/man/man1/`, followed by `mandb` on distributions that cache.

Worth it for two reasons beyond having the page at all. It is rendered by the
binary that answers your commands, from the same definitions that parse them,
so it cannot describe an option this build lacks or miss one it has — a page
obtained separately can drift from its binary; this one cannot. And it is the
whole help laid out for reading: `--help` runs to about 100 columns and never
rewraps, where the page adapts to the terminal and arrives in a pager, so
`KNOWN LIMITS` or `EXIT STATUS` is a `/` away rather than a scroll.

### From source

`cargo build --release` and `cargo install --path .` both produce a binary
already called `csvdt`, so none of the renaming above applies.

## How much help you want

`-h` is the summary: the introduction, then one sentence for each option and
the shape of the value it takes. It fits on a screen, which is the point —
it answers "which flag, and does the column come before or after the mode".

`--help` is the whole of it: every option in full, worked examples, the
timestamp formats read and written, the known limits, and the exit statuses.
Some nine hundred lines, so pipe it to a pager or grep it.

The manual page is `--help` laid out for reading, plus the sections neither
form of help can carry — the environment csvdt reads, the one file it writes,
and what to reach for when csvdt is the wrong tool. Two lines to install it,
[above](#the-manual-page-from-any-install).

The summaries in `-h` are not a second set of descriptions. Each is the
sentence the option's own help opens with, taken from it, so the short form
cannot come to describe a build the long form does not — the same reason the
page is generated rather than kept by hand. One text, three depths.

## Which build am I running?

```bash
$ csvdt --version
csvdt 1.0.0 (IANA tzdata 2025b)                       # built from source

$ csvdt --version
csvdt 1.0.0+2026.09.abc1234 (IANA tzdata 2025b)       # a published build
```

Reading that string:

| part | meaning |
| --- | --- |
| `1.0.0` | the program version |
| `+2026.09.abc1234` | build metadata: the year and month it was built, and the commit it was built from. Only a published build carries it; one built from source has nothing to identify but itself |
| `IANA tzdata 2025b` | the time zone database compiled into this binary, which determines every `-l/--local` result |

The `+` part is semver build metadata, not a release. A scheduled rebuild
changes only the bundled time zone database or the compiler, never the program,
so the version stays put and the metadata distinguishes the build. Together
with the database name it is also the archive tag that build was kept under:
`1.0.0+2026.09.abc1234` with `tzdata 2025b` is the release
`1.0.0-2026.09.abc1234-tzdata2025b`.

## Am I up to date?

The current published build reports itself in a plain-text asset, so this is one
request with nothing to parse:

```bash
curl -sL https://github.com/YardQuit/csvdt/releases/download/release/VERSION
```

That is byte-for-byte what `csvdt --version` prints for that build, so comparing
is a straight string comparison:

```bash
[ "$(csvdt --version)" = "$(curl -sL $base/VERSION)" ] \
  && echo "up to date" || echo "a newer build exists"
```

For the full provenance of a build — commit, compiler, and a hash of the
resolved dependency set:

```bash
curl -sL https://github.com/YardQuit/csvdt/releases/download/release/BUILD-INFO
```

## Releases

Only a released version is ever built. `master` has no binaries and no
packages: what an analyst installs to produce a result should be a release.

The newest release is rebuilt on the first of each month, because `-l/--local`
resolves zones against the IANA database compiled into the binary and so only
knows the rules that existed when it was made. The tag does not move, but what
a build of it contains does — `Cargo.lock` is not committed, so each rebuild
resolves the current `chrono-tz` and with it the current zone rules. A rebuild
is published only when the dependencies or the compiler actually changed, so a
month in which nothing moved has no release of its own.

That means the version number alone does not identify a binary, and three
kinds of release exist so that something does:

- **`1.0.0`** — the version's own release. Source tarball, plus the binaries
  and packages that version was published with, written once and never
  replaced. This is the release GitHub designates "Latest".
- **`release`** and **`rpm-release`** — the current build of the newest
  version, replaced in place. Use these for downloads and scripts.
- **`1.0.0-2026.09.abc1234-tzdata2025b`** — every distinct build, kept
  permanently and never replaced: the binaries for all four platforms *and*
  the RPM packages from the same build. The tag names the version, the commit
  and the time zone database, so it can be quoted in a report and read back
  without following a link. Use one of these to reproduce a result with the
  exact build that produced it, whichever way it was installed.

Every release carries `SHA256SUMS`, `VERSION`, `BUILD-INFO` and the
`Cargo.lock` the build resolved, and the ones with packages on them add
`RPM-BUILD-INFO` and `SHA256SUMS-rpms`. Between them they are enough to stand
the build environment up again and compile the same source with the same
dependencies.

The packages are compiled separately, inside Fedora, from their own dependency
resolution, so a month's RPM can carry a different time zone database from
that month's binaries. When that happens the two land on different archive
entries rather than sharing one — each entry names the database its own
assets actually hold.

## Columns

Every action takes a column number and nothing in a CSV file states one, so
`--peek` numbers them for you and shows the top of the file:

```bash
$ printf 'when,who,level\n2026-01-01T00:00:00Z,ada,info\n' | csvdt -H -p --peek
0                     1    2
when                  who  level
2026-01-01T00:00:00Z  ada  info
```

`-H` and `-p` mean here what they mean everywhere else, so what you see is what
a run would work on. Plain `--peek` shows the numbering and the file's first
record, since without `-H` that record is data. With `-H` the reader takes it
as a header instead, so what you see is the first *data* row — the file's
second line — and `-p`, which is what asks for a header to be written, puts the
header back above it. Claiming `-H` over a file that has none therefore shows
the second line, for the same reason a run would convert from there.

It converts nothing and writes no CSV: the padding is there to be read,
not passed on, which is why the options that write are refused beside it. The
options that decide what a record *is* all apply, so the columns it numbers
are the columns a run would see.

It reads a ragged file rather than refusing one, whatever `--flexible` would
have done — somebody counting columns is often counting because the file is
ragged — and a tab or newline inside a quoted field is shown as `\t` or `\n`
rather than breaking the table into more rows than it has.

`--remove 0,3,4` drops columns; `-a/--arrange 3,1` reorders them. Both take
positions in the input, so the numbers never shift as the row changes shape.

`--arrange` moves the columns you name to the front, in the order you name
them, and keeps the rest behind them in their original order — so the row comes
out the same width it went in, and moving one column first is just `-a4`:

```bash
$ printf 'a,b,c,d,e\n0,1,2,3,4\n' | csvdt -H -p -a3,1
d,b,a,c,e
3,1,0,2,4
```

## Uneven rows

`-f/--flexible` lets records with differing field counts through instead of
stopping at the first one. `-f=fix` goes further and pads short records with
empty fields, so the output is rectangular and valid CSV:

```bash
$ printf 'a,b,c\n0,1\n0\n' | csvdt -H -p -f=fix
a,b,c
0,1,
0,,
```

The mode must be attached with `=`, so `csvdt -f data.csv` still reads the file
rather than taking the path as the mode.

`fix` measures the **first** record, in a single pass. That's cheap, but it
can't know about a row wider than the first — such a row is an error rather than
being silently cut down.

`-f=widest` reads the input twice instead: once to find the widest record, then
again to pad everything out to it, header included. Use it when you can't vouch
for the first record, which is common in the damaged files these modes are for:

```bash
$ printf 'a,b\n0,1,2,3\n0\n' | csvdt -H -p -f=widest
a,b,,
0,1,2,3
0,,,
```

That header was the short one — `fix` would have refused this file. Where
`widest` pads the header, the added names are empty rather than invented.

Input that can't be rewound is copied to a temporary file first — a pipe, and
also a named input that isn't a regular file: a fifo, or the pipe that process
substitution puts behind a filename in `csvdt -f=widest <(...)`. A symlink is
followed, so what matters is what it resolves to. A regular file is read twice
directly, since copying it would double the cost for nothing. The temporary
file is removed when the run ends, including on failure. It is readable by its
owner alone, and created only at a name nothing holds already — the temporary
directory is shared with every other account on the machine, and a symlink left
there would otherwise have csvdt write the input through it. The counting pass
finishes before the first byte of output, so a run that fails while counting
writes nothing at all — but once the second pass starts writing it writes as it
reads, so a run that fails partway has already sent every row it reached. What
you have then is valid rectangular CSV that stops early, told from a finished
run only by the exit status. It costs roughly one extra read — about 50% over a
single pass on a 22 MB file.

Note that both filling modes **add fields that were not in the input**, and they
are indistinguishable from empty fields the source really had — so a filled file
is not a faithful copy of the original.

`--fill-log` is the account of that. It takes a destination — a path, or `-` for
standard error — and writes one CSV row per record a filling mode touched, or
would have touched:

```bash
$ printf 'a,b\n0,1,2,3\n0\n' | csvdt -H -p -f=widest --fill-log - > /dev/null
line,record,fields,width,delta,status
1,0,2,4,-2,padded
3,2,1,4,-3,padded
csvdt: 2 row(s) written to the fill log in standard error, measured against the widest record
```

`delta` is how far the row was from that width — negative where it fell short,
positive where it ran over, the row's own count leading — and `status` says
where it stands: `padded` where the fields were supplied, `short` under plain
`--flexible`, which pads nothing and where the log is the only record a row was
ever short, and `over` for a row wider than the width. Which record the width
came from differs by mode, so the run says on stderr which one it used.

`over` means different things by mode. Under `fix` it is ordinary — the width is
the first record, and any row wider than it ends the run. Under `widest` it
cannot happen to a file that holds still, since that width *is* the widest
record: a row exceeding it means the file changed between the two reads. Under
plain `--flexible` nothing stops.

### Which mode to use

Run plain `--flexible` first, with a log and nothing else. It changes no byte
and exits 0, so it costs a pass and risks nothing — and its log names the
anomaly rather than the consequence:

```bash
$ printf 'a,b,c\n0,1,2\n3,4,5,6\n7,8,9\n' | csvdt -H -p -f --fill-log - > /dev/null
line,record,fields,width,delta,status
3,2,4,3,1,over
csvdt: 1 row(s) written to the fill log in standard error, measured against the first record
```

One row is wide. Reach for `widest` without knowing that and it pads the whole
file to accommodate it — every row gaining an empty column that was never in the
source, and the output rectangular and looking correct:

```bash
$ printf 'a,b,c\n0,1,2\n3,4,5,6\n7,8,9\n' | csvdt -H -p -f=widest --fill-log - > /dev/null
line,record,fields,width,delta,status
1,0,3,4,-1,padded
2,1,3,4,-1,padded
4,3,3,4,-1,padded
csvdt: 3 row(s) written to the fill log in standard error, measured against the widest record
```

The rule for reading a log is uniformity. One delta repeated across every row
means the reference is wrong, not the rows — a header that lost a name, usually.
Nearly every row padded with one row *missing* from the log means that missing
row is the widest, and the file has just grown a column to hold it. Deltas that
vary, over some rows and not others, mean the file itself is ragged.

So: rows short against a sound header wants `fix`; a first record that is itself
unreliable wants `widest`; one stray wide row wants neither until that row has
been dealt with. `--help` covers this and the rest.

## Input encoding

Fields are read as text, so the input must be UTF-8. A byte-order mark is
stripped if present. That covers modern Windows output — PowerShell 7's
`Export-Csv` and Excel's "CSV UTF-8" both write UTF-8.

Anything else is refused rather than guessed at:

- **cp1252 / latin-1**, which Excel's plain "Save as CSV" produces on a Western
  locale, fails naming the record and the line it is in, the field, and how far
  into that field the first bad byte lies:

  ```
  csvdt: CSV parse error: record 1 (line 2, field: 1, byte: 8): invalid utf-8: invalid UTF-8 in field 1 near byte index 3
  ```

  Read that `byte:` with care — it is where the *record* starts, not where the
  bad byte is. The bad byte here is at offset 32; byte 8 is where record 1
  begins, and `near byte index 3` is how far into field 1 it lies. Converting
  the file is the answer either way.
- **UTF-16** is named explicitly, including without a byte-order mark. That case
  is worth the special handling: pure-ASCII UTF-16 has a NUL after every
  character and a NUL is legal UTF-8, so it would otherwise read as text and
  produce NUL-interleaved output instead of failing.

Convert first if needed:

```bash
iconv -f UTF-16 -t UTF-8 in.csv > out.csv
iconv -f CP1252 -t UTF-8 in.csv > out.csv
```

A field whose data genuinely contains NUL bytes is passed through untouched —
the UTF-16 check looks for the alternating pattern, not for any NUL at all.

## Time zones

`-u/--utc` and `-o/--offset` are pure fixed-offset arithmetic and involve no
time zone rules, so the bundled database version cannot affect them. `-l/--local`
does use it, and takes the zone from `TZ`, or the system's configured zone when
`TZ` is unset.

That makes `-l` the only conversion here whose output isn't settled by the input
alone. The record already carries its offset — csvdt refuses a timestamp without
one — so `-u` and `-o` are arithmetic on a fact the file contains, and give the
same result on any machine from any build:

```bash
$ for z in UTC Asia/Tokyo America/Chicago; do TZ=$z csvdt -H -p -u0 log.csv | md5sum; done
d51de2c0bfb2...   # identical
d51de2c0bfb2...
d51de2c0bfb2...

$ for z in UTC Asia/Tokyo America/Chicago; do TZ=$z csvdt -H -p -l0 log.csv | md5sum; done
7a89877d8846...   # three different answers
5f7753e4a1b4...
d139f8228105...
```

One consequence is worth knowing: because `-l` follows the rules, its offsets
vary from row to row, and a column holding both `+01:00` and `+02:00` **no longer
sorts into the order events happened.** Where an autumn shift repeats an hour,
two different instants read the same on the clock:

```bash
$ TZ=Europe/Stockholm csvdt -H -p -l0 -i 0,replace x.csv | tail -n +2 | sort
2024-10-27T02:30:00+01:00,second      # sorted first, happened second
2024-10-27T02:30:00+02:00,first
```

`-u` and `-o` give one offset throughout, so their output sorts chronologically
by construction. Build the timeline with those and use `-i/--insert` to keep an
`-l` column beside it for reading.

`-l` answers a different question — what did the clock read in some zone — which
needs the zone (from the environment) and the rules (from the bundled snapshot),
neither of which the record carried. It also *replaces* the offset the record
had rather than working from it. So prefer `-u` or `-o` where a result has to be
reproducible from the file by itself, and use `-l` for reading. `--help` ends
with a KNOWN LIMITS section setting out exactly what `-l` depends on, including
a known gap in the bundled aliases.

Naming a zone (`TZ=Asia/Tokyo`) behaves identically on every platform. A `TZ`
holding POSIX rules instead of a name (`TZ=EST5EDT,M3.2.0,M11.1.0`) is only
honoured on Unix; on Windows the system zone is used instead.

A `TZ` naming a zone this binary doesn't know is **refused**:

```
$ TZ=Europe/Stockholmm csvdt -H -p -l0 data.csv
csvdt: TZ is set to 'Europe/Stockholmm', which is not a zone in the IANA
database built into this binary (tzdata 2025b). '--local' would otherwise
have reported UTC as though it were that zone. ...
```

A typo used to fall through to UTC, and nothing in the output showed it — a
plausible timestamp with a plausible offset. Note this is stricter than `date`,
which answers in UTC for the same input. The message names the bundled release
because a zone *newer* than the build is a different problem from a
misspelling, with a different fix.

Forms that legitimately aren't zone names still work: POSIX rules, the
bracketed `<+04>-4` form, and glibc's `TZ=:/usr/share/zoneinfo/...` path. Those
two are checked rather than trusted, for the same reason: a rule this build
cannot read — a transition time past 24:00, which zones really do publish, as
Asia/Jerusalem's own `IST-2IDT,M3.4.4/26,M10.5.0` does — and a path naming no
file both used to become UTC without a word.

## Output is canonical CSV, not a copy

Records are parsed and written again rather than passed through, so even a run
that changes nothing about the data normalises how it's written: CRLF becomes
LF, a missing final newline is added, and quotes are written only where they're
needed (`"a","b"` comes back as `a,b`).

A field's own text is not among them — it comes through as the file wrote it,
whitespace and all. `--trim` is there when the whitespace is formatting rather
than data, and it isn't the default because trimming reaches inside quoted
fields, which is exactly where a CSV says its spaces *are* data:

```bash
$ printf '" 1 ",x\n' | csvdt
 1 ,x

$ printf '" 1 ",x\n' | csvdt --trim all
1,x
```

Converting doesn't need it either way: a timestamp is read past any whitespace
around it, so a file written with `, ` separators converts without `--trim`
altering the columns beside it.

```bash
$ printf 'id, ts\n1, 2024-01-01T00:00:00Z\n' | csvdt -H -p -u1
id, ts,<to_utc
1, 2024-01-01T00:00:00Z,2024-01-01T00:00:00+00:00
```

None of this changes what a value means as CSV, but it does mean the output
isn't byte-comparable with the input, so keep the original where the file itself
is the evidence.

The same canonicalising is why `--single-quote` output is read back *without*
`--single-quote`: only reading uses the single quote, and the writer quotes
with `"` regardless, so the output is ordinary double-quoted CSV. Read with
`--single-quote` again, a field the writer quoted keeps its quotes as data and
splits on any delimiter inside it. A run whose output holds such a field —
a single empty field counts, since the writer quotes it to keep the row from
becoming a blank line — says so on stderr, with a count.

## When a value won't parse

A conversion that can't read its input writes the literal `parse_err` in place
of the result, moves on to the next row, and still exits 0:

```bash
$ printf 'ts,msg\n2024-01-01T00:00:00Z,ok\nJan  1 00:00:00,syslog\n' \
    | csvdt -H -p -u0
ts,<to_utc,msg
2024-01-01T00:00:00Z,2024-01-01T00:00:00+00:00,ok
Jan  1 00:00:00,parse_err,syslog
```

One malformed line out of millions shouldn't throw away the run, which is why
this isn't fatal. How many failed is reported at the end, on stderr so it
disturbs nothing a pipe is reading:

```
csvdt: 1 of 2 values could not be converted, and were written as parse_err
```

A few failures among many rows are ordinary, and the run exits 0. **Nothing
converting at all is different** — that usually means `-u` was pointed at the
wrong column, or the timestamps aren't a format csvdt reads — so that exits
non-zero, because otherwise it is indistinguishable from success:

```bash
$ csvdt -H -p -u1 syslog.csv > utc.csv && echo "converted $(wc -l < utc.csv) rows"
csvdt: nothing converted: all 2 values were written as parse_err. Check the
column number, and that the timestamps are in a format csvdt reads ...
```

The output is still written in that case; only the status says it holds no
converted value. Where you want any failure at all to be a failure, count the
markers:

```bash
$ csvdt -H -p -u0 log.csv | grep -c parse_err
```

The marker is a word rather than an empty field on purpose: an empty field is
invisible, and indistinguishable from one the source really left blank. The value
that failed normally stays beside it in its own column, so you can see what
tripped it — the exception is `-i replace`, which puts the marker where the value
was.

## `--comment` and the writer

The writer is told the comment character, so a field holding it is quoted
exactly as a field holding the delimiter would be — quoting is what keeps it
data rather than the start of a comment — and the output reads back with the
same `--comment`, rows and all:

```bash
$ printf 'a,b\n"#x",2\n3,4\n' | csvdt -H -p --comment '#'
a,b
"#x",2                                 # quoted, so a second read keeps the row
3,4
```

This adds quotes the input may not have carried — under `--comment '#'` the
field `a#b` comes back as `"a#b"` — which changes how the value is written, not
what it is, like every other normalisation here.

Two quoting styles cannot give that protection, and there the rows a second
read would drop are counted and reported on stderr, the same way `parse_err`
is, with the run still exiting 0. `--quote never` quotes nothing, by
definition. `--quote nonnumeric` decides by whether the field is a number and
consults nothing else, so `--comment '-'` over the numeric field `-5` writes it
bare:

```
csvdt: 1 row(s) begin with '#', which '--comment' makes the start of a
comment. The quoting style asked for leaves the field bare ...
```

**The header counts there too, and is the worst of them.** A second read drops
the header rather than a row — so `-H` takes the first data row for the column
names, and that row is then exempt from the conversion rather than missing from
it. Every column number still lines up, which is what makes it quiet, so that
case is named separately in the warning.

## Durations

`-d/--duration` reports only days, hours, minutes and seconds — never years,
months or weeks. Those units have no fixed length, so expressing a duration in
them would mean silently rounding. Sub-second precision is kept when the source
timestamps carry it; `--round-seconds` rounds to whole seconds when that
precision is noise rather than evidence. A duration is never negative: the two
instants are taken low-to-high whichever order the columns name them, leap
seconds included.

## Wrapping csvdt

`--list-options` prints what this build accepts in a form a script can read, so
an editor front end or a completion script doesn't have to scrape `--help`:

```
$ csvdt --list-options
# csvdt-option-list 1
# short	long	value	separator	value-name
-H	--has-header	none	none	
...
-f	--flexible	optional	equals	keep|fix|widest
...
	--remove	required	either	num[,num...]
...
```

Five tab-separated fields per option: short form, long form, whether it takes a
value (`none`/`required`/`optional`), whether that value must be attached with
`=` (`equals`/`either`), and the placeholder `--help` shows. Every line carries
all five, so a field is empty rather than absent — a flag has no short form or
no placeholder to report. Lines starting with `#` are comments, the first naming
the format version. The list is generated
from the option definitions themselves, so it cannot fall behind them — a test
compares it against `--help` to prove that.

The `separator` field is the part `--help` can't express machine-readably:
`--flexible` is the only option whose value must be joined with `=`, and a
wrapper that emitted `-f fix` would hand csvdt a filename instead.

[`emacs/`](emacs/) is a worked example: an Emacs front end that names three
csvdt options — `-H`/`--has-header`, `--single-quote` and `--read-delimiter`,
and only because a region run cannot be worked out without them. Which flag
says the first record is a header decides whether the region is given the
buffer's header line; the other two decide where a record begins, since a
quote only opens a field where a field begins, and that depends on the
delimiter. Everything else is discovered from the binary, so the front end
needs no change when csvdt gains an option. A test holds each of the three to
an option csvdt still reports, and another asserts no fourth name has crept
in.

## Piping

`csvdt file | head` closes the pipe as soon as it has what it wants, and that
is not an error: csvdt stops and exits 0, saying nothing. Any other write
failure is still reported and still exits non-zero.

## Known limits

`csvdt --help` ends with a **KNOWN LIMITS** section collecting these in one
place, so they don't have to be discovered option by option. In short: input
must be UTF-8; columns are addressed by position rather than by header name; a
conversion whose result falls outside RFC3339 — a year beyond `0001`–`9999`, or
an offset that is not a whole number of minutes — writes `parse_err` rather than
a value nothing can read back; `-l/--local` uses the time zone data compiled
into the binary, where the deprecated single-word names (`EET`, `WET`, `CET`,
`MET`, `EST5EDT`) are links carrying their geographic zone's full history — an
older release defined them as rules with no history, so name the geographic
zone where a result has to be reproduced; `-o/--offset` accepts only offsets a real place uses;
`-d/--duration` with one column trusts the row order; `--comment`'s character
is a reason to quote on the way out, so the output reads back — except under
`--quote never`, and `nonnumeric` over numeric fields, where dropped rows are
counted instead (see above); `--flexible`'s two filling modes are the only
options that add data; rows are processed one at a time, so nothing needing
the whole file first is possible — memory follows the widest single record, not
the file, which matters exactly when an unclosed quote makes the rest of the
file one record; and a CR-only file (classic Mac line endings) loses every row
after its first comment line to `--comment`, since only `\n` ends a comment —
convert the line endings first.

## Building from source

```bash
cargo build --release
cargo test
```

`cargo test` and the Emacs suites in [`emacs/`](emacs/) run in CI on every pull
request, and on `master`. The Rust tests run on Linux, macOS and Windows, since
`-l/--local` reads `TZ` on Unix and not on Windows and the two genuinely differ.

[`checks/`](checks/) holds a separate set, run by hand rather than in CI:

```bash
./checks/run-checks.sh
```

Those are not tests of decided behaviour but comparisons against a second
implementation — Python's `csv` and `datetime` — and against what this
documentation says, over generated input. A test written from csvdt's own
expectations cannot find a place where those expectations are wrong; these
can, and have. Worth a run before a release, or after changing anything about
parsing, conversion, or which records a selection covers.

## Cutting a release

The version is written down in three places that are checked — `Cargo.toml`,
the `Version:` header in [`emacs/csvdt.el`](emacs/csvdt.el), and `Version:` in
the [RPM spec](packaging/rpm/csvdt.spec) — and in a handful of README lines
that are not. Bumping only the first is caught by CI eventually, on whichever
leg runs the Emacs suite or builds the RPM; the README lines are caught by
nobody and go on naming the previous release.

```bash
packaging/set-version.sh 1.1.0
```

It sets all of them, resets the spec's `Release:` to 1, moves CHANGELOG.md's
`## Unreleased` heading to the new version, and then prints every remaining
mention of the old one so you can see that what is left is history. It leaves
the spec's `%changelog` and the released changelog sections alone, since a
version written there was true when it was written.

What it cannot do is write the spec's `%changelog` entry, which is a sentence
about what changed. It says so at the end.

**Bump, commit, then tag that commit.** A tag placed before the commit that
set the version produces a binary reporting the previous one, and nothing
downstream can tell — the tag name and the binary's `--version` simply
disagree, on a release page that says otherwise.

Pushing the tag is what publishes everything: both workflows build that tag,
attach the binaries and the packages to the version's own release, refresh
`release` and `rpm-release` so the download links point at the new version,
and cut the first permanent archive entry for it.

The tag is also the only thing either workflow will build. Both resolve the
highest version tag and compile that, so a tag for anything but the highest
version stops with an error rather than quietly handing the download links to
a version that is not current.

The version's release then holds GitHub's "Latest" designation, which is what
`releases/latest/download/…` follows and what a visitor sees first. Its
binaries are frozen at that moment; `release` is where a build of the same
version with current zone rules lives, and the version's own `BUILD-INFO`
names the database it was published with.

## License

GNU General Public License v3.0 or later. See [LICENSE](LICENSE).
