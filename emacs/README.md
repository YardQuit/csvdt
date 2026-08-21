# csvdt.el

An Emacs front end for `csvdt`, meant to sit alongside `csv-mode`.

## Nothing here needs updating when csvdt changes

Piping `csvdt --help` into a help buffer is trivial and always current. But the
binary can also tell Emacs what its options *are*, so the package doesn't need
to know them:

- `csvdt-help` shows csvdt's own `--help`, read from the binary each time.
- The options offered when reading arguments come from `csvdt --list-options`,
  which csvdt prints from its own argument definitions. The result is cached
  against the binary's path and modification time, so a rebuild invalidates it
  by itself — without spawning csvdt again to ask.

Result: **no csvdt option name appears anywhere in `csvdt.el`** beyond the three
it calls to ask (`--list-options`, `--help`, `--version`) and the few a
defcustom holds: `csvdt-header-options`, which the package must recognise to
know when a region needs the buffer's header in front of it, and
`csvdt-single-quote-options` and `csvdt-read-delimiter-options`, which say
where one record ends and the next begins. A test asserts each of them is
still an option csvdt reports. Add an option to csvdt and it is
completable in Emacs immediately, with no change here — checked literally, by
diffing the discovered set against the binary.

`--list-options` also says how each value must be attached, which the help
layout doesn't make machine-readable. `--flexible` is the one option whose value
must be joined with `=`, so `-f=` and `--flexible=` are offered as well as the
bare forms — `--flexible`'s value is optional, so the bare form is legitimate on
its own, and only a value written separately would be read as a filename.

## Setup

On Fedora, RHEL or CentOS Stream it is a package. It has to be named in the
same command as the csvdt it belongs to: the subpackage requires that exact
version-release, and `dnf` has nowhere to resolve that from when the packages
are given as URLs rather than out of a repository.

```bash
sudo dnf install \
  https://github.com/YardQuit/csvdt/releases/download/rpm-release/csvdt.x86_64.rpm \
  https://github.com/YardQuit/csvdt/releases/download/rpm-release/emacs-csvdt.noarch.rpm
```

That installs `csvdt.el` where Emacs already looks and an autoload file in
`site-start.d`, so every command below is there with no `require` and no
`load-path` line. `rpm-release` is the newest released version, rebuilt each
month; a version's own release page carries the same two packages as that
version was published with, and either page works as long as both URLs name
the same one — the dependency is on an exact version-release.

Otherwise, from a checkout:

```elisp
(add-to-list 'load-path "/path/to/csvdt/emacs")
(require 'csvdt)

;; optional: bind the command map somewhere
(with-eval-after-load 'csv-mode
  (define-key csv-mode-map (kbd "C-c C-v") csvdt-command-map))
```

If `csvdt` isn't on `exec-path`, set `csvdt-executable` to its path. The
shell's rule applies: a name carrying a directory part — absolute, or relative
like `"./target/release/csvdt"` — means that file, resolved against
`default-directory` and never searched for on `exec-path`; only a bare name is.

Or install it as a package, which is what the `;;;###autoload` cookies in it
are for — the commands are then there without `require`:

```elisp
M-x package-install-file RET /path/to/csvdt/emacs/csvdt.el RET
```

`csvdt.el` is not versioned separately — it lives in the csvdt repository
because the two share a contract, so the copy next to a given csvdt is the one
that matches it. Its `Version:` header says exactly that: it carries csvdt's
own version, and a test fails if the two drift apart, the same bargain the RPM
spec's `Version` is held to. It works against older binaries anyway; see
below.

## Commands

| command | what it does |
| --- | --- |
| `csvdt-run-dwim` | run on the banked lines *and* the region together, else the whole buffer |
| `csvdt-run-buffer` | run on the whole buffer |
| `csvdt-run-region` | run on the region — handy for trying something on a few rows first |
| `csvdt-run-banked` | run on banked lines, from whatever banks them |
| `csvdt-help` | csvdt's own `--help` in a help buffer |
| `csvdt-describe-run` | what a run would cover, and why — without running it |
| `csvdt-version` | which csvdt, and which time zone database is built into it |

With the command map bound as above: `r` dwim, `b` buffer, `SPC` region,
`k` banked, `?` describe, `h` help, `v` version.

## Arguments

Type a command line the way you would in a shell. It comes prefilled from
`csvdt-default-arguments` (default `-H -p`), **left selected**:

| key | what it does |
| --- | --- |
| type anything | replaces the prefill |
| `→` or `C-f` | keeps it, and puts point after it so you can add more |
| `RET` | submits it as it stands |
| `TAB` | completes the option at point |
| `M-p` | walks back through lines you have run before |

The prompt shows the csvdt version, so you always know which binary you are
driving.

Selected rather than merely prefilled because the two possible header mistakes
are not symmetric. Leaving `-H` off a file that *has* a header is loud — the
header comes back as `parse_err`. Passing it to a file that has *none* is
quiet: the first row is taken as a header and, depending on the insert
position, either goes through unconverted or is replaced by the generated
header name and lost outright. A prefill you had to notice and delete pointed
at the quiet mistake; one you have to keep does not.

It is a shell-style line and *not* a comma-separated list, because a comma is
csvdt's own separator inside a value — `-a 1,0`, `--remove 0,3,4`,
`-i 0,replace`, `-o0,+02:00`. Splitting the line on commas tore those in half.
Quoting works too, for a value that contains a space:

```
csvdt arguments: --print-delimiter " "
```

Either quote character, since the examples you paste come from a shell and a
shell takes either — `--comment '#'` arrives as one character, not three. A
double-quoted run honours the shell-familiar backslash escapes — `\t` `\n`
`\r` `\\` `\"` and no others, so `"\t"` is a tab but `"C:\data"` is refused
rather than read as Lisp's DEL escape — while a single-quoted one is literal
throughout. So a lone quote is written by wrapping it in the other kind, and a
value that must keep its backslashes belongs in single quotes; any other
backslash sequence inside double quotes is refused with exactly that advice:

```
csvdt arguments: --print-delimiter "'"
```

Outside quotes a backslash is left as itself, which is *not* the shell's rule.
A Windows path typed as a value has to survive as one argument, and there is no
filename here for shell escaping to be worth more than that — these commands
send the lines on standard input.

A filename in that line is refused, rather than run:

```
csvdt arguments: -H -p sales.csv
csvdt would read sales.csv and ignore the selection; these commands send it
the lines on standard input
```

These commands always hand csvdt the selected lines on standard input, so a
filename is not a second input — csvdt reads the file and the selection never
reaches it. Nothing in the result would say so: the output buffer shows another
file's contents, and the echo area reports a successful run.

Only a name that really is a file is refused, which leaves the more common slip
to csvdt itself. `-f fix` puts `fix` where the filename goes, because that
mode has to be attached with `=`, and csvdt's own message for it names the
option:

```
csvdt exited 1: no file named 'fix'. If you meant the --flexible mode,
attach it: -f=fix
```

Which arguments are values rather than filenames comes from
`--list-options`, so `-u 0`, `--trim none` and `-o 0,+01:00` are understood as
option-and-value without this file naming a single csvdt option.

## What gets processed

`csvdt-run-dwim` takes the **banked lines and the region together** — both, not
one or the other. The region is usually the piece you have not banked yet, so
dropping either would run over less than you picked out. They are merged, so a
selection that abuts or overlaps a banked stretch does not send a line twice.
With neither, the whole buffer.

Merged twice, in fact — once when the list is assembled, and again after the
widening below, because widening brings stretches together that were apart.
Two banked lines inside one record that spans several lines do not touch, so
nothing joins them at the first pass; each then widens to that same record,
and it would go out once per stretch. A duplicate row, valid CSV, identical
to its neighbour and nothing in a count to question it.

A run says what it covered when it was more than one stretch:

```
csvdt -H -p -d0: 12 lines from 4 stretches
```

Anything csvdt says on stderr about a run that still succeeded — how much did
not convert, rows a re-read would take for comments — appears with that
summary, so an Emacs user sees everything a shell user would:

```
csvdt -H -p -u0: 40123 lines
csvdt: 3 of 40122 values could not be converted, and were written as parse_err
```

Whatever the source, whole records are what go out. Half a line is half a
record — a selection starting or ending mid-row would hand csvdt a truncated
field, which it reads as a value rather than as damage. A partially selected
line therefore expands outwards to the whole line; it is never truncated and
never dropped:

```
selected : "12:00:00+02:00,a
            BBBB-"

sent     : "AAAA-07-25T12:00:00+02:00,a
            BBBB-07-25T13:00:00+02:00,b
"
```

The run reports it — `csvdt -u0: 2 lines, selection widened to whole lines` —
so a selection that was not what it looked like says so.

A record is not always a line, though. A quoted field may hold newlines — an
address, a notes column, an embedded document — and then one record covers
several lines, which is ordinary CSV and which csvdt reads and writes
faithfully. Widening to whole lines is not enough there: a selection that
begins inside such a field looks like a row and is the back half of one, and
sending it alone produced a clean conversion of a field nobody wrote:

```
buffer   : h,when
           "x
           y",2026-07-25T12:00:00+02:00

selected : y",2026-07-25T12:00:00+02:00

was sent : y",2026-07-25T12:00:00+02:00     -> first column "y"", exit 0
now sent : "x
           y",2026-07-25T12:00:00+02:00     -> first column "x\ny"
```

So the selection is pushed out to the record boundaries as well, and the run
says `, widened to whole records: a quoted field spans lines`. The header is
carried whole for the same reason: taking only its first line handed csvdt a
fragment that opened a quote and never closed it. A file whose fields hold no
newlines is unaffected — every range in it already sits on record boundaries.

Which character quotes a field, and which separates them, decide where a
record begins, so `--single-quote` and `--read-delimiter` are read from the
arguments before the boundaries are worked out. They and the header options
are the only csvdt options this package names, each in a defcustom, and a test
asserts every one of them is an option csvdt still reports.

Finding a record boundary means reading from the start of the buffer, since
nothing nearer can say which side of a quote a position is on. The scan
crosses the text before the selection once and steps over each quote character
in it, so the cost follows how many quotes there are rather than how large the
file is: measured at 0.17 s for a 14 MB file with twelve thousand quotes, and
1.9 s for an 18 MB one where every field is quoted — nearly two million. A
whole-buffer run pays nothing, having nothing before it to read.

Narrowing is the one case widening cannot rescue: a buffer narrowed to begin
or end mid-line has no whole line to reach, and sending the lines beyond the
restriction would cover rows nobody chose. The run passes on the short record
and says `, first record cut short by the narrowing` or `, last record …`,
rather than leaving a truncated row to be discovered by counting columns.
Both ends are asked about — only the end was, at first, so a narrowing that
began mid-record was the same cut going unreported.

A record can be cut where the line is not, for the same reason as above — a
narrowing ending at a line boundary that falls inside a quoted field leaves
whole lines reachable and the record they belong to severed. That one could
pass quietly: asked to accept ragged records, which is the obvious thing to
reach for when a count looks off, csvdt took the truncated record rather than
refusing it, so the field came back without the half that was out of reach, at
exit 0. Both kinds are reported, at both ends.

A run that fails names the cut too. That is the case the check exists for:
csvdt sees a short record, refuses it, and says so about a line — which
sends you looking at data that is not the matter, since the half of it out
of reach is. The message ends `-- note the first record cut short by the
narrowing`.

## Banked lines

### Why this needs saying at all

Emacs has no notion of *banked lines*. It has a **region**, and a region may be
non-contiguous — `region-bounds` reports one `(BEG . END)` per piece, and a
package defines its own selection by hooking `region-extract-function`. That is
the standard interface, and this package reads it, so anything expressed as a
region needs **no configuration whatever**: a rectangle arrives as one piece per
line, and a package that makes its selection non-contiguous arrives as its
pieces.

A bank is a different thing, and the difference is not stylistic. The region
answers *what is selected right now*, so releasing the mark takes its pieces
with it. A bank answers *what have I set aside so far*, which has to survive
exactly that — or it could not accumulate:

| | mark active | mark released |
| --- | --- | --- |
| non-contiguous region | 3 pieces | **gone** |
| donkey's bank | — | **persists, and accumulates** |

donkey's `donkey-bank-selection` releases the mark on purpose, so you can keep
navigating and select again. That is the point of banking, and it is also
precisely what puts the bank beyond anything `region-bounds` can report. There
is nowhere standard to look, because Emacs has no such concept to standardise.
Hence one variable.

Banking is also general editing rather than anything to do with CSV. Adding to
a bank, taking lines back out, splitting a run of them: none of that changes
because the buffer holds CSV, and every system goes about it its own way. So
this package **reads** a bank and never keeps one.

**Nothing is banked until you say where they come from**, so until you do, a
run covers the region or the whole buffer as it always did. What this package
does with a bank once it has one is the CSV-shaped part: whole lines, buffer
order, touching stretches merged, and the lot sent as a single file so a
duration is measured across what was banked rather than across what was
skipped.

**If the package has a function for it, use that.**

```elisp
(setq csvdt-banked-lines-function #'some-package-banked-lines)
```

It is called in the buffer being processed and returns either `(BEG . END)`
conses or overlays. Ranges are put in buffer order, widened to whole records and
merged where they touch, so an implementation need not do any of that. Nor need
it be tidy: a pair may be either way round, a dead overlay is dropped, `nil`
entries are ignored, a pair naming nothing is discarded rather than counted as
a stretch, and an overlay or marker belonging to an unrelated buffer is refused
rather than having its positions clamped into this one — an indirect buffer and
its base are not unrelated, since they hold the same characters at the same
positions, so banking made in either is honoured in both. Anything that is
neither a pair nor an overlay is reported, naming the variable, instead of
signalling from somewhere inside; so is a function that returns something which
is not a list of them at all. If the package you use is your
own, adding such a function to it is the sturdiest thing you can do here: it is
a name that package chose to expose, and it can keep working even if how the
lines are marked changes.

### A note for anyone writing a banking package

A bank *can* be built on the native interface, and it is worth knowing why you
might not want to. Keep the mark active and override `region-extract-function`
to report the accumulated pieces: every region-aware command in Emacs then sees
the bank, with no per-package wiring for this tool or any other. The set does
not follow point around — reported pieces stay exactly as reported while you
navigate:

```
mark active, point at line 2 : ((6 . 12) (18 . 24))
after moving to end of buffer: ((6 . 12) (18 . 24))   ; survives navigation
after deactivate-mark        : nil                    ; dies here
```

Two costs come with it:

- **The mark stays active for as long as the bank exists**, so everything else
  treats you as having a selection. `C-w` kills the bank, `M-w` copies it,
  typing under `delete-selection-mode` replaces it. That may be exactly right —
  donkey's own `y`/`d` do act on the bank — or it may be a loaded gun.
- **Any command that calls `deactivate-mark` drops the bank**, silently, and a
  great many commands do. A carefully assembled bank can be lost to an
  unrelated keystroke.

Storing the bank yourself is immune to both, which is what donkey does. The
variable above is the price of that robustness: a bank that survives anything,
in exchange for one line saying where to look.

### A worked example: donkey

**If you use [donkey](https://github.com/YardQuit/donkey), this is the setting
you need.** It banks lines with `donkey-bank-selection` and reports them from
`donkey-banked-spans` as `(START . END)` conses in buffer order — the shape this
package takes, so one line connects them:

```elisp
(with-eval-after-load 'donkey
  (setq csvdt-banked-lines-function #'donkey-banked-spans))
```

Without it, `csvdt-run-banked` says there are no banked lines however many you
have banked, and `csvdt-run-dwim` quietly falls back to the region — see
[Why this needs saying at all](#why-this-needs-saying-at-all).

Verified: banking two rows with `donkey-bank-selection` and then running
`-H -p -u0 -i 0,replace` over the bank converts those two rows and no others,
with the header carried in front.

`donkey-banked-spans` is a public name, added for this, and promises whole
lines in buffer order with nothing outside the accessible portion of the
buffer. On a donkey older than that, the same spans come from
`donkey--banked-spans`; the leading `--` marks it as donkey's own and free to
change shape, which is the fragile road described above.

Either way, not `donkey--effective-line-spans`: that merges the region in as
well, and this package does that itself, so the narrower one is the right half.

The overlay route works too, and donkey's banked overlays carry
`donkey-banked`:

```elisp
(setq csvdt-banked-lines-function
      (csvdt-banked-lines-from-overlays 'donkey-banked))
```

Mind the difference between the property and the face. `describe-text-properties`
shows a banked line as

```
priority -50 donkey-banked t face donkey-banked-selection
```

and the eye lands on `donkey-banked-selection`, which is the **face** — the
*value* of the `face` property, not a property of its own. Naming it finds
nothing. Naming `face` finds the banked lines *and* everything else highlighted
in that buffer.

**Failing that, name the overlay property it sets.** Most packages mark banked
lines with an overlay:

```elisp
(setq csvdt-banked-lines-function
      (csvdt-banked-lines-from-overlays 'some-package-banked))
```

To find the property, put point on a banked line and run
`M-x describe-text-properties`: the overlays covering it are listed with what
they carry. You want a property **name**, not the face it is drawn with — those
look alike in that listing and only one of them works. `face` itself matches
every highlighted overlay in the buffer, which is rarely what you mean.

This route reaches into another package's internals, so it can break when that
package changes them. It is a line in your own configuration rather than
anything csvdt.el carries — the package names no banking implementation and
requires none — but it is worth knowing which of the two you are standing on.

If a run covers more or less than you expected, **`M-x csvdt-describe-run`**
says what it would cover and why, without running anything:

```
csvdt would run over the whole buffer, 5 lines.
  Banked: nothing, `csvdt-banked-lines-function' is unset.  Region: none.

csvdt would run over the whole buffer, 5 lines.
  Banked: nothing reported -- lines that look banked may not be.  Region: none.

csvdt would run over 3 lines in 2 stretches.
  Banked: 2 lines in 2 stretches.  Region: 1 line in 1 stretch.

csvdt would run over the whole buffer, 3 lines.
  Banked: nothing, `csvdt-banked-lines-function' is unset.  Region: none.
  First record cut short by the narrowing.
```

The first two read differently on purpose: not knowing where banked lines come
from is a different problem from lines that only look banked — highlighted by
something else, or not actually banked by the package that highlights them.

The last is the one a run would surprise you with. A narrowing that begins or
ends mid-record leaves half a record reachable and nothing whole to widen back
to, so the run reports it — and this reports it first, in the same words,
which is the point of asking before rather than after.

It counts the way a run counts, which means whole records: two banked lines
inside one record that spans several are described as the one stretch the run
will make of them, not as the two lines you banked. That matters because this
is the command you reach for *when a run covered something unexpected* — being
wrong exactly where the run is surprising is the one thing it cannot afford.
Which character quotes a field decides where a record ends, so the dialect is
read from `csvdt-default-arguments`; a run whose own arguments name a different
one may cover a little more than described.

The banked lines are sent as **one file**, in buffer order — so a duration
measured across them is measured across what was banked rather than across what
was skipped. `csvdt-run-banked` runs the banked lines only; `csvdt-run-dwim`
takes them together with the region.

Until this is set, nothing is banked as far as csvdt.el is concerned, and a run
covers the region or the buffer as it always did.

## Modal editors, and other ways of selecting

Nothing here talks to an editing package directly. A run asks two questions —
is a region active, and where does it start and end — so any package that
keeps Emacs's region works without configuration. **evil** and **meow** both do
by design; I could not install either in the environment this was written in, so
take that as their intent rather than as something I tested.

The check takes one keystroke, whatever you use: **`?`** (`csvdt-describe-run`)
reports what a run would cover without running it. If it names your lines, you
are set.

What the shapes do, which *is* tested:

| selection | what csvdt receives |
| --- | --- |
| charwise, mid-row to mid-row | both rows, whole |
| linewise | those rows, unchanged |
| rectangle / visual block | every row it spans, **whole** — columns are ignored |
| non-contiguous | each piece, and **not** the gaps between them |

The last row is why the region is read through `region-bounds` rather than its
two ends: taking the ends would sweep up everything lying between the pieces.

A rectangle selecting columns 2–6 of three rows sends those three rows entire.
That is deliberate: a rectangle spans columns and a record does not, and
picking columns is csvdt's own
job — `--remove` and `-a/--arrange` do it by position, which survives the rows
changing shape.

Banking is different from selecting: it accumulates lines here and there, and
plain Emacs has no such notion, so it only applies if something you run provides
it. Set `csvdt-banked-lines-function` as above and the banked lines join the
region rather than replacing it.

## Regions keep the header

A region selected below the header has no header of its own, so csvdt would
take its first row for one — and quietly, because a header is exempt from
whatever conversion is being applied. Depending on the insert position, that row
either goes through unconverted or is replaced by the generated header name and
lost.

So when the arguments ask for a header and the region starts below it, the
buffer's real header line is put in front of the region. The whole selection is
then data, and the output is named after the columns it actually came from:

```
ts,x                            <- not selected
2026-07-25T12:00:00+02:00,a     <- region
2026-07-26T12:00:00+02:00,b     <- region

to_utc,x                        <- with this on
2026-07-25T10:00:00+00:00,a
2026-07-26T10:00:00+00:00,b

to_utc,a                        <- without it: the first row is gone
2026-07-26T10:00:00+00:00,b
```

Set `csvdt-region-carries-header` to nil for the old behaviour. A whole-buffer
run already starts at the header, so nothing is prepended; nor is anything when
the arguments do not ask for a header, since that would add a row the selection
does not have.

This is the one place the package names a csvdt option, in
`csvdt-header-options` — it cannot know which option means "header" otherwise,
and `--list-options` does not say. Both forms are listed, and a clustered
`-Hp` counts too, worked out from what csvdt reports about which short options
take a value. A test asserts every configured form is an option csvdt actually
reports, so a rename fails the build rather than silently disabling the feature.

## Output

Output goes to `*csvdt output*` in `csv-mode` when it is available, otherwise
`fundamental-mode`; set `csvdt-output-mode` to change that. A mode that turns
out not to be available leaves the buffer plain rather than losing a run csvdt
has already done the work for.

You can run csvdt on `*csvdt output*` itself to feed a result back through it,
which is the one case where a source buffer is replaced rather than left alone.

Each run replaces the buffer's contents outright, so overlays another package
left on the previous result are removed with it. That matters more than it
sounds: emptying a buffer collapses an overlay to zero width rather than
removing it, and one created to advance with text inserted at its end then
grows over whatever replaces it — so a line highlighted for having been banked
in the output buffer used to colour the entire next result.

The buffer is sent as UTF-8 with Unix line endings whatever the locale's
process encoding is, since csvdt requires UTF-8 and refuses anything else. So a
file Emacs decoded from cp1252, or one with CRLF endings, goes through as the
text you are looking at rather than as the bytes it was stored in.

## Errors

csvdt writes a specific reason for every refusal, so the wrapper just passes
them through rather than inventing its own:

```
csvdt exited 1: Specified column is out of bound: record 0 (line: 1) has 2 field(s), counted from 0
csvdt exited 100: '--offset' target +15:00 is not a real UTC offset; they run from -12:00 to +14:00
```

What csvdt wrote before failing is shown too, in `*csvdt output*` as usual.
That matters most for the "nothing converted" exit, where the output is
complete and its `parse_err` markers are exactly what the message tells you to
inspect. The one exception is a run fed from `*csvdt output*` itself: its
contents are the input, so a failed run leaves them alone.

A file buffer is never modified — output always goes to its own buffer, so
nothing can be lost to a mistaken argument.

## Tests

```bash
cargo build --release      # the tests drive a real csvdt
emacs/run-tests.sh
```

Both run in CI on every pull request, and on `master`, along with `cargo test`
on Linux, macOS and Windows.

`run-tests.sh` byte-compiles `csvdt.el` treating **any warning as a failure**,
then runs the checks listed below under `emacs -Q --batch` — it ends with
`ALL PASS (300 checks)`, and that count is the number worth trusting, since one
written down here goes stale the first time a check is added.

The count is checked, not just printed. A run reaching fewer checks than the
file declares says `ONLY 288 OF 300 CHECKS RAN` and fails, because `ALL PASS`
is also what a run prints where nothing was tried — a check commented out while
something was being chased, a `let` that returned early, an edit that left the
calls unreachable. Deleting twelve checks used to print `ALL PASS` and exit 0.
And because the line that prints the verdict is itself in that file, the runner
insists on seeing one: a tail that stops being reached takes Emacs out at 0 with
nothing said, and that now fails too.

Point it at a different binary with `CSVDT=/path/to/csvdt emacs/run-tests.sh`,
or a different Emacs with `EMACS=`.

`run-tests.sh`'s checks use a buffer with a header and a timestamp, because a
header-free one cannot tell the two apart: without an action applied, `-H -p`
and no arguments at all produce identical output.

What it checks:

- each command through its own `interactive` form, not just the function
  underneath it — testing only the layer below is what let a broken argument
  reader ship
- option discovery reads `--list-options`, and produces the same set as parsing
  `--help` does — no extras, none missed
- an unrecognised format version, text that is not a list, and no output at all
  are each declined rather than misread
- the fallback path, driven by a stub binary that refuses `--list-options`
- a cache recorded against a different binary is refilled rather than reused
- a whole buffer, a partial range and an empty buffer each produce the expected
  output
- every comma-bearing argument form survives the split and then runs end to end
  against csvdt — the reader is exercised, not bypassed
- either quote character comes off a value, each holds what the other cannot,
  and a backslash outside quotes stays as itself so a Windows path survives
- completion covers the argument at point, and a comma inside it is not a
  boundary
- a partially selected line expands to the whole line, whether it is the start
  or the end of the selection that falls mid-row, and a rectangle sends the
  whole rows it spans
- a non-contiguous region reaches csvdt as its pieces, gaps excluded, with
  nothing configured — driven by a buffer-local `region-extract-function`
- banked lines are ordered, de-duplicated, merged where they touch, accepted as
  overlays or conses, and sent as one file with the header in front
- a run covers the banked lines and the region together, without sending a line
  twice when they overlap, and each alone still behaves as it did
- a region below the header carries it, a whole-buffer run does not change, and
  nothing is prepended when the arguments do not ask for a header
- the header option is recognised on its own, as a long form, and clustered —
  and not when an H happens to fall inside another option's value
- every configured header option is one csvdt actually reports
- a result fed back through csvdt does not lose the input
- an overlay another package left on a previous result does not colour the next
  one, and no text properties reach the output whatever the source carried
- non-ASCII reaches csvdt as UTF-8, and dos endings as Unix ones, under a
  latin-1 process encoding — asserted on the actual bytes, via a stub that
  dumps its stdin
- an unavailable `csvdt-output-mode` still shows the output
- csvdt's own error text surfaces on a failure, with its exit status
- an `=`-attached argument, built from the offered candidate, actually runs
- the source buffer is not modified
- a missing binary says which variable to set
- **no csvdt option name appears in `csvdt.el`** — the property the whole design
  rests on, so it is asserted rather than assumed
- `package.el` can read the file as a package, and the `Version:` header it
  needs carries the crate's version rather than one of its own

Both of the last two failure modes were confirmed to actually fail: a warning
added to `csvdt.el` exits 1, and mentioning an option name in it exits 1. So
were the two guards on the count: twelve checks deleted exits 1 naming the
shortfall, and a tail made unreachable exits 1 saying nothing was proven.

There is a second runner for what batch mode cannot do:

```bash
emacs/tty-test.sh
```

`emacs --batch` reads its own stdin, so a keyboard macro never reaches a
minibuffer there. This one gives Emacs a pty and types for real. It needs
`script` from util-linux, which is why it is not part of `run-tests.sh`, and it
ends with `ALL PASS` over a count it declares and checks the same way the batch
runner does.

What it checks, all of it through the command loop:

- `RET` submits the prefilled arguments as they stand
- typing replaces them, because they are left selected
- `→` keeps them instead, to be added to, and `C-f` does the same
- a comma typed into a value stays inside the argument rather than ending it
- `TAB` completes a partial long option from what csvdt itself reports
- `M-p` recalls the previous line and runs it again
- that line is filed under csvdt's own history, not the one every minibuffer
  shares — every other check here passes either way, so this is the only one
  that can tell them apart
- a region marked with the keys a person marks one with sends only its own
  records, `csvdt-run-dwim` finds that region without being told, and with no
  region it runs the whole buffer

The last of those is what batch mode cannot reach at all: whether a region
counts as live is settled by `transient-mark-mode` and by whether a command
activated the mark, and the command loop is what settles both.

## Older csvdt binaries

A csvdt from before `--list-options` existed, or one printing a format version
this file doesn't read, falls back to parsing option names out of `--help`. That
path is exercised in the tests against a stub reporting `0.9.0` and refusing
`--list-options`, and it produces the same set — so pointing `csvdt-executable`
at an older build costs you the `=` hints and nothing else.
