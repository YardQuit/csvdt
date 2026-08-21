"""Each quoting style against the promise it makes about a second read.

'--quote' offers four styles, and the help makes a different promise for
each. Two of them promise the output is still the same table: 'always'
quotes every field, and 'necessary' quotes a field holding the delimiter,
the quote, a record terminator or the comment character. 'never' promises
the opposite in as many words -- invalid CSV, if that is what the data
needs -- so nothing is asked of it here. 'nonnumeric' sits between: it
quotes every field that is not a number, which is more than is needed, and
the one thing it consults is that numeric test.

What is checked is the table, not the bytes: read the input, run it through
csvdt, read the output back, and the two readings must hold the same
records. csv_roundtrip.py asks that of the default style; this asks it of
every style, and over output delimiters and comment characters as well,
which is where a style that cannot quote has somewhere to fail.

A mismatch under 'nonnumeric' is a finding unless csvdt said so: a bare
numeric field beginning with the comment character is a documented gap, and
the run reports how many rows it touches on stderr. Anything else is damage
nothing warned about.
"""
import csv, io, os, random, re, subprocess, sys

CSVDT = os.environ.get("CSVDT", "target/release/csvdt")

STYLES = ["always", "necessary", "nonnumeric"]
# Disjoint from COMMENTS, so no combination is refused for naming the same
# character twice. '.', '-', '+' and 'e' are the interesting ones: each can
# sit inside a field the numeric test calls a number, and so inside a field
# 'nonnumeric' writes bare.
DELIMITERS = [",", ";", "|", "\t", ".", "-", "+", "e"]
COMMENTS = [None, "#", "%", "@", "!"]

# Values chosen for the hazards: the delimiter and the quote, a record
# terminator, a leading comment character, the empty field -- and numbers,
# written bare under 'nonnumeric', in the shapes the numeric test accepts.
VALUES = ["plain", "x,y", "x;y", "x|y", "x\ty", "a.b", "a-b", "a+b", "hey",
          'q"q', 'a""b', "", " ", "#lead", "%lead", "@lead", "!lead",
          "a\nb", "a\r\nb", "a\rb", "12", "-5", "+5", "5.5", "1e5", "1E5",
          "inf", "nan", "0", "2024-06-15T14:00:00Z", "é", "中文"]

COMMENTED = re.compile(r"csvdt: (\d+) row\(s\) begin with")
SPLIT = re.compile(r"csvdt: (\d+) row\(s\) hold a number written bare")


def run(args, text):
    p = subprocess.run([CSVDT] + args, input=text.encode(),
                       capture_output=True, timeout=20)
    return (p.returncode, p.stdout.decode("utf-8", "replace"),
            p.stderr.decode("utf-8", "replace"))


def records(text, delimiter=","):
    """Read as Python's csv does, which is the reference reader here."""
    return [row for row in csv.reader(io.StringIO(text, newline=""),
                                      delimiter=delimiter, quotechar='"')
            if row != []]


def through_csvdt(text, delimiter, comment):
    """Read as csvdt does, which is the reader a second run would use.

    Needed wherever '--comment' is in play: a comment is recognised only
    where one begins a record, and a newline inside a quoted field does not
    begin one, so filtering lines by their first character in Python would
    model the wrong reader. Asking csvdt cannot get that wrong.
    """
    args = ["-f", "--print-delimiter", "\x01"]
    if delimiter != ",":
        args += ["--read-delimiter", delimiter]
    if comment:
        args += ["--comment", comment]
    code, out, _ = run(args, text)
    if code != 0:
        return None
    return records(out, delimiter="\x01")


def table(rng):
    width = rng.randint(1, 4)
    rows = [[rng.choice(VALUES) for _ in range(width)]
            for _ in range(rng.randint(1, 5))]
    buffer = io.StringIO(newline="")
    csv.writer(buffer, lineterminator="\n").writerows(rows)
    return buffer.getvalue()


def check(rng):
    """One draw: a table, a style, an output delimiter, a comment character."""
    style = rng.choice(STYLES)
    delimiter = rng.choice(DELIMITERS)
    comment = rng.choice(COMMENTS)

    text = table(rng)
    want = records(text)
    if not want:
        return None

    args = ["-H", "-p", "--quote", style, "--print-delimiter", delimiter]
    if comment:
        args += ["--comment", comment]
    code, out, err = run(args, text)
    if code != 0:
        # A refused run is a separate question, and there is nothing to
        # compare: csvdt wrote no table to read back.
        return None

    # The input is read with csvdt's own reader too when a comment character
    # is in use, since a value here may begin with one and be dropped on the
    # way in as readily as on the way out.
    if comment:
        want = through_csvdt(text, ",", comment)
        got = through_csvdt(out, delimiter, comment)
    else:
        got = records(out, delimiter=delimiter)
    if want is None or got is None:
        return None
    if want == got:
        return None

    # 'nonnumeric' cannot quote a number, so a bare number can begin with the
    # comment character and drop its row, or hold the delimiter and split it.
    # Both are gaps csvdt reports, and a run that reported one has accounted
    # for the difference; a run that stayed quiet has not.
    if style == "nonnumeric":
        dropped = COMMENTED.search(err)
        if dropped and comment and len(want) - len(got) == int(dropped.group(1)):
            return None
        split = SPLIT.search(err)
        if split and len(want) == len(got):
            widened = sum(1 for a, b in zip(want, got) if len(b) > len(a))
            if widened == int(split.group(1)):
                return None

    said = "; csvdt said: " + err.strip() if err.strip() else \
           "; csvdt said nothing"
    return (f"--quote {style} --print-delimiter {delimiter!r}"
            f"{' --comment ' + repr(comment) if comment else ''}\n"
            f"    input:  {text!r}\n"
            f"    output: {out!r}\n"
            f"    want:   {want!r}\n"
            f"    got:    {got!r}{said}")


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    rounds = int(sys.argv[2]) if len(sys.argv) > 2 else 400
    rng = random.Random(seed)
    bad, seen = [], set()
    for _ in range(rounds):
        found = check(rng)
        if found is None:
            continue
        signature = found.split("\n")[0]
        if signature in seen:
            continue
        seen.add(signature)
        bad.append(found)

    if bad:
        print(f"{len(bad)} quoting round-trip(s) lost records:\n")
        for entry in bad:
            print(" ", entry, "\n")
        return 1
    print(f"quoting: {rounds} draws round-tripped")
    return 0


if __name__ == "__main__":
    sys.exit(main())
