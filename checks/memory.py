"""What a run costs in memory, against what KNOWN LIMITS promises.

Two claims, and they are of different kinds. That memory grows with the
widest record and not with the file is a property of the design -- break it
and a large file stops working on a small machine, quietly, in the field.
That one enormous field costs about twice its own size is a measured figure,
and figures drift.

So the first is held tightly and the second loosely. A run over a million
rows must not cost meaningfully more than a run over a thousand: that is the
regression worth catching, and it is the same answer on any machine. The
multiplier is allowed a wide band, because it depends on an allocator and a
page size, and a check that fails on somebody's laptop for being right is
worse than no check.

Fixtures are written to disk rather than held here and piped in. Measuring a
child's peak by fork and wait4 while the parent holds the fixture in memory
reports the parent's pages, inherited across the fork: that read 99 MiB for
an 88 MB file where the kernel's own high-water mark for the run was 5 MiB,
and made a flat curve look linear.
"""
import os, resource, subprocess, sys, tempfile

CSVDT = os.environ.get("CSVDT", "target/release/csvdt")
MIB = 1 << 20


def peak_kib(args, path):
    """The child's high-water mark, in KiB, for one run over PATH.

    Taken from RUSAGE_CHILDREN, which only ever rises within a process, so
    each measurement is made in a subprocess of its own and read back.
    """
    reader = (
        "import os,resource,subprocess,sys\n"
        "with open(os.devnull,'wb') as n:\n"
        "    subprocess.run(sys.argv[1:], stdout=n, stderr=n)\n"
        "print(resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss)\n"
    )
    out = subprocess.run([sys.executable, "-c", reader, CSVDT] + args + [path],
                         capture_output=True, timeout=300)
    try:
        return int(out.stdout.decode().strip())
    except ValueError:
        return None


def rows_file(directory, n):
    path = os.path.join(directory, f"rows-{n}.csv")
    with open(path, "wb") as f:
        f.write(b"a,b\n")
        row = b"2024-06-15T14:00:00Z,x\n"
        for _ in range(0, n, 10000):
            f.write(row * min(10000, n))
    return path


def field_file(directory, mb):
    path = os.path.join(directory, f"field-{mb}.csv")
    with open(path, "wb") as f:
        f.write(b'a,b\n"')
        chunk = b"z" * MIB
        for _ in range(mb):
            f.write(chunk)
        f.write(b'",1\n')
    return path


def main():
    findings = []
    if not sys.platform.startswith("linux"):
        print("memory: skipped, ru_maxrss is not in KiB off Linux")
        return 0

    with tempfile.TemporaryDirectory() as directory:
        # 1. Memory must not grow with the file. A thousand rows and a
        # million of the same row differ by 23 MB of input; the run should
        # not differ by anything like it.
        small = peak_kib(["-H", "-p"], rows_file(directory, 1_000))
        large = peak_kib(["-H", "-p"], rows_file(directory, 1_000_000))
        if small is None or large is None:
            print("memory: could not measure, skipped")
            return 0
        grew = (large - small) / 1024.0
        # A generous ceiling: anything under a few MiB is buffers and noise,
        # while a run that held the file would be up by 20 MiB and more.
        if grew > 8:
            findings.append(
                ("MEMORY GROWS WITH THE FILE",
                 f"1k rows {small/1024:.1f} MiB, 1M rows {large/1024:.1f} MiB, "
                 f"up {grew:.1f} MiB over 23 MB of extra input"))

        # 2. One enormous field costs about twice its own size. Measured at
        # two sizes so the baseline falls out of the difference rather than
        # having to be guessed at.
        eight = peak_kib(["-H", "-p"], field_file(directory, 8))
        thirty_two = peak_kib(["-H", "-p"], field_file(directory, 32))
        if eight is not None and thirty_two is not None:
            marginal = (thirty_two - eight) / 1024.0 / 24.0
            if not 1.2 <= marginal <= 3.5:
                findings.append(
                    ("A LARGE FIELD COSTS A DIFFERENT MULTIPLE",
                     f"{marginal:.2f}x the field, and the help says about "
                     f"twice; 8 MB {eight/1024:.1f} MiB, "
                     f"32 MB {thirty_two/1024:.1f} MiB"))

    print(f"memory: {len(findings)} findings")
    for kind, detail in findings:
        print(f"  [{kind}]")
        print(f"      {detail}")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
