"""--peek's numbering against the fields a run actually sees.

--peek exists to answer one question -- which number does this column have --
and the whole of its worth is that the answer matches the number an action
then takes. Nothing checked that. It is the only mode with no differential
cover at all, which matters more now that these checks stand between a build
and a release.

The model is a run of csvdt itself with no action asked for, which writes the
records back as it read them. That is the field list to beat. --peek shows
the same records padded into a table, so two things must hold for every input
and every way of reading it:

  the numbering covers as many columns as the widest row shown has fields,
  and the rows shown are the rows a run would have worked on.

Field text is compared as well where the generated fields make that sound --
no spaces to be confused with padding, nothing needing an escape. Where they
do not, the counts still are, and the awkward fields are the point of
generating them.

Which rows --peek shows is part of the contract and is modelled here rather
than read back from its output: with -H and -p, the header and the first data
row; with -H alone, the first data row, since the header is a header and not
data; with neither, the first record, which is data like any other.
"""
import csv, io, os, random, subprocess, sys

CSVDT = os.environ.get("CSVDT", "target/release/csvdt")

# Each is a way of reading that changes where a field begins or ends, and so
# changes the answer. The help says all four apply to --peek.
DIALECTS = [
    ([], ","),
    (["--read-delimiter", ";"], ";"),
    (["--single-quote"], ","),
    (["--trim", "all"], ","),
    (["--comment", "#"], ","),
]

SIMPLE = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
# Characters that make a field awkward to read back out of a padded table:
# two spaces look like a column gap, a tab or newline is shown as an escape,
# and a wide character occupies one column by count and two on a terminal.
AWKWARD = ["a b", "a  b", "x\ty", "p\nq", "q\"q", "  lead", "trail  ",
           "", "naïve", "日本", "a\\tb", "-", "#hash", ";semi"]


def field(rng, awkward):
    if awkward and rng.random() < 0.5:
        return rng.choice(AWKWARD)
    return "".join(rng.choice(SIMPLE) for _ in range(rng.randint(1, 8)))


def quote(value, delimiter):
    if any(c in value for c in (delimiter, '"', "\n", "\r")):
        return '"' + value.replace('"', '""') + '"'
    return value


def generate(rng, awkward):
    """A small CSV, sometimes ragged, and the rows it is built from."""
    width = rng.randint(1, 5)
    rows = []
    for _ in range(rng.randint(1, 4)):
        # Ragged on purpose a third of the time: the help says uneven records
        # are shown rather than refused, and that is when somebody is most
        # likely to be looking for a column number.
        n = width if rng.random() < 0.67 else rng.randint(1, 6)
        rows.append([field(rng, awkward) for _ in range(n)])
    return rows


def run(args, text):
    p = subprocess.run([CSVDT] + args, input=text.encode(),
                       capture_output=True, timeout=20)
    return p.returncode, p.stdout.decode("utf-8", "replace"), \
        p.stderr.decode("utf-8", "replace")


def shown_rows(records, has_header, print_header):
    """The records --peek shows, by the rule its help states."""
    if not records:
        return []
    if has_header and print_header:
        return records[:2]
    if has_header:
        return records[1:2]
    return records[:1]


def numbering(line):
    """The column numbers off --peek's first line."""
    return line.split()


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    rounds = int(sys.argv[2]) if len(sys.argv) > 2 else 400
    rng = random.Random(seed)
    findings = []

    for _ in range(rounds):
        awkward = rng.random() < 0.5
        dialect, delimiter = rng.choice(DIALECTS)
        rows = generate(rng, awkward)
        text = "".join(
            delimiter.join(quote(f, delimiter) for f in row) + "\n"
            for row in rows)
        header = rng.choice([[], ["-H"], ["-H", "-p"]])

        peeked_status, peeked, _ = run(dialect + header + ["--peek"], text)
        # A run with no action writes the records back as csvdt read them,
        # which is what --peek must have been looking at.
        #
        # With -f, because --peek shows uneven records rather than refusing
        # them, "whatever --flexible would have done". A model run without it
        # stops at the first ragged record and exits 1, and comparing that to
        # a --peek that showed the file is comparing two different questions:
        # the first draft did, and called 173 of 400 inputs a divergence when
        # every one of them was the documented behaviour of both.
        #
        # -f rather than a filling mode, since that is the one that passes
        # records through as they are and so still reports the field counts
        # the file really has.
        seen_status, seen, _ = run(dialect + ["-H", "-p", "-f"], text)

        def note(kind, detail):
            findings.append((kind, dialect + header, kind, detail))

        if peeked_status == 101 or seen_status == 101:
            note("PANIC", repr(text)[:90])
            continue
        # Either both read the file or neither does. A --peek that refuses
        # what a run accepts, or accepts what a run refuses, is reading it
        # differently -- which is the one thing this mode must not do.
        if (peeked_status == 0) != (seen_status == 0):
            note("DISAGREED ON THE FILE",
                 f"peek {peeked_status}, run {seen_status}: {text!r}"[:110])
            continue
        if peeked_status != 0:
            continue

        records = [r for r in csv.reader(io.StringIO(seen, newline=""))
                   if r != []]
        want = shown_rows(records, "-H" in header, "-p" in header)
        lines = peeked.split("\n")
        while lines and lines[-1] == "":
            lines.pop()
        if not lines:
            if want:
                note("SHOWED NOTHING", f"a run saw {len(want)} rows")
            continue

        numbers, shown = numbering(lines[0]), lines[1:]

        if len(shown) != len(want):
            note("WRONG NUMBER OF ROWS",
                 f"showed {len(shown)}, a run works on {len(want)}")
            continue
        if not want:
            continue
        # The help: the numbering covers the widest of the rows shown.
        widest = max(len(r) for r in want)
        if len(numbers) != widest:
            note("NUMBERING DOES NOT COVER THE ROWS",
                 f"numbered {len(numbers)}, widest row has {widest}")
            continue
        if numbers != [str(i) for i in range(widest)]:
            note("NUMBERING IS NOT 0..n", " ".join(numbers)[:70])
            continue

        # Where nothing in the fields can be confused with the padding, the
        # text is comparable too, and a numbering that lines up over the
        # wrong fields is only visible here.
        plain = all(all(f and not any(c in f for c in ' \t\n\r"\\') for f in r)
                    for r in want)
        if plain:
            for row, expected in zip(shown, want):
                if row.split() != expected:
                    note("FIELDS ARE NOT THE ONES A RUN SEES",
                         f"{row.split()} vs {expected}")
                    break

    # And the other half of the contract: the options that write have no CSV
    # here to act on, so each is refused -- with a status, never a panic --
    # while the ones that decide what a record is apply and are welcome.
    #
    # The set is taken from --list-options rather than written out here. A
    # hand-written list covers what somebody remembered: this one covered
    # thirteen invocations of twelve options and left --round-seconds, the
    # thirteenth in --peek's own conflict list, untried -- which is also the
    # one the help forgot to name. Driven from the binary's account of itself,
    # an option added later is refused by default and has to be argued into
    # WELCOME to pass.
    WELCOME = {"--has-header", "--print-header", "--trim",
               "--read-delimiter", "--single-quote", "--comment"}
    # Modes that answer and exit rather than joining a run. --help and
    # --version win over the rest of the line rather than refusing it.
    APART = {"--peek", "--list-options", "--generate-man",
             "--help", "--version"}
    # A value each will accept, so a refusal is the conflict with --peek and
    # not a value the option would have turned down on its own.
    VALUES = {"--flexible": "widest", "--insert": "0,replace",
              "--rfc3339": "0", "--utc": "0", "--local": "0",
              "--offset": "0,+02:00", "--split": "0", "--duration": "0,1",
              "--remove": "0", "--arrange": "1,0", "--trim": "all",
              "--read-delimiter": ";", "--comment": "#",
              "--print-delimiter": ";", "--quote": "always",
              "--fill-log": "-"}

    listed = subprocess.run([CSVDT, "--list-options"], capture_output=True,
                            timeout=20).stdout.decode()
    refusals = 0
    welcomed = 0
    for line in listed.splitlines():
        if line.startswith("#") or not line.strip():
            continue
        _short, long, takes = line.split("\t")[:3]
        if long in APART:
            continue

        # -p is refused without -H on its own account, which is not a conflict
        # with --peek and must not be counted as one.
        args = ["--peek"] + (["-H"] if long == "--print-header" else []) + [long]
        if takes != "none":
            if long not in VALUES:
                findings.append(("NO VALUE TO TRY IT WITH", [long], "",
                                 "this check has no value for an option that "
                                 "takes one, so it cannot ask about it"))
                continue
            args.append(VALUES[long])

        status, _, err = run(args, "a,b\n1,2\n")
        if status == 101 or "panicked" in err:
            findings.append(("PANIC ON A REFUSAL", args, "", err[:90]))
        elif long in WELCOME:
            welcomed += 1
            # Only that it is not refused. Whether it then applies is the
            # question the first half of this file answers, by comparing the
            # fields --peek shows against the ones a run sees under each of
            # these dialects -- which is the harder question and the one worth
            # asking, since an option accepted and ignored exits 0 too.
            if status != 0:
                findings.append(("REFUSED AN OPTION PEEK READS WITH", args, "",
                                 f"exit {status}, wanted 0: {err[:70]}"))
        else:
            refusals += 1
            if status != 2:
                findings.append(("NOT REFUSED", args, "",
                                 f"exit {status}, wanted 2"))
    if welcomed != len(WELCOME):
        findings.append(("AN OPTION PEEK READS WITH WENT UNTRIED", [], "",
                         f"{welcomed} of {len(WELCOME)} reached"))

    print(f"peek: seed {seed}, {rounds} runs, {welcomed} options it reads "
          f"with and {refusals} refusals, "
          f"{len(findings)} findings")
    for kind, args, _, detail in findings[:10]:
        print(f"  [{kind}] {' '.join(args)}")
        print(f"      {detail}")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
