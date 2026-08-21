# The help corpus

One file per option, plus the four trailing sections — `examples.txt`,
`formats.txt`, `limits.txt`, `exit_status.txt` — that `--help` ends with and
the manual page files under headings of their own.

These files are the text of `--help` and, through `--generate-man`, of the
page. Nothing is written twice: what you edit here is what both print.

## Wrapping, and the verbatim gutter

Wrap prose by hand at 64 columns. `--help` prints it at an indent of ten and
never rewraps, so the width you write is the width a reader gets.

The page is different: roff fills prose to the width the reader's terminal
happens to be, which is what a page is for. That is right for a paragraph and
wrong for a command, a sample of output, or a diagram whose columns line up —
filled, two shell lines become one that runs neither.

So mark those. A line that must reach the page exactly as written opens with
`|` in the first column:

```
Convert first if needed:
|    iconv -f CP1252 -t UTF-8 in.csv > out.csv
|    iconv -f UTF-16 -t UTF-8 in.csv > out.csv
A field whose data genuinely holds NUL bytes is passed through
```

The gutter is a mark for the renderer and never reaches a reader: it is
stripped from every string clap is given, so `--help` and `-h` print the line
without it, and stripped again on the way into the page. Keep a marked line to
64 columns like everything else, gutter counted: 64 plus the 14 the page
leaves an option's help is 78.

Fourteen, not the seven a section's body gets — a marked block lives in either,
and it is the deeper indent that decides. And a marked line is not exempt from
the measure the way it looks like it should be. The gutter buys a break, not
`.nf`, so filling stays on and roff still wraps a line that does not fit — then
justifies what it wrapped. A `--peek` example widened to 70 columns reached an
80-column page as

```
    $       csvdt       -H       -p        --peek        sales.csv
    xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

which is a command nobody can copy, produced by the one treatment that exists
to keep it copyable.

Everything else fills. An indented block with no gutter is a nested
paragraph: the page hands its indent to roff as `.RS` and lets the words
reach the reader's width, so a value's description under `--quote` or a body
under EXIT STATUS reads at 100 columns like the paragraph above it.

Mark the whole block, every line of it, not just the first.

Give a marked block a blank line either side. Seven were once written hard
against the prose above and below, which read as cramped in the page and in
`--help` alike, and made the same kind of block look like two different kinds
depending on which file it was in.

## What decides

`VERBATIM` and `preserve_aligned_blocks` in `src/main.rs`. These tests hold
the bargain: `the_verbatim_gutter_marks_the_page_and_never_shows_in_it` checks
that no gutter reaches anything csvdt prints and that every marked line
arrives in the page as written, with its breaks; `the_gutter_decides_what_is_held_and_what_fills`
covers the three treatments against a block written out by hand;
`the_help_files_fit_the_column_the_manual_page_leaves_them` holds every line
to the 64 above, and ties that number to the 14 it is derived from rather
than leaving it a bare constant; `every_verbatim_block_stands_clear_of_the_prose_around_it`
asks the blank line either side of every marked block, in every file — a
count is not written down here, because a count goes stale the moment a file
gains a block.

An unmarked block that should have been marked is the failure worth watching
for — but it is narrower than it sounds, because the gutter is not the only
thing that holds a block. A segment with two or more lines carrying a column
gap is held whether it was marked or not, which covers every table in the
corpus: take the gutter off the whole of one and the page does not change.
What the gutter alone holds is the block with no column gap in it — a shell
command, a line of quoted stderr, a run of sample timestamps.

Take it off *part* of one and the page does change, which is the trap that
sentence sets. Marking is what decides where a segment begins and ends, so
unmarking a middle row cuts one block into three, and the row left between
them is a segment of one: no second line, no column gap, and it reaches the
page as a nested paragraph for roff to fill while the rows above and below
it stay held. Partial marking is worse than none.
`every_verbatim_block_stands_clear_of_the_prose_around_it` does catch it,
though as crowding rather than as what it is — the fragments it leaves sit
hard against the line that lost its gutter.

A block left wholly unmarked mostly will not fail a test, since the corpus
cannot say what you meant. One class of it does:
`every_shell_command_in_the_corpus_is_held_verbatim` asks that a line whose
first non-space is `$ ` carries the gutter, since a command roff has filled
is a command nobody can copy. For anything else, render it and look:

```sh
cargo run --release -- --generate-man | groff -man -Tutf8 | less
```

## The first sentence is the summary

An option's entry in `-h` is the opening sentence of its file, taken rather
than written again, reflowed and folded to 64 columns. `--help` prints the
whole file. Nothing is authored twice, so the first sentence has to be able
to stand alone: it is what a reader checking a flag will see, and often the
only thing they see.

A sentence ends, to the splitter, at the first full stop that is not inside
brackets. The stop in `e.g.` is a full stop like any other, so an
abbreviation in that first sentence cuts it short:

```
      --remove <num[,num...]>
          Remove a column, e.g.
```

Put the abbreviation inside brackets — `(e.g. -o1,-06:00)` — which is where
the splitter does not look. Some entries already do, and a file carrying one
outside brackets further down is harmless, because the summary has ended by
then.

`every_summary_is_the_opening_of_the_help_it_summarises` asks this of every
option: that `-h`'s entry is how `--help`'s begins, that it ends a sentence,
that it was not cut at an abbreviation, and that its brackets balance.
