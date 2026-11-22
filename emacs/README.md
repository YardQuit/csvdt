# csvdt.el

An Emacs front end for `csvdt`, meant to sit alongside `csv-mode`.

## Staying in step with csvdt

Piping `csvdt --help` into a help buffer is trivial and always current. But the
binary can also tell Emacs what its options *are*, so the package doesn't need
to know them:

- `csvdt-help` shows csvdt's own `--help`, read from the binary each time.
- The options offered when reading arguments come from `csvdt --list-options`,
  which csvdt prints from its own argument definitions. The result is cached
  against the binary's path and modification time, so a rebuild invalidates it
  by itself, without spawning csvdt again to ask.

Result: the package depends on no csvdt option name beyond the three it calls
to ask (`--list-options`, `--help`, `--version`) and `--lines`, which is how a
run on part of the buffer tells csvdt which lines were selected. No setting
names an option. A test asserts `--lines` is still an option csvdt reports, and
another that no other long option name appears in `csvdt.el`. Short options are
not covered by that test: the `-H -p` in `csvdt-default-arguments` is a default
you can change, which the package does not depend on. Add an option to csvdt
and it is completable in Emacs immediately, with no change here, checked
literally, by diffing the discovered set against the binary.

`--list-options` also says how each value must be attached, which the help
layout doesn't make machine-readable. `--flexible` is the one option whose value
must be joined with `=`, so `-f=` and `--flexible=` are offered as well as the
bare forms: `--flexible`'s value is optional, so the bare form is legitimate on
its own, and a value written separately is refused with the joined form shown.

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
the same one; the dependency is on an exact version-release.

Otherwise, from a checkout:

```elisp
(add-to-list 'load-path "/path/to/csvdt/emacs")
(require 'csvdt)

;; optional: bind the command map somewhere
(with-eval-after-load 'csv-mode
  (define-key csv-mode-map (kbd "C-c C-v") csvdt-command-map))
```

If `csvdt` isn't on `exec-path`, set `csvdt-executable` to its path. The
shell's rule applies: a name carrying a directory part (absolute, or relative
like `"./target/release/csvdt"`) means that file, resolved against
`default-directory` and never searched for on `exec-path`; only a bare name is.

Or install it as a package, which is what the `;;;###autoload` cookies in it
are for; the commands are then there without `require`:

```elisp
M-x package-install-file RET /path/to/csvdt/emacs/csvdt.el RET
```

`csvdt.el` is not versioned separately; it lives in the csvdt repository
because the two share a contract, so the copy next to a given csvdt is the one
that matches it. Its `Version:` header says exactly that: it carries csvdt's
own version, and a test fails if the two drift apart, the same bargain the RPM
spec's `Version` is held to. It works against older binaries anyway; see
below.

### Byte-compiling, and when it is worth it

Nothing here ships byte-compiled. The RPM installs `csvdt.el` as source
deliberately: an `.elc` is tied to the Emacs that produced it, which is why an
Emacs add-on usually pins a version, and the subpackage would stop being
`noarch` the moment it carried one. From a checkout the same is true, so
however it is installed, the commands run interpreted unless you compile them
yourself.

Whether that is worth doing depends on which path a run takes. Measured on a
25 MB, 500,001-row CSV with `-H -p -r 1` under Emacs 31.1, three runs each. Each
cell gives the time Emacs is busy starting the run, then the time until the
result is shown:

| | interpreted | byte-compiled |
| --- | --- | --- |
| whole buffer | 0.11–0.27 s, 1.3–1.6 s | 0.19–0.29 s, 1.3–1.5 s |
| 2000 banked ranges | 0.22–0.33 s, 0.22–0.35 s | 0.18–0.21 s, 0.19–0.23 s |
| 20000 banked ranges | 0.36–0.41 s, 0.40–0.46 s | 0.26–0.28 s, 0.30–0.32 s |

A whole-buffer run gains nothing from compiling, and a bigger file does not
change that. No Elisp touches a row: the time is the text being written out,
csvdt itself, and Emacs reading and decoding the result, all of which grow with
the file while the Lisp slice stays where it is.

Banked lines, or a region in several pieces, send only the lines picked, but
where each begins is worked out in Lisp, in one pass over the buffer for the
whole selection. The time follows the number of ranges, not that number times
the size of the file. Compiling is worth about a third of the busy time, so
compile if you bank lines, and do not expect a large file to speed up.

Byte-compiling is also a check. `run-tests.sh` compiles as a gate and treats
any warning as a failure, so a release compiles clean; a warning against your
own Emacs is worth reporting.

For a package install, `package-install-file` compiles as it installs and
there is nothing to do. For a checkout, particularly one loaded through a
symlink, where the file changes under Emacs whenever you pull, this compiles
it at start-up only when the source has moved on, and keeps the `.elc` out of
the working tree:

```elisp
(let* ((csvdt-path (expand-file-name "csvdt/csvdt.el" user-emacs-directory))
       (csvdt-elc  (expand-file-name "csvdt/csvdt.elc" user-emacs-directory))
       (recompiled nil))
  (add-to-list 'load-path (file-name-directory csvdt-path))
  (when (file-newer-than-file-p csvdt-path csvdt-elc)
    ;; Required first, so that `byte-compile-dest-file-function' is already a
    ;; special variable here: under lexical binding a `let' on an unknown name
    ;; is a lexical binding the compiler never reads.
    (require 'bytecomp)
    (let ((byte-compile-dest-file-function (lambda (_) csvdt-elc))
          (start (current-time)))
      (setq recompiled (and (ignore-errors (byte-compile-file csvdt-path))
                            (float-time (time-since start))))))
  ;; A source too broken to compile still has to load, so which file is loaded
  ;; is decided here rather than left to `load' finding the stale `.elc' first.
  (let ((compiled (and (file-exists-p csvdt-elc)
                       (not (file-newer-than-file-p csvdt-path csvdt-elc)))))
    (load (if compiled csvdt-elc csvdt-path) nil t)
    (message "csvdt loaded %s%s"
             (if compiled "compiled" "from source (interpreted)")
             (if recompiled (format ", recompiled in %.2fs" recompiled) ""))))
```

`csvdt-path` is the symlink; `file-newer-than-file-p` follows it, so editing
the file in the checkout is what triggers the rebuild. Writing the `.elc` next
to the symlink rather than beside its target is what keeps a build artefact
out of the repository; `run-tests.sh` removes its own for the same reason.
Compiling this file takes about a tenth of a second, so the rebuild is not a
start-up cost worth avoiding.

## Commands

| command | what it does |
| --- | --- |
| `csvdt-run-dwim` | run on the banked lines *and* the region together, else the whole buffer |
| `csvdt-run-buffer` | run on the whole buffer |
| `csvdt-run-region` | run on the region, handy for trying something on a few rows first |
| `csvdt-run-banked` | run on banked lines, from whatever banks them |
| `csvdt-help` | csvdt's own `--help` in a help buffer |
| `csvdt-describe-run` | what a run would cover, and why, without running it |
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
are not symmetric. Leaving `-H` off a file that *has* a header is loud: the
header comes back as `parse_err`. Passing it to a file that has *none* is
quiet: the first row is taken as a header and, depending on the insert
position, either goes through unconverted or is replaced by the generated
header name and lost outright. A prefill you had to notice and delete pointed
at the quiet mistake; one you have to keep does not.

It is a shell-style line and *not* a comma-separated list, because a comma is
csvdt's own separator inside a value: `-a 1,0`, `--remove 0,3,4`,
`-i 0,replace`, `-o0,+02:00`. Splitting the line on commas tore those in half.
Quoting works too, for a value that contains a space:

```
csvdt arguments: --print-delimiter " "
```

Either quote character, since the examples you paste come from a shell and a
shell takes either: `--comment '#'` arrives as one character, not three. A
double-quoted run honours the shell-familiar backslash escapes, `\t` `\n`
`\r` `\\` `\"` and no others, so `"\t"` is a tab but `"C:\data"` is refused
rather than read as Lisp's DEL escape, while a single-quoted one is literal
throughout. So a lone quote is written by wrapping it in the other kind, and a
value that must keep its backslashes belongs in single quotes; any other
backslash sequence inside double quotes is refused with exactly that advice:

```
csvdt arguments: --print-delimiter "'"
```

Outside quotes a backslash is left as itself, which is *not* the shell's rule.
A Windows path typed as a value has to survive as one argument, and there is no
filename here for shell escaping to be worth more than that; these commands
give csvdt the selected lines as a file of their own.

A word in that line that is neither an option nor an option's value is
refused, rather than run:

```
csvdt arguments: -H -p sales.csv
Cannot run: "sales.csv" is neither an option nor the value of one, and these
commands give csvdt its input file themselves
```

These commands always hand csvdt the selected lines as a temporary file after
your arguments, and csvdt takes one input file, so any other word left where
the file goes would be a second. csvdt would refuse it too, but its message
would name the temporary file, which you never typed, rather than the word you
did. A file, a word that names nothing, `-` and anything after `--` are all
refused the same way.

The commonest slip is a value written apart from an option that wants it joined
with `=`, and the message says so:

```
csvdt arguments: -H -p -f fix
Cannot run: "fix" is neither an option nor the value of one, and these
commands give csvdt its input file themselves; --flexible takes its value
joined with "=": -f=fix
```

Which arguments are values rather than stray words comes from
`--list-options`, so `-u 0`, `--trim none` and `-o 0,+01:00` are understood as
option-and-value without this file naming a single csvdt option.

## What gets processed

`csvdt-run-dwim` takes the **banked lines and the region together**: both, not
one or the other. The region is usually the piece you have not banked yet, so
dropping either would run over less than you picked out. They are merged, so a
selection that abuts or overlaps a banked stretch does not send a line twice.
With neither, the whole buffer.

csvdt is given the whole buffer, or the part it is narrowed to, and the lines
selected with `--lines`. It reads the buffer as it reads any file and writes
what a run over all of it writes for the records on those lines, each once, in
buffer order. So two banked lines inside one record that spans several lines
send it once, and the output for a selection is the output the whole buffer
would give for the same rows.

A run says what it covered when it was more than one stretch:

```
csvdt --lines 3-5,9,20-22,40-44 -H -p -d0: 13 lines from 4 stretches
```

Anything csvdt says on stderr about a run that still succeeded (how much did
not convert, rows a re-read would take for comments) appears with that
summary, so an Emacs user sees everything a shell user would:

```
csvdt -H -p -u0: 40123 lines
csvdt: 3 of 40122 values could not be converted, and were written as parse_err
```

Whatever the source, whole records are what go out. Half a line is half a
record: a selection starting or ending mid-row would hand csvdt a truncated
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

The run reports it, `csvdt --lines 2-3 -u0: 2 lines, selection widened to whole
lines`,
so a selection that was not what it looked like says so.

A record is not always a line, though. A quoted field may hold newlines (an
address, a notes column, an embedded document) and then one record covers
several lines, which is ordinary CSV and which csvdt reads and writes
faithfully. Widening to whole lines is not enough there: a selection that
begins inside such a field looks like a row and is the back half of one, and
sending it alone would convert cleanly a field nobody wrote:

```
buffer   : h,when
           "x
           y",2026-07-25T12:00:00+02:00

selected : y",2026-07-25T12:00:00+02:00     (line 3)

taken    : "x
           y",2026-07-25T12:00:00+02:00     -> first column "x\ny"
```

csvdt takes the whole record, since it reads the buffer from the top as a
full run does, and says which lines it took; the run's summary passes that
on: `, widened to whole records: lines 2-3`. Comment lines, blank lines, the
header under `-H`, `--single-quote` and `--read-delimiter` are all read by
csvdt in the one place, so nothing in this package reads the buffer as CSV.

Because csvdt reads the buffer from its start, what one row hands to the next
is handed on as in a full run: `-d` with one column measures the first
selected row from the row before it, selected or not, and `--flexible`
measures against the record a full run would. csvdt stops reading after the
last record the selection takes, so a selection near the top of a large
buffer is quick; the buffer is still written out whole for it first.

Narrowing is the one case widening cannot rescue: a buffer narrowed to begin
or end mid-line has no whole line to reach, and sending the lines beyond the
restriction would cover rows nobody chose. The run passes on the short line
and says `, first line cut short by the narrowing` or `, last line …`, rather
than leaving a truncated row to be discovered by counting columns. The end is
mentioned only when the selection reaches it, since csvdt stops reading
before.

A record can be cut where the line is not: a narrowing on a line boundary
that falls inside a quoted field. Seeing that means reading the buffer as CSV,
which is csvdt's job and not this package's, and csvdt is only given the
narrowed part. It reports the short record or the unclosed quote it then
finds; under `--flexible`, which accepts ragged records, it can pass
unnoticed, so narrow to whole records.

A run that fails names a cut too. csvdt sees a short record, refuses it, and
says so about a line, which sends you looking at data that is not the matter,
since the half of it out of reach is. The message ends `-- note the first line
cut short by the narrowing`.

## Banked lines

### Background

Emacs has no notion of *banked lines*. It has a **region**, and a region may be
non-contiguous (`region-bounds` reports one `(BEG . END)` per piece) and a
package defines its own selection by hooking `region-extract-function`. That is
the standard interface, and this package reads it, so anything expressed as a
region needs **no configuration whatever**: a rectangle arrives as one piece per
line, and a package that makes its selection non-contiguous arrives as its
pieces.

A bank is a different thing, and the difference is not stylistic. The region
answers *what is selected right now*, so releasing the mark takes its pieces
with it. A bank answers *what have I set aside so far*, which has to survive
exactly that, or it could not accumulate:

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
does with a bank once it has one is the line-shaped part: whole lines, buffer
order, touching stretches merged, and the lot given to csvdt as one run with
`--lines`, which widens them to whole records.

If the package has a function for it, use that.

```elisp
(setq csvdt-banked-lines-function #'some-package-banked-lines)
```

It is called in the buffer being processed and returns either `(BEG . END)`
conses or overlays. Ranges are put in buffer order, widened to whole lines and
merged where they touch, and csvdt widens them to whole records, so an
implementation need not do any of that. Nor need
it be tidy: a pair may be either way round, a dead overlay is dropped, `nil`
entries are ignored, a pair naming nothing is discarded rather than counted as
a stretch, and an overlay or marker belonging to an unrelated buffer is refused
rather than having its positions clamped into this one; an indirect buffer and
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
not follow point around: reported pieces stay exactly as reported while you
navigate:

```
mark active, point at line 2 : ((6 . 12) (18 . 24))
after moving to end of buffer: ((6 . 12) (18 . 24))   ; survives navigation
after deactivate-mark        : nil                    ; dies here
```

Two costs come with it:

- **The mark stays active for as long as the bank exists**, so everything else
  treats you as having a selection. `C-w` kills the bank, `M-w` copies it,
  typing under `delete-selection-mode` replaces it. That may be exactly right
  (donkey's own `y`/`d` do act on the bank) or it may be a loaded gun.
- **Any command that calls `deactivate-mark` drops the bank**, silently, and a
  great many commands do. A carefully assembled bank can be lost to an
  unrelated keystroke.
Storing the bank yourself is immune to both, which is what donkey does. The
variable above is what that costs: a bank that survives anything,
in exchange for one line saying where to look.

### A worked example: donkey

**If you use [donkey](https://github.com/YardQuit/donkey), this is the setting
you need.** It banks lines with `donkey-bank-selection` and reports them from
`donkey-banked-spans` as `(START . END)` conses in buffer order, the shape this
package takes, so one line connects them:

```elisp
(with-eval-after-load 'donkey
  (setq csvdt-banked-lines-function #'donkey-banked-spans))
```

Without it, `csvdt-run-banked` says there are no banked lines however many you
have banked, and `csvdt-run-dwim` quietly falls back to the region; see
[Background](#background).

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

and the eye lands on `donkey-banked-selection`, which is the **face**, the
*value* of the `face` property, not a property of its own. Naming it finds
nothing. Naming `face` finds the banked lines *and* everything else highlighted
in that buffer.

Failing that, name the overlay property it sets. Most packages mark banked
lines with an overlay:

```elisp
(setq csvdt-banked-lines-function
      (csvdt-banked-lines-from-overlays 'some-package-banked))
```

To find the property, put point on a banked line and run
`M-x describe-text-properties`: the overlays covering it are listed with what
they carry. You want a property **name**, not the face it is drawn with; those
look alike in that listing and only one of them works. `face` itself matches
every highlighted overlay in the buffer, which is rarely what you mean.

This route reaches into another package's internals, so it can break when that
package changes them. It is a line in your own configuration rather than
anything csvdt.el carries (the package names no banking implementation and
requires none) but it is worth knowing which of the two you are standing on.

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
  First line cut short by the narrowing.
```

The first two read differently on purpose: not knowing where banked lines come
from is a different problem from lines that only look banked, highlighted by
something else, or not actually banked by the package that highlights them.

The last is the one a run would surprise you with. A narrowing that begins or
ends mid-line leaves half a line reachable and nothing whole to widen back to,
so the run reports it, and this reports it first, in the same words, which is
the point of asking before rather than after.

It counts the lines that go to csvdt. Where a quoted field carries a record
over several lines csvdt widens to the whole record when it runs, and the
run's summary names the lines it took, so two banked lines inside one record
are described here as two stretches and the run then takes the record once.

The banked lines go to csvdt as **one run**, in buffer order, and it writes
for each what a run over the whole buffer writes, so a duration is measured
from the row before it in the buffer, banked or not. `csvdt-run-banked` runs the
banked lines only; `csvdt-run-dwim` takes them together with the region.

Until this is set, nothing is banked as far as csvdt.el is concerned, and a run
covers the region or the buffer as it always did.

## Modal editors, and other ways of selecting

Nothing here talks to an editing package directly. A run asks two questions
(is a region active, and where does it start and end) so any package that
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
| rectangle / visual block | every row it spans, **whole**; columns are ignored |
| non-contiguous | each piece, and **not** the gaps between them |

The last row is why the region is read through `region-bounds` rather than its
two ends: taking the ends would sweep up everything lying between the pieces.

A rectangle selecting columns 2–6 of three rows sends those three rows entire.
That is deliberate: a rectangle spans columns and a record does not, and
picking columns is csvdt's own
job: `--remove` and `-a/--arrange` do it by position, which survives the rows
changing shape.

Banking is different from selecting: it accumulates lines here and there, and
plain Emacs has no such notion, so it only applies if something you run provides
it. Set `csvdt-banked-lines-function` as above and the banked lines join the
region rather than replacing it.

## Regions keep the header

A region selected below the header has no header of its own. csvdt is given
the whole buffer, so under `-H` the buffer's first record is the header
whatever was selected, and `-p` writes it once at the top of the output. The
whole selection is data, and the output is named after the columns it
actually came from:

```
ts,x                            <- not selected
2026-07-25T12:00:00+02:00,a     <- region
2026-07-26T12:00:00+02:00,b     <- region

to_utc,x                        <- csvdt -H -p -u0 -i 0,replace
2026-07-25T10:00:00+00:00,a
2026-07-26T10:00:00+00:00,b
```

`csvdt-region-carries-header` used to turn this off, which handed csvdt the
first selected row as the header and lost that row. It is obsolete since
1.2.0, read by nothing, and goes in 2.0. So are `csvdt-header-options`,
`csvdt-single-quote-options`, `csvdt-read-delimiter-options` and
`csvdt-comment-options`, which fed the package's own reading of the buffer.

## Comment lines

Which lines are comments is csvdt's to say, with `--comment`, and since csvdt
reads the buffer itself a run on part of the buffer under `--comment` works as
any other: a comment line is skipped, a selection holding only comment lines
takes no record and csvdt says so, and a comment line holding a quote opens
nothing. Before 1.2.0 such a run was refused, because the package's own
reading of the buffer could not tell a comment from a record.

## Output

A run happens in the background. The command returns once csvdt is started
and Emacs stays usable while it works. The selected text is written to a
temporary file in pieces, from a timer, and csvdt is started on that file; the
result is shown, and the summary or the error is given, when csvdt is done. A
second run started meanwhile replaces the first, whose result is dropped. Only
placing the records of a selection, which reads the buffer once, happens before
the command returns: 0.1 to 0.4 s on a 25 MB file, whatever the selection.

If the buffer is edited while its text is still being written, the run is
stopped with "The buffer changed while csvdt was reading it", since what was
written no longer matches what you see. Once csvdt has started the buffer may
be changed freely.

Output goes to `*csvdt output*` in `csv-mode` when it is available, otherwise
`fundamental-mode`; set `csvdt-output-mode` to change that. A mode that turns
out not to be available leaves the buffer plain rather than losing a run csvdt
has already done the work for.

You can run csvdt on `*csvdt output*` itself to feed a result back through it,
which is the one case where a source buffer is replaced rather than left alone.

Each run replaces the buffer's contents outright. The result is built in a
buffer of its own and its text swapped in, so a large result is held once, and
overlays another package left on the previous result go with the old text. That
matters more than it sounds: emptying a buffer would collapse an overlay to zero
width rather than remove it, and one created to advance with text inserted at
its end would then grow over whatever replaces it, so a line highlighted for
having been banked in the output buffer would colour the entire next result.

The text is written as UTF-8 with Unix line endings whatever the locale's
coding system is, since csvdt requires UTF-8 and refuses anything else. So a
file Emacs decoded from cp1252, or one with CRLF endings, goes through as the
text you are looking at rather than as the bytes it was stored in.

## Errors

csvdt writes a specific reason for every refusal, so the wrapper just passes
them through rather than inventing its own:

```
csvdt exited 1: Specified column is out of bound: record 0 (line: 1) has 2 field(s), counted from 0
csvdt exited 100: '--offset' target +15:00 is not a real UTC offset; they run from -12:00 to +14:00
```

A failed run is reported in the echo area; the commands do not signal an error,
since by then they have returned. Lisp that needs to know calls
`csvdt--await` on what the run function returns, which waits and signals the
same message as a `user-error`.

What csvdt wrote before failing is shown too, in `*csvdt output*` as usual.
That matters most for the "nothing converted" exit, where the output is
complete and its `parse_err` markers are exactly what the message tells you to
inspect. The one exception is a run fed from `*csvdt output*` itself: its
contents are the input, so a failed run leaves them alone.

A file buffer is never modified; output always goes to its own buffer, so
nothing can be lost to a mistaken argument.

## Tests

```bash
cargo build --release      # the tests drive a real csvdt
emacs/run-tests.sh
```

Both run in CI on every pull request, and on `master`, along with `cargo test`
on Linux, macOS and Windows.

`run-tests.sh` byte-compiles `csvdt.el` treating **any warning as a failure**,
then runs the checks listed below under `emacs -Q --batch`. It ends with
`ALL PASS (300 checks)`, and that count is the number worth trusting, since one
written down here goes stale the first time a check is added.

The count is checked, not just printed. A run reaching fewer checks than the
file declares says `ONLY 288 OF 300 CHECKS RAN` and fails, because `ALL PASS`
is also what a run prints where nothing was tried: a check commented out while
something was being chased, a `let` that returned early, an edit that left the
calls unreachable. Deleting twelve checks would otherwise print `ALL PASS` and
exit 0. And because the line that prints the verdict is itself in that file, the
runner insists on seeing one: a tail that stops being reached takes Emacs out at
0 with nothing said, and that fails too.

Point it at a different binary with `CSVDT=/path/to/csvdt emacs/run-tests.sh`,
or a different Emacs with `EMACS=`.

`run-tests.sh`'s checks use a buffer with a header and a timestamp, because a
header-free one cannot tell the two apart: without an action applied, `-H -p`
and no arguments at all produce identical output.

The checks are plain batch Emacs Lisp reporting through one function,
`test-csvdt-check`, not ERT. So `M-x ert` and `ert-run-tests-batch` do not see
them, and a single check cannot be selected by name. Moving some three hundred
checks, several of which share state with the ones before them, would be a
rewrite of the file for a runner that adds a selector and nothing else, so it
has not been done. A run is asynchronous, and the tests wait for it through
`csvdt--await`, which `csvdt--run` is advised to call; the checks of the
background behaviour itself switch that off.

What it checks:

- each command through its own `interactive` form, not just the function
  underneath it: testing only the layer below is what let a broken argument
  reader ship
- option discovery reads `--list-options`, and produces the same set as parsing
  `--help` does: no extras, none missed
- an unrecognised format version, text that is not a list, and no output at all
  are each declined rather than misread
- the fallback path, driven by a stub binary that refuses `--list-options`
- a cache recorded against a different binary is refilled rather than reused
- a whole buffer, a partial range and an empty buffer each produce the expected
  output
- every comma-bearing argument form survives the split and then runs end to end
  against csvdt: the reader is exercised, not bypassed
- either quote character comes off a value, each holds what the other cannot,
  and a backslash outside quotes stays as itself so a Windows path survives
- completion covers the argument at point, and a comma inside it is not a
  boundary
- a partially selected line expands to the whole line, whether it is the start
  or the end of the selection that falls mid-row, and a rectangle sends the
  whole rows it spans
- a non-contiguous region reaches csvdt as its pieces, gaps excluded, with
  nothing configured, driven by a buffer-local `region-extract-function`
- banked lines are ordered, de-duplicated, merged where they touch, accepted as
  overlays or conses, and given to csvdt as one run with `--lines`
- a run covers the banked lines and the region together, without sending a line
  twice when they overlap, and each alone still behaves as it did
- a region below the header is read with it, a whole-buffer run does not
  change, no header is written when the arguments do not ask for one, and the
  obsolete `csvdt-region-carries-header` changes nothing
- a selection inside a record that spans lines takes the whole record once,
  and the summary names the lines csvdt took
- `--lines` is an option csvdt actually reports, and the five settings that
  fed the old reading of the buffer are obsolete since 1.2.0
- a result fed back through csvdt does not lose the input
- an overlay another package left on a previous result does not colour the next
  one, and no text properties reach the output whatever the source carried
- non-ASCII reaches csvdt as UTF-8, and dos endings as Unix ones, under a
  latin-1 locale, asserted on the actual bytes, via a stub that dumps its
  input file
- an unavailable `csvdt-output-mode` still shows the output
- csvdt's own error text surfaces on a failure, with its exit status
- a word that is neither an option nor an option's value is refused by name,
  with the joined form when it is a value written apart from `-f` or
  `--flexible`
- an `=`-attached argument, built from the offered candidate, actually runs
- the source buffer is not modified
- a run returns before it is reported, a text larger than one chunk arrives
  whole, a buffer edited while it is written stops the run, and a run started
  over another replaces it
- a run leaves no scratch buffer behind, whether it succeeded or failed
- a run on part of the buffer under a comment option runs, and one holding only
  a comment line says it took nothing
- the lines of any number of ranges are counted in one pass over the buffer,
  counted in characters crossed rather than timed
- a missing binary says which variable to set
- **no csvdt option name appears in `csvdt.el`**: the property the whole design
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
  shares; every other check here passes either way, so this is the only one
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
`--list-options`, and it produces the same set, so pointing `csvdt-executable`
at an older build costs you the `=` hints and nothing else.
