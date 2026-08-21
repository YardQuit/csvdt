"""What a run costs in time, against what KNOWN LIMITS promises.

The shape, not the seconds. How long a machine takes depends on the machine,
and a check that fails on somebody's laptop for being right is worse than no
check -- but how the time grows with the input is a property of the program,
and it is the same answer everywhere. Doubling the input should cost about
twice as long. A return to something worse than that is the regression worth
catching, and it is invisible to every other check here: output that takes an
hour to produce is still correct output.

The bound is one-sided and wide on purpose. Anything up to 3.0x per doubling
passes, which allows for cache, allocator and a loaded machine, and still
fails loudly on the 4x a quadratic writer would show. A doubling that costs
less than twice is not a regression and is never a finding: the way to get a
low ratio is for the smaller run to have been too quick to measure, and
failing a release because a machine was fast is the fault this file exists to
avoid.

Two shapes are asked about, since they cost differently:

  one enormous field, which is what an unclosed quote leaves behind -- the
  whole rest of the file arriving as a single value;

  the same field made of nothing but quote characters, which is the writer's
  worst case, because every one of them doubles again on the way out.

A file of ordinary rows is measured too, and only reported: it is the case
where memory stays flat and time is the only thing that grows, and the figure
is worth printing beside the others rather than held to a band.

Fixtures go to disk rather than being piped in, so what is timed is csvdt
reading a file rather than this process writing one.
"""
import os, subprocess, sys, tempfile, time

CSVDT = os.environ.get("CSVDT", "target/release/csvdt")
MIB = 1 << 20

# The most a doubling may cost. Nothing is asked of the other direction --
# see above.
SLOWEST = 3.0
# Below this the numbers are scheduler noise rather than measurement, and a
# ratio taken from them says nothing. Reported rather than passed silently.
FLOOR_SECONDS = 0.02


def written(directory, name, body):
    path = os.path.join(directory, name)
    with open(path, "w") as handle:
        handle.write(body)
    return path


def plain_field(directory, mb):
    return written(directory, f"plain{mb}.csv", "h\n" + "a" * (mb * MIB) + "\n")


def quote_field(directory, mb):
    """A quoted field of nothing but quote characters: 2 bytes in per byte."""
    return written(directory, f"quote{mb}.csv",
                   'h\n"' + '""' * (mb * MIB) + '"\n')


def rows_file(directory, mb):
    row = "2024-06-15T14:00:00Z,1700000000,ada,info\n"
    body = row * ((mb * MIB) // len(row))
    return written(directory, f"rows{mb}.csv", "when,epoch,who,level\n" + body)


def seconds(args, path, reps=3):
    """The quickest of REPS runs, which is the one least disturbed."""
    best = None
    for _ in range(reps):
        start = time.perf_counter()
        with open(os.devnull, "wb") as sink:
            subprocess.run([CSVDT] + args + [path], stdout=sink, stderr=sink,
                           timeout=600)
        taken = time.perf_counter() - start
        best = taken if best is None else min(best, taken)
    return best


def curve(label, make, directory, sizes, findings):
    """Time each size and hold every doubling to the band."""
    measured = []
    for mb in sizes:
        path = make(directory, mb)
        measured.append((mb, seconds(["-H", "-p"], path)))
        os.unlink(path)

    print(f"  {label}: " + ", ".join(f"{mb}MB {t:.2f}s" for mb, t in measured))
    if measured[0][1] < FLOOR_SECONDS:
        print(f"      too quick to measure below {FLOOR_SECONDS}s; "
              "no ratio taken")
        return
    for (small, before), (large, after) in zip(measured, measured[1:]):
        ratio = after / before
        if ratio > SLOWEST:
            findings.append(
                (f"{label} {small}MB -> {large}MB cost {ratio:.2f}x, "
                 f"over {SLOWEST}x: {before:.2f}s -> {after:.2f}s"))


def main():
    findings = []
    with tempfile.TemporaryDirectory(prefix="csvdt-timing-") as directory:
        print("time against the size of one enormous field:")
        curve("plain     ", plain_field, directory, (16, 32, 64), findings)
        curve("all quotes", quote_field, directory, (8, 16, 32), findings)

        # Reported, not held: the case where memory stays flat and time is
        # the only thing that grows.
        path = rows_file(directory, 128)
        # Two runs rather than three: this one is printed, not held to a
        # band, so the quickest of two is enough to be worth reading.
        plain = seconds(["-H", "-p"], path, reps=2)
        converting = seconds(["-H", "-p", "-u0"], path, reps=2)
        os.unlink(path)
        print(f"  128MB of ordinary rows: {plain:.2f}s to reformat, "
              f"{converting:.2f}s converting a column")

    if findings:
        print(f"\n{len(findings)} timing(s) outside the documented shape:")
        for entry in findings:
            print("  ", entry)
        return 1
    print("timing: every doubling cost about twice the last")
    return 0


if __name__ == "__main__":
    sys.exit(main())
