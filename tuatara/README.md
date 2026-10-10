# csvdt for Tuatara

The front end in [`emacs/`](../emacs/), written again for the Tuatara editor:
csvdt run over a buffer, a selection or banked lines, the result in a buffer
of its own. Same contract, same rules about whole records and the header, and
the same refusal to know csvdt's options. Where the two editors differ, this
one does what Tuatara does, and the differences are listed
[below](#where-it-differs-from-the-emacs-one).

It lives here rather than with Tuatara for the reason `csvdt.el` does: it and
the csvdt beside it share a contract, so the copy in a given checkout is the
one that matches it.

## Nothing here needs updating when csvdt changes

The help is csvdt's own `--help`, read from the program each time. The options
offered for completion come from `csvdt --list-options`, kept against the
program's real path and the time it was written, so a rebuilt csvdt is asked
again and an unchanged one is not run just to find that out. A csvdt too old
to list them has them read from `--help` instead.

Three options are named, each in a setting, because a run over part of a
buffer cannot be worked out without them: `*csvdt-header-options*`
(`-H`/`--has-header`), `*csvdt-single-quote-options*` (`--single-quote`) and
`*csvdt-read-delimiter-options*` (`--read-delimiter`). A test holds each to an
option csvdt still reports, and another fails if any other option csvdt
reports is named anywhere in `csvdt.lisp`. That second one caught a docstring
naming two of them on its first run, which is the point of having it.

## Setup

Link this directory into Tuatara's `packages/` as `csvdt`, and it is a package
like the ones that come with Tuatara:

```bash
ln -s /path/to/csvdt/tuatara /path/to/tuatara/packages/csvdt
```

```lisp
;; ~/.config/tuatara/init.lisp
(load-package "csvdt")
```

Without the link, load it by path, which does the same:

```lisp
(load "/path/to/csvdt/tuatara/load.lisp")
```

If `csvdt` is not on the PATH, set `*csvdt-program*` after either line. A name
with a slash in it means that file and is never looked for on the PATH; a
relative one, `"./target/release/csvdt"`, is taken from the directory of the
buffer you run it in.

```lisp
(setf *csvdt-program* "/home/me/src/csvdt/target/release/csvdt")
```

The settings, each with its docstring in `C-h v`:

| setting | default |
| --- | --- |
| `*csvdt-program*` | `"csvdt"` |
| `*csvdt-default-arguments*` | `"-H -p"` |
| `*csvdt-region-carries-header*` | `t` |
| `*csvdt-header-options*` | `("-H" "--has-header")` |
| `*csvdt-single-quote-options*` | `("--single-quote")` |
| `*csvdt-read-delimiter-options*` | `("--read-delimiter")` |
| `*csvdt-output-mode*` | `csv-mode` |

## Commands

| command | what it does |
| --- | --- |
| `csvdt-run-dwim` | the banked lines and the selection together, else the whole buffer |
| `csvdt-run-buffer` | the whole buffer |
| `csvdt-run-region` | the selection, to try something on a few rows first |
| `csvdt-run-banked` | the banked lines |
| `csvdt-describe-run` | what a run would cover, without running it |
| `csvdt-help` | csvdt's own `--help` |
| `csvdt-version` | which csvdt, and which time zone database is in it |

In CSV and TSV buffers they are under `C-c C-v`: `r` dwim, `b` buffer, `SPC`
selection, `k` banked, `?` describe, `h` help, `v` version, beside csv-mode's
own `C-c C-l` and `C-c C-s`. Anywhere else, `M-x`, or a key of your own:

```lisp
(bind-key "; c" 'csvdt-run-dwim :map :leader)   ; SPC ; c
```

## Arguments

Typed as a shell line, prefilled from `*csvdt-default-arguments*` and **left
selected**:

| key | what it does |
| --- | --- |
| type anything | replaces the prefill |
| `→` or `C-f` | keeps it, to add to |
| `RET` | runs it as it stands |
| `TAB` | completes the option before the cursor, from what csvdt reports |
| `M-p` / `M-n` | back and forth through lines run before |

Selected rather than merely offered for the reason `emacs/README.md` gives at
length: leaving `-H` off a file with a header is loud, passing it to a file
without one is quiet, and a prefill you have to keep points away from the quiet
mistake. The prompt names the csvdt that will run.

The line is split as a shell splits it, not on commas, since a comma is
csvdt's own separator inside a value (`-a 1,0`, `-i 0,replace`). Either quote
works. Double quotes take `\t` `\n` `\r` `\\` `\"` and refuse any other
backslash, so `"C:\data"` is refused rather than misread; single quotes take
everything as it is. Outside quotes a backslash is itself.

A filename in the line is refused: these commands give csvdt the lines on its
standard input, and a file named would take their place in a result that looks
exactly like a run over them. Only a name that is a file on disk is refused,
and which arguments are values rather than filenames comes from
`--list-options`, so `-u 0` with a file called `0` lying about is still a run.

## What gets processed

`csvdt-run-dwim` takes the banked lines **and** the selection, merged so a line
in both goes once; with neither, the whole buffer.

Banked lines are Tuatara's own (`bank-selected-lines`, `m l` in NORMAL state),
read straight from the buffer. There is nothing to configure: the Emacs version
needs `csvdt-banked-lines-function` only because Emacs has no bank of its own.
They go to csvdt as one file in buffer order, so a duration is measured across
what was banked rather than across what lies between, and running over them
leaves them banked.

The selection is taken as `y` and `d` take it: a line selection whole, a
rectangle by the rows it spans. A rectangle's columns are ignored on purpose:
picking columns is csvdt's job (`--remove`, `-a`), by position, which survives
the rows changing shape. A run takes the main selection only, not the places of
cursors (`t`) or a split.

Whatever was picked, whole records are what go out:

- A selection starting or ending inside a line takes the whole line, and says
  `, selection widened to whole lines`.
- A record is not always a line. A quoted field can hold a line end, and a
  selection starting inside one would send the back half of a record as a
  record of its own, which converts cleanly into a field nobody wrote. So the
  range is pushed out to record boundaries, and the run says `, widened to
  whole records: a quoted field spans lines`. Finding the boundary reads from
  the start of the buffer, quote to quote, so it costs as many steps as there
  are quotes before the selection. A whole-buffer run reads nothing.
- Two banked lines inside one multi-line record widen to the same record, and
  it is sent once.
- When the arguments ask for a header and what is sent starts below it, the
  buffer's header record goes in front, so csvdt does not take your first
  selected row for the header. Set `*csvdt-region-carries-header*` to nil to
  stop it.

`--single-quote` and `--read-delimiter` are read from the arguments before any
of that, since they decide where a record begins. `csvdt-describe-run` reads
them from `*csvdt-default-arguments*`, having no line to read yet, and says so
in its docstring.

## Running, and the output

A run is a Tuatara job. Quick ones look instant; one that takes more than half
a second carries on in the background with the editor free, shows in the mode
line and in `M-x list-jobs`, and `M-x stop-job` ends csvdt. A second run over
the same buffer while one is going is refused rather than queued, since the two
would race for the same output buffer.

The output goes to `*csvdt output*` in `*csvdt-output-mode*` (csv-mode), which
falls back to fundamental-mode if the mode cannot be entered rather than
losing the result. Lines banked in the previous result are let go, the text
they stood for being gone. You can run csvdt on `*csvdt output*` itself to feed
a result back through.

The source buffer is never changed. On success the echo area gets the line
count, the stretches when there was more than one, the widening notes above,
and anything csvdt said on stderr about a run that still succeeded:

```
csvdt -H -p -u0 -i 0,replace: 3 lines from 2 stretches
```

On failure it gets csvdt's own reason and exit status, and what csvdt wrote
before failing is shown in `*csvdt output*`, except when the run was fed from
that buffer, whose text is kept:

```
csvdt exited 100: '--utc' was given "0-i", which is not a column number. Columns are named by position, counted from 0.
```

## Where it differs from the Emacs one

- **No narrowing.** Tuatara has none, so there is no record cut short by a
  restriction to report, and none of the `cut short by the narrowing` notes.
- **Banks are native.** No `csvdt-banked-lines-function`, no overlay property
  to name; see above.
- **Runs are jobs**, in the background when they take time, stoppable.
- **Bytes as the buffer holds them.** Emacs decodes a file and re-encodes it as
  UTF-8 on the way to csvdt. Tuatara keeps a file's bytes as they are, so a file
  that is not UTF-8 reaches csvdt as it is on disk and csvdt refuses it in its
  own words. Line ends are still sent as `\n` when the buffer's are `\r\n`.
- **No command map to bind.** Tuatara binds commands, not keymaps, so the keys
  are installed in csv-mode and anything else is a `bind-key` line.

## Tests

```bash
cargo build --release
TUATARA=/path/to/tuatara tuatara/run-tests.sh
```

`TUATARA` defaults to a checkout beside this one (`../tuatara`). The runner
loads Tuatara from source with Quicklisp, in its toolbox when there is one, the
same way Tuatara's own `tools/test.sh` does, then this package and its tests.
Any compiler warning in the package fails the run. The runs need a csvdt:
`CSVDT=/path/to/csvdt`, else `target/release/csvdt`, else the one on the PATH;
without one they are skipped, not passed.

With the `packages/csvdt` link in place, Tuatara's own `tools/test.sh` picks
these tests up as well, the way it does for the packages that come with it,
and its documentation and key checks then cover this package's docstrings too.
They pass.

What is checked:

- command lines split as a shell splits them: commas kept, either quote, the
  five escapes and no others, backslashes outside quotes, pasted newlines
- `--list-options` read, an unknown format and no output declined, the
  `--help` fallback taking the first mention of each option
- records found across quoted line ends, a quote mid-field taken as data, half
  a line widened, a record covered twice sent once
- the header option alone, long, clustered, and not inside another option's
  value or past `--`
- every option this package names is one csvdt reports, and no other appears
  in `csvdt.lisp`
- a whole-buffer run through `M-x` and the minibuffer, with the prefill typed
  over, kept with `→`, and run as it stands with `RET`; the source unchanged
- a selection from mid-row to mid-row sends both rows whole with the header in
  front, and nothing in front when no header is asked for
- two banked rows sent as one file with the header, the row between left out;
  dwim over a bank and a selection that overlap, the shared row sent once
- a filename in the arguments refused, a value apart from its option not taken
  for one, csvdt's own error and status shown, a missing program named

They are not in CI. CI would have to build Tuatara, its grammars and its
terminal library first, and that is a bigger job than this package is.
