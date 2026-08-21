"""The numbers csvdt reports on stderr, against what it actually wrote.

Three counts reach a caller, and each is a claim about the future: how many
rows begin with the comment character and would be dropped by a second read,
how many hold a field --single-quote would misread, and how many values were
written as parse_err out of how many. A script may act on any of them, and
nothing checked that any of them was right.

These are stronger than the usual differential, because the claim is testable
rather than merely modellable: the output can be fed back through csvdt and
the loss counted. So each count is asked twice --

  once of a model here, which reads the output the way a second read would
  and counts what it loses, without consulting csvdt at all;

  and once of csvdt itself, by running the output back through it and
  counting the records that survive.

A count that agrees with the model but not with the second read would mean
the model and the program share a misunderstanding, which is the failure a
differential exists to catch.
"""
import csv, io, os, random, re, subprocess, sys

CSVDT = os.environ.get("CSVDT", "target/release/csvdt")

COMMENT_CHARS = ["#", ";", "%", "@", "!"]
QUOTING = ["never", "necessary", "always", "nonnumeric"]

# Values chosen so that a good few of them start with a comment character
# once written bare, or need the quoting that --single-quote misreads.
VALUES = ["plain", "x,y", "#lead", ";lead", "%lead", "@lead", "!lead",
          'q"q', "sp ace", "", "2024-06-15T14:00:00Z", "nope", "12",
          "a\nb", "tab\there"]


def run(args, text):
    p = subprocess.run([CSVDT] + args, input=text.encode(),
                       capture_output=True, timeout=20)
    return (p.returncode, p.stdout.decode("utf-8", "replace"),
            p.stderr.decode("utf-8", "replace"))


def records(text, quotechar='"'):
    """Read as Python's csv does, which is the reference reader here."""
    try:
        return [r for r in csv.reader(io.StringIO(text, newline=""),
                                      quotechar=quotechar) if r != []]
    except Exception:
        return None


def lines_beginning_with(text, comment):
    """Rows a second read with --comment would drop, counted from the text.

    A comment is only recognised where one begins a line, so this is a
    question about the written bytes rather than about the fields: a row
    whose line starts with the character is gone, whatever the field meant.
    """
    return sum(1 for line in text.split("\n")
               if line.startswith(comment))


def misread_rows(text):
    """Rows holding a field that reading with the single quote gets wrong.

    csvdt writes ordinary double-quoted CSV whatever the reader was told, so
    a field the writer quoted keeps its quotes as data under --single-quote
    and splits on any delimiter inside it.

    Rows *holding* such a field, which is what the message says and counts.
    Not rows that come out differently on that second read: a field holding
    a newline ends the record early there, so every row after it shifts too,
    and counting those counts the blast radius rather than the hazard. One
    row holding one such field made four rows differ, and the first draft of
    this check called that a divergence.
    """
    rows = records(text, '"')
    if rows is None:
        return None

    def written_quoted(field, alone):
        # What the writer puts quotes round: a field carrying a delimiter, a
        # quote or a record terminator -- and an empty field that is the
        # whole record, since bare it would be a blank line and a blank line
        # is skipped. An empty field beside others needs no quotes and gets
        # none, which is why 'a,' warns about nothing and a lone '' warns.
        return any(c in field for c in ',"\n\r') or (alone and field == "")

    return sum(1 for row in rows
               if any(written_quoted(f, len(row) == 1) for f in row))


COMMENTED = re.compile(r"csvdt: (\d+) row\(s\) begin with")
MISREAD = re.compile(r"csvdt: (\d+) row\(s\) hold a field")
CONVERTED = re.compile(r"csvdt: (\d+) of (\d+) values could not be converted")
NOTHING = re.compile(r"csvdt: nothing converted: all (\d+) values")


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    rounds = int(sys.argv[2]) if len(sys.argv) > 2 else 300
    rng = random.Random(seed)
    findings = []

    for _ in range(rounds):
        comment = rng.choice(COMMENT_CHARS)
        quoting = rng.choice(QUOTING)
        width = rng.randint(1, 3)
        rows = [[rng.choice(VALUES) for _ in range(width)]
                for _ in range(rng.randint(1, 5))]
        text = "".join(
            ",".join('"' + v.replace('"', '""') + '"' for v in row) + "\n"
            for row in rows)

        args = ["-H", "-p", "--comment", comment, "--quote", quoting]
        action = rng.choice([[], ["-u0"], ["-r0"]])
        status, out, err = run(args + action, text)

        def note(kind, detail):
            findings.append((kind, args + action, detail))

        if status == 101 or "panicked" in err:
            note("PANIC", err[:80])
            continue
        if status not in (0, 1, 2, 100):
            note(f"UNDOCUMENTED STATUS {status}", err[:80])
            continue
        if status in (2, 100) or not out:
            continue

        # 1. Rows that begin with the comment character.
        said = COMMENTED.search(err)
        claimed = int(said.group(1)) if said else 0
        modelled = lines_beginning_with(out, comment)
        if claimed != modelled:
            note("COMMENTED-OUT COUNT DISAGREES WITH THE OUTPUT",
                 f"said {claimed}, the output has {modelled}")
        # And what a second read really loses, asked of csvdt rather than
        # modelled: every row the first read kept, less what survives.
        #
        # Only where the output is CSV that reads back. '--quote never'
        # writes a field holding a delimiter bare -- 'x,y' comes out as two
        # fields on any second read -- which is what that style means and is
        # documented as such. Counting rows across that is counting two
        # different things, and the first draft of this check did: it called
        # every such run a divergence, 172 of them, with the program right
        # every time.
        roundtrips = quoting != "never"
        again_status, again, _ = run(
            ["-H", "-p", "--comment", comment], out)
        if roundtrips and again_status in (0, 1):
            kept = records(out)
            survived = records(again)
            if kept is not None and survived is not None:
                lost = len(kept) - len(survived)
                if lost != claimed:
                    note("A SECOND READ LOSES A DIFFERENT NUMBER",
                         f"said {claimed}, a second read lost {lost}")

        # 2. Rows a single-quote read would misread. Only claimed when the
        # run was told to read that way, so only asked then.
        said = MISREAD.search(err)
        if said:
            note("MISREAD COUNT WITHOUT --single-quote", said.group(0)[:60])

        # 3. Values written as parse_err, out of how many. The result column
        # is found by position, which needs the output to have the columns
        # it was written with -- so the same restriction as above.
        if action and roundtrips:
            body = records(out)
            if body:
                # Found by its header, not by position. A generated column
                # lands beside the one it came from -- that is what -i's
                # default placement means -- so with '-r0' the result sits
                # at index 1 and not at the end. Slicing from the input
                # width read an original column instead, found no parse_err
                # in it, and called the count wrong 110 times over.
                #
                # csvdt names such a column with a leading '<', which is the
                # marker to go by, and which also finds both of -s's two.
                made = [i for i, name in enumerate(body[0])
                        if name.startswith("<")]
                values = [row[i] for row in body[1:]
                          for i in made if i < len(row)]
                failed = sum(1 for v in values if v == "parse_err")
                total = len(values)
                said = CONVERTED.search(err)
                nothing = NOTHING.search(err)
                if said:
                    if (int(said.group(1)), int(said.group(2))) != (failed,
                                                                    total):
                        note("PARSE_ERR COUNT DISAGREES WITH THE OUTPUT",
                             f"said {said.group(1)} of {said.group(2)}, "
                             f"wrote {failed} of {total}")
                elif nothing:
                    if int(nothing.group(1)) != total or failed != total:
                        note("NOTHING-CONVERTED COUNT DISAGREES",
                             f"said all {nothing.group(1)}, "
                             f"wrote {failed} of {total}")
                elif failed:
                    note("PARSE_ERR WENT UNREPORTED",
                         f"wrote {failed} of {total}, stderr said nothing")

    # --single-quote gets its own pass, since the count is only made there
    # and the input has to be single-quoted to reach it.
    single = 0
    for _ in range(rounds // 3):
        width = rng.randint(1, 3)
        rows = [[rng.choice(VALUES) for _ in range(width)]
                for _ in range(rng.randint(1, 5))]
        text = "".join(
            ",".join("'" + v.replace("'", "''") + "'" for v in row) + "\n"
            for row in rows)
        single += 1
        status, out, err = run(["-H", "-p", "--single-quote"], text)
        if status == 101 or "panicked" in err:
            findings.append(("PANIC", ["--single-quote"], err[:80]))
            continue
        if status not in (0, 1) or not out:
            continue
        said = MISREAD.search(err)
        claimed = int(said.group(1)) if said else 0
        modelled = misread_rows(out)
        if modelled is not None and claimed != modelled:
            findings.append(("MISREAD COUNT DISAGREES WITH THE OUTPUT",
                             ["--single-quote"],
                             f"said {claimed}, reading it back differs on "
                             f"{modelled}"))

    print(f"tallies: seed {seed}, {rounds} runs and {single} single-quote "
          f"runs, {len(findings)} findings")
    for kind, args, detail in findings[:10]:
        print(f"  [{kind}] {' '.join(args)}")
        print(f"      {detail}")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
