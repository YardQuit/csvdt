# checks

Differential checks, run against a real csvdt.

These are **not** the test suite. `cargo test` and `emacs/run-tests.sh` pin
behaviour that has been decided, case by case, and they are what a commit has
to pass. These ask a different question — *is the decided behaviour right?* —
by putting csvdt beside a second implementation, or beside its own
documentation, over generated input. They are meant for before a release, or
after a change to parsing, conversion or the selection logic.

```sh
./run-checks.sh                  # everything
./run-checks.sh columns          # one, by name
SEED=7 ./run-checks.sh           # a different draw of generated input
CSVDT=/path/to/csvdt ./run-checks.sh
```

Every check is seeded, so a failure repeats: run it again with the same
`SEED` and the same case comes back. Exits non-zero if anything is found.
Needs `python3`; the Emacs checks are skipped with a note if no `emacs` is on
the path. The whole set takes a minute and a half, most of it in the three that build large fixtures on disk — `memory.py`, `timing.py` and `canonical.py`.

csvdt is rebuilt on the way in, unless `CSVDT` names a binary. It used to be
built only when missing, which meant an edited source beside a stale binary
was checked as the previous build and reported `ALL CLEAR` — the one answer
here that must never be wrong.

## What each one asks

**Against another implementation.** Python's `csv` and `datetime` were
written by other people from the same specifications. A test written against
csvdt's own expectations cannot find a place where those expectations are
wrong; this can.

| check | question |
| --- | --- |
| `csv_roundtrip.py` | Whatever records Python's `csv` finds in a file, does csvdt write them back? Quotes, doubled quotes, delimiters and newlines inside fields, every line ending, unclosed quotes, a byte-order mark. |
| `quoting.py` | Does each `--quote` style keep the promise it makes about a second read? `always` and `necessary` must give back the same table over every output delimiter and comment character; `nonnumeric`, which quotes by the numeric test alone, must either give it back or say on stderr which rows it could not. `never` is exempt: its description is that it writes invalid CSV where the data needs quoting. |
| `timestamps.py` | Does every conversion name the same instant Python reads? Compared as instants, not text, so formatting is free to differ and only the meaning is held. `-u`, `-o`, `-s`, `-r`, years 1 to 9999, every leniency the help documents — a lower-case `t` or `z`, a space where the `T` belongs, an offset missing its colon, offsets out to ±23:59 — and every fraction width. A conversion whose result leaves the year range is `parse_err` rather than a divergence: Python's own `datetime` stops at the same two ends, so it refuses exactly the values csvdt does. |
| `durations.py` | Is `-d` exact? Python's `timedelta` stops at microseconds and cannot referee nanoseconds, so the span is computed in whole nanoseconds as integers. |
| `local.py` | Does `-l/--local` read the clock a zone actually kept? The one conversion with no model behind it — the others are held against Python here, `-l` appeared only in the exit-status and option-matrix checks. Three properties, held apart because they do not rest on the same things. At any date: the instant does not move, and no offset carries a seconds part, which RFC3339 cannot write and the help says is refused instead. Inside a window from 1970: the clock reading matches Python's. The window is there because the two databases genuinely differ before standard time — chrono-tz 2025b has Europe/Amsterdam on +00:00 from 1892 where this host's tzdata has +00:19:32 through 1937 — and twenty minutes of that is not a fault in csvdt. The older dates are still exercised by the two properties no database can dispute. Which copy of the data answered is printed rather than held: it can confirm the binary's own and never quite the opposite. |

**Against its own documentation.** The help states these precisely enough to
model independently, which is what makes the comparison worth anything.

| check | question |
| --- | --- |
| `columns.py` | Do `--remove`, `-a` and `-i` do what the help says, over every subset and ordering of a small file's columns — including where `-i` puts a value when it is not told? |
| `list_options.py` | Does `--list-options` describe how the binary actually parses? This is the contract the Emacs front end reads to tell a value from a filename, so a wrong `separator` makes it mistake one for the other. |
| `canonical.py` | Is the output really canonical — a fixed point, unchanged by a second run? And do fields straddling the writer's buffer boundaries survive, including a megabyte of quote bytes where every one doubles? |
| `readme_examples.py` | Does the README — and the help's own EXAMPLES section — print what it says csvdt prints? Every `$ ` example that needs nothing but a pipe is run and compared; every message quoted without a command is reproduced from a fixture kept beside the claim. Both READMEs, both directions — the line must still be in the file, and the binary must still produce it. The help's EXAMPLES are run against fixtures kept here, since a manual page's examples are the part a reader copies and stale copied output is worse than none. |
| `peek.py` | Does `--peek` number the columns a run actually sees? A run with no action and `-f` writes the records back as csvdt read them, which is the field list to beat; `--peek` shows the same records padded into a table. The numbering must cover the widest row shown, the rows shown must be the rows a run works on under `-H` and `-p`, and where the generated fields hold nothing that can be confused with padding the text is compared too. Ragged input, every reading dialect, wide and combining characters, tabs and newlines inside fields. Then the other half, over every option `--list-options` declares rather than a list written out here: the six that decide what a record is are welcome and must exit 0, and every other one is refused with exit 2, never a panic. Driven from the binary's own account of itself, so an option added later is refused by default and has to be argued into the welcome set — the hand-written list this replaced covered thirteen invocations of twelve options and never tried `--round-seconds`, the thirteenth in `--peek`'s conflict list, so a build that lost that conflict still read ALL CLEAR. |
| `exit_status.py` | Is it the right one of the four? `option_matrix.py` asks only that the status is documented, which a build returning 0 for everything would pass. Here the run's own output decides: the action writes a result column, a value it could not read is `parse_err`, so some converted must be 0, none converted must be 1, and no values at all must be 0 — nothing failed. Weighted to the boundary, where one row either way changes the answer. Then the 1/100 line, which is about when the trouble was found rather than how bad it is: refused before the input is opened is 100, found by doing the work is 1. And the writing half of that — a run onto `/dev/full`, and each of `--help`, `-h`, `--version`, `--list-options` and `--generate-man` onto the same, all of which must report 1 and say why. Skipped where there is no `/dev/full` rather than passing on a machine that could not have run it. |
| `tallies.py` | Do the numbers on stderr match what was written? Three counts reach a caller and each is a claim about the future: how many rows begin with the comment character and would be dropped by a second read, how many hold a field `--single-quote` would misread, and how many values were written as `parse_err` out of how many. Each is asked twice — of a model here that reads the output the way a second read would, and of csvdt itself by running the output back through it. The comment count is checked byte-wise so it holds under `--quote never`, whose output is not CSV that reads back and where the other two are not asked. |
| `memory.py` | Does a run cost what KNOWN LIMITS says? Two claims of different kinds, held differently. That memory grows with the widest record and not with the file is a property of the design — a million rows must not cost meaningfully more than a thousand, and that is the same answer on any machine. That one enormous field costs about twice its own size is a measured figure, so it is allowed a wide band; a check that fails on somebody's laptop for being right is worse than none. Fixtures go to disk, never into this process: measuring a child by fork while the parent holds the fixture reports the parent's inherited pages, which read 99 MiB for a run the kernel put at 5. |
| `timing.py` | Does the time grow the way KNOWN LIMITS says? The shape, not the seconds: how long a machine takes is the machine's business, but doubling the input must cost about twice as long, and that is the same answer everywhere. Each doubling of one enormous field — plain, and made of nothing but quote characters, which is the writer's worst case — must land between 1.3x and 2x-and-a-half, wide enough for a loaded machine and still failing loudly on the 4x a quadratic writer would show. A file of ordinary rows is timed and reported rather than held to a band, since that is the case where memory stays flat and time is all that grows. Output that takes an hour to produce is still correct output, so no other check here would notice. |
| `option_matrix.py` | Options in combination: nothing panics, a run that succeeds writes CSV that parses, a filling mode fills to a rectangle, and the exit status is one of the four documented. A sample rather than a sweep — 1500 runs per seed, drawn from some 78,000 argument sets over seven inputs — so a different `SEED` is a different draw, not a repeat. |

**The front end against the binary it wraps.** csvdt.el assembles, encodes and
decodes text on the way through, and decides which records a selection covers.

| check | question |
| --- | --- |
| `emacs_whole_buffer.el` | Does a run from Emacs give what the shell gives, byte for byte, for the same input and arguments? |
| `emacs_selection.el` | Does a selection send exactly the records it touches, each once? The buffer is built from records whose byte spans are known here by construction, so the reference does not share the code under test. |
| `emacs_compat.el` | Does the package keep to the Emacs its header asks for? Every function it uses, against the version Emacs's own NEWS files date it to. |

## Whether they work

A check that cannot fail is worse than no check, so these have been run
against a csvdt broken on purpose. Dropping the last record of every run is
caught by four of them. Reintroducing the two selection bugs this directory
grew out of — a selection not reaching out to whole records, and two banked
stretches sending the same record twice — is caught by `emacs_selection.el`,
620 and 110 divergences respectively, out of 1170 runs.

`readme_examples.py` was written after four transcripts were found to have
drifted — two showing a message without the `csvdt: ` prefix added to every
message after the README was written, one showing a `--version` string no
build has ever printed, and one carrying an `Error: ` prefix the message does
not have. All four are caught by editing them back, and the reverse too: a
message changed in `src/main.rs` and left alone in the README fails it.

Each has also been given a defect aimed at what it claims, one at a time:
`--remove` dropping only its first column, `--list-options` calling every
separator `either`, the writer quoting nothing, RFC 3339 output an hour late,
durations losing their fraction, an argument refusal exiting 3, the package
calling `string-search`, and each of the two paths csvdt.el sends text
through losing a character. Every one was caught, and by the check that
claims that ground: only `columns.py` saw the first, only `durations.py` the
fifth. The writer defect was seen by three, which is what a defect in the
writer should do.

`emacs_compat.el` is the loosest of them and says so in its own comments.
The version comes from the earliest NEWS mention of a name, which is wrong
in both directions: `always` arrived in 28.1 and is dated 25.1, because the
word appears in older prose, and `count-lines` is dated 28.1 because that
release added an argument to a function older than any of this. The second
kind is listed in the file with its reason and with the number of arguments
that reason covers: the excuse for `count-lines` holds while the package
passes two and lapses the moment it passes the third, which is the argument
the NEWS entry is about. Written as a bare name it was a permanent silence
about a symbol whose whole point was that the silence is conditional. A real
answer would be
byte-compiling under the oldest supported Emacs; this is the cheap version,
and it catches what got through before — `string-search`, and a 29-only
`pos-bol`, both flagged when put back in on purpose.

Worth saying what they do not cover, since a green run is easy to over-read.
They check behaviour against generated input and stated intent. They say
nothing about performance, nothing about signals or temporary files, and
nothing about a claim no reference implementation shares — a leap second
written back as `:60` is something Python simply refuses to read, so no
comparison here can judge it. Those live in `tests/cli.rs`, where they are
pinned case by case.
