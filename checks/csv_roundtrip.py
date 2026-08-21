"""Whatever records another reader finds in the input, csvdt must write back.

The reference is Python's csv module: an implementation written by other
people from the same specification, which is the point -- a test written
against csvdt's own expectations cannot find a place where those
expectations are wrong.

Blank lines are dropped from the reference, since the csv crate skips them
by design, and a byte-order mark is stripped before comparing because csvdt
strips it. Everything else must survive: quotes, doubled quotes, delimiters
and newlines inside fields, every line ending, and an unclosed quote, which
both readers take the same way.
"""
import csv, io, os, random, subprocess, sys

CSVDT = os.environ.get("CSVDT", "target/release/csvdt")
csv.field_size_limit(10 ** 8)

FIELDS = [
    "", "a", "abc", " a", "a ", "  ", "0", "-5", "1.5",
    '"a"', '"a,b"', '"a""b"', '""', '"""', '"a\nb"', '"a\r\nb"', '"a\rb"',
    'a"b', '"', 'a,b', "a\tb", "é", "中文", "\U0001f600",
    '" a "', '"a', 'x"y"z', "'a'", "'a,b'",
]
EOLS = ["\n", "\r\n", "\r"]


def reference(data, delimiter=",", quote='"'):
    return [row for row in csv.reader(io.StringIO(data, newline=""),
                                      delimiter=delimiter, quotechar=quote)
            if row != []]


def run(args, data):
    p = subprocess.run([CSVDT] + args, input=data.encode(),
                       capture_output=True)
    return p.returncode, p.stdout.decode("utf-8", "replace")


def generate(rng, rectangular=True):
    width = rng.randint(1, 4)
    rows = []
    for _ in range(rng.randint(1, 6)):
        w = width if rectangular else rng.randint(1, 4)
        rows.append(",".join(rng.choice(FIELDS) for _ in range(w)))
    eol = rng.choice(EOLS)
    data = eol.join(rows)
    if rng.random() < 0.7:
        data += eol
    if rng.random() < 0.1:
        data = "﻿" + data
    return data


def check(label, args, data):
    code, out = run(args, data)
    if code != 0:
        return None          # a refused input is a separate question
    want = reference(data.lstrip("﻿"))
    got = reference(out)
    if want == got:
        return None
    return (f"{label} {args}\n    input:  {data!r}\n"
            f"    output: {out!r}\n    want:   {want!r}\n    got:    {got!r}")


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    rounds = int(sys.argv[2]) if len(sys.argv) > 2 else 250
    rng = random.Random(seed)
    bad, seen = [], set()
    for _ in range(rounds):
        data = generate(rng)
        for label, args in (("plain", []), ("flexible", ["-f"]),
                            ("header", ["-H", "-p"])):
            found = check(label, args, data)
            if found and found.split("want:")[-1][:80] not in seen:
                seen.add(found.split("want:")[-1][:80])
                bad.append(found)
        found = check("ragged", ["-f"], generate(rng, rectangular=False))
        if found and found.split("want:")[-1][:80] not in seen:
            seen.add(found.split("want:")[-1][:80])
            bad.append(found)
    print(f"csv round-trip: seed {seed}, {rounds} files, {len(bad)} divergences")
    for line in bad[:8]:
        print(f"  {line}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
