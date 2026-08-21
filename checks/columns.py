"""--remove, -a/--arrange and -i's placement, modelled from the help.

These are pure functions of the record and the argument, and the help states
each precisely enough to write down independently. That is a different
question from whether they crash: this asks whether the answer is the one
documented, over every subset and every ordering of a small file's columns.
"""
import csv, io, itertools, os, subprocess, sys

CSVDT = os.environ.get("CSVDT", "target/release/csvdt")


def run(args, rows):
    data = "".join(",".join(r) + "\n" for r in rows).encode()
    p = subprocess.run([CSVDT] + args, input=data, capture_output=True)
    out = [r for r in csv.reader(io.StringIO(p.stdout.decode(), newline=""))
           if r != []]
    return p.returncode, out, p.stderr.decode()


def cells(rows, columns):
    return [[f"r{r}c{c}" for c in range(columns)] for r in range(rows)]


def arranged(row, named):
    """"The columns you want first, in the order you want them.  Columns you
    don't name follow, keeping their original relative order."""
    return ([row[i] for i in named]
            + [row[i] for i in range(len(row)) if i not in named])


def check_arrange():
    bad = []
    for width in range(1, 6):
        rows = cells(3, width)
        for size in range(1, width + 1):
            for named in itertools.permutations(range(width), size):
                args = ["-a", ",".join(str(c) for c in named)]
                code, got, err = run(args, rows)
                if code != 0:
                    bad.append((args, f"exit {code}: {err.strip()[:60]}"))
                    continue
                want = [arranged(r, list(named)) for r in rows]
                if got != want:
                    bad.append((args, f"{got} != {want}"))
                elif any(len(r) != width for r in got):
                    bad.append((args, f"width changed: {got}"))
    return bad


def check_remove():
    bad = []
    for width in range(1, 6):
        rows = cells(3, width)
        for size in range(1, width):     # removing every column is its own case
            for dropped in itertools.combinations(range(width), size):
                args = ["--remove", ",".join(str(c) for c in dropped)]
                code, got, err = run(args, rows)
                want = [[v for i, v in enumerate(r) if i not in dropped]
                        for r in rows]
                if code != 0:
                    bad.append((args, f"exit {code}: {err.strip()[:60]}"))
                elif got != want:
                    bad.append((args, f"{got} != {want}"))
    return bad


def check_out_of_bound():
    """"Every column named must exist, or the row is reported as out of
    bound" -- so none of these may come back a success."""
    bad = []
    rows = cells(2, 3)
    for args in (["-a", "3"], ["-a", "0,5"], ["--remove", "3"],
                 ["--remove", "0,9"], ["-a", "99"], ["-u3"], ["-d0,7"]):
        code, got, _ = run(args, rows)
        if code == 0:
            bad.append((args, f"a column past the end was accepted: {got}"))
    return bad


def check_insert():
    """-i places the produced value at a named column, and with no -i it goes
    right after the action's own column -- or its second, for a two-column
    --duration."""
    bad = []
    width = 4
    rows = [[str(1700000000 + r) for _ in range(width)] for r in range(2)]

    def produced(row_index):
        return f"2023-11-14T22:13:{20 + row_index:02d}+00:00"

    for column in range(width):
        for where in ("before", "after", "replace"):
            args = ["-r0", f"-i{column},{where}"]
            code, got, err = run(args, rows)
            if code != 0:
                bad.append((args, f"exit {code}: {err.strip()[:60]}"))
                continue
            for index, row in enumerate(got):
                source, value = rows[index], produced(index)
                if where == "replace":
                    want = list(source)
                    want[column] = value
                elif where == "before":
                    want = source[:column] + [value] + source[column:]
                else:
                    want = source[:column + 1] + [value] + source[column + 1:]
                if row != want:
                    bad.append((args, f"row {index}: {row} != {want}"))

    # The default placement, which is not the same question.
    timestamps = [["2024-01-01T00:00:00Z", "2024-01-02T00:00:00Z", "x"]]
    for args, position in ((["-u0"], 1), (["-u1"], 2),
                           (["-d0,1"], 2), (["-d1,0"], 1), (["-d0"], 1)):
        code, got, err = run(args, timestamps)
        if code != 0:
            bad.append((args, f"exit {code}: {err.strip()[:60]}"))
        elif len(got[0]) != 4:
            bad.append((args, f"expected one added column, got {got[0]}"))
        else:
            # The added value is whichever cell is not one of the originals.
            added = [i for i, v in enumerate(got[0]) if v not in timestamps[0]]
            if added != [position]:
                bad.append((args, f"placed at {added}, want [{position}]"))
    return bad


def main():
    total = 0
    for name, check in (("arrange", check_arrange), ("remove", check_remove),
                        ("out of bound", check_out_of_bound),
                        ("insert", check_insert)):
        bad = check()
        total += len(bad)
        print(f"columns/{name}: {len(bad)} divergences")
        for args, detail in bad[:6]:
            print(f"  {' '.join(args)}: {detail}")
    return 1 if total else 0


if __name__ == "__main__":
    sys.exit(main())
