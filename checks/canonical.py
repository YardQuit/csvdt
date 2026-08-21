"""Canonical output is a fixed point, and buffer boundaries do not move it.

Two claims in one file, because both are about the writer.

The output is canonical CSV -- parsed and written again, quotes only where
needed, one line ending. That is a fixed-point claim: feeding the output
back must give the same bytes. The combinations the help says will differ on
a second read are left out, since there the second run is meant to.

And the writer buffers a megabyte, which moves every boundary a field can
straddle. A field whose quote, delimiter or newline lands exactly on one is
where a writer refilling mid-escape goes wrong, so those sizes are built
deliberately rather than waited for.
"""
import csv, io, os, random, subprocess, sys

CSVDT = os.environ.get("CSVDT", "target/release/csvdt")
csv.field_size_limit(10 ** 9)

FIELDS = ["a", "", " sp ", "a,b", 'q"q', "line\nbreak", "\ttab", "é",
          "12", "1.5", "-5", "#hash", "'single'", '"quoted"', "  ",
          "2024-06-15T14:00:00Z", "1700000000", "x" * 40, "\r", "a\r\nb"]

ARGSETS = [[], ["-H", "-p"], ["--quote", "always"], ["--quote", "necessary"],
           ["--comment", "#"], ["-H", "-p", "--comment", "#"],
           ["--trim", "all"], ["-f"], ["-f=fix"], ["-a", "0"],
           ["--print-delimiter", ";"]]

BOUNDARIES = [8 * 1024, 64 * 1024, 128 * 1024, 512 * 1024,
              1024 * 1024, 2 * 1024 * 1024]


def run(args, data):
    p = subprocess.run([CSVDT] + args, input=data, capture_output=True)
    return p.returncode, p.stdout, p.stderr


def as_csv(rows):
    buf = io.StringIO()
    csv.writer(buf, lineterminator="\n").writerows(rows)
    return buf.getvalue().encode()


def generate(rng):
    width = rng.randint(1, 5)
    rows = [[rng.choice(FIELDS) for _ in range(width)]
            for _ in range(rng.randint(1, 6))]
    text = as_csv(rows).decode()
    return text.encode() if rng.random() < 0.8 else text.rstrip("\n").encode()


def check_fixed_point(seed, rounds):
    rng = random.Random(seed)
    bad, seen = [], set()
    checked = 0
    for _ in range(rounds):
        data = generate(rng)
        for args in ARGSETS:
            code, once, _ = run(args, data)
            if code != 0:
                continue
            # A second read has to be told about a changed print delimiter.
            again = args + (["--read-delimiter", ";"]
                            if "--print-delimiter" in args else [])
            code2, twice, _ = run(again, once)
            checked += 1
            if code2 != 0 or once != twice:
                key = (tuple(args), once[:40], twice[:40])
                if key not in seen:
                    seen.add(key)
                    bad.append(f"{args}: {data!r}\n    once:  {once!r}\n"
                               f"    twice: {twice!r}")
    return checked, bad


def check_boundaries():
    bad = []
    for boundary in BOUNDARIES:
        for delta in (-1, 0, 1):
            for name, marker in (("quote", '"'), ("comma", ","),
                                 ("newline", "\n"), ("plain", "a")):
                value = "a" * (boundary + delta - 1) + marker
                data = as_csv([["h1", "h2"], [value, "tail"]])
                code, out, err = run(["-H", "-p"], data)
                if code != 0:
                    bad.append(f"{boundary // 1024}KiB{delta:+d} {name}: "
                               f"exit {code}: {err.decode()[:60]}")
                    continue
                rows = [r for r in csv.reader(
                    io.StringIO(out.decode(), newline="")) if r != []]
                if len(rows) != 2 or rows[1][0] != value or rows[1][1] != "tail":
                    bad.append(f"{boundary // 1024}KiB{delta:+d} {name}: "
                               f"field came back {len(rows[1][0]) if len(rows) > 1 else '?'} "
                               f"bytes, want {len(value)}")
    # And the worst case for a writer: every byte needs doubling.
    for size in (1024 * 1024 - 1, 1024 * 1024, 1024 * 1024 + 1):
        value = '"' * size
        code, out, _ = run(["-H", "-p"], as_csv([["h1", "h2"], [value, "tail"]]))
        rows = [r for r in csv.reader(io.StringIO(out.decode(), newline=""))
                if r != []]
        if code != 0 or len(rows) != 2 or rows[1][0] != value:
            bad.append(f"{size} quote bytes: not returned unchanged")
    return bad


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    rounds = int(sys.argv[2]) if len(sys.argv) > 2 else 150
    checked, bad = check_fixed_point(seed, rounds)
    print(f"canonical/fixed point: seed {seed}, {checked} pairs, "
          f"{len(bad)} not idempotent")
    for line in bad[:6]:
        print(f"  {line}")
    boundary = check_boundaries()
    print(f"canonical/buffer boundaries: {len(boundary)} wrong")
    for line in boundary[:6]:
        print(f"  {line}")
    return 1 if bad or boundary else 0


if __name__ == "__main__":
    sys.exit(main())
