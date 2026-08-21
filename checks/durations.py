"""-d, refereed in whole nanoseconds.

Python's timedelta stops at microseconds, so it cannot judge a tool that
keeps nanoseconds: asked to check a span of .123456789 it reports .123457
and calls the exact answer wrong. The span is therefore computed here as an
integer number of nanoseconds, and csvdt's ISO 8601 duration parsed back the
same way, so the comparison is exact at the precision csvdt actually works
to.
"""
import calendar, csv, io, os, random, re, subprocess, sys
from datetime import date

CSVDT = os.environ.get("CSVDT", "target/release/csvdt")
EPOCH = date(1970, 1, 1).toordinal()

TIMESTAMP = re.compile(r"(\d{4})-(\d{2})-(\d{2})[Tt ](\d{2}):(\d{2}):(\d{2})"
                       r"(?:\.(\d+))?(Z|z|[+-]\d{2}:?\d{2})$")
DURATION = re.compile(r"P(?:(\d+)D)?(?:T(?:(\d+)H)?(?:(\d+)M)?"
                      r"(?:(\d+)(?:\.(\d+))?S)?)?$")


def nanoseconds(text):
    """The instant as nanoseconds since the epoch, or None if unreadable."""
    m = TIMESTAMP.match(text)
    if not m:
        return None
    year, month, day, hour, minute, second, fraction, offset = m.groups()
    try:
        days = date(int(year), int(month), int(day)).toordinal() - EPOCH
    except ValueError:
        return None
    if offset in ("Z", "z"):
        shift = 0
    else:
        body = offset[1:].replace(":", "")
        shift = ((1 if offset[0] == "+" else -1)
                 * (int(body[:2]) * 3600 + int(body[2:]) * 60))
    seconds = days * 86400 + int(hour) * 3600 + int(minute) * 60 \
        + int(second) - shift
    return seconds * 10 ** 9 + int((fraction or "").ljust(9, "0")[:9])


def span(text):
    m = DURATION.fullmatch(text)
    if not m:
        return None
    days, hours, minutes, seconds, fraction = m.groups()
    whole = (int(days or 0) * 86400 + int(hours or 0) * 3600
             + int(minutes or 0) * 60 + int(seconds or 0))
    return whole * 10 ** 9 + int((fraction or "").ljust(9, "0")[:9])


def generate(rng):
    year = rng.choice([rng.randint(1, 9999), rng.randint(1960, 2040)])
    month = rng.randint(1, 12)
    day = rng.randint(1, calendar.monthrange(year, month)[1])
    fraction = rng.choice(["", ".5", ".123456789", ".000000001",
                           ".999999999", ".1", ".01", ".000000010"])
    offset = rng.choice(["Z", "+00:00", "+02:00", "-05:00", "+05:30",
                         "+14:00", "-12:00", "+0200", "+13:45"])
    return (f"{year:04d}-{month:02d}-{day:02d}T{rng.randint(0, 23):02d}:"
            f"{rng.randint(0, 59):02d}:{rng.randint(0, 59):02d}"
            f"{fraction}{offset}")


def duration_field(row):
    """The one field of ROW that is a duration rather than a timestamp.

    Which column it lands in depends on the arguments: -d0,1 appends,
    -d1,0 inserts after column 0. The claim being checked is about the
    span, not about where it is put, so it is found rather than indexed.
    """
    found = [field for field in row if DURATION.fullmatch(field)]
    return found[0] if len(found) == 1 else None


def either_order(rng, count):
    """`-d0,1` and `-d1,0` must measure the same span.

    The help says so outright -- "it comes out identical no matter which of
    the two column numbers you list first" -- because the two-column form is
    an elapsed time and not a signed difference. Nothing checked it, and
    nothing about the code makes it true by construction: each order takes a
    different column as the one to subtract from.
    """
    pairs = [(generate(rng), generate(rng)) for _ in range(count)]
    data = "".join(f"{a},{b}\n" for a, b in pairs).encode()
    runs = {}
    for args in ("-d0,1", "-d1,0"):
        done = subprocess.run([CSVDT, args], input=data, capture_output=True)
        if done.returncode != 0:
            return [f"{args}: csvdt failed: {done.stderr.decode()[:120]}"]
        runs[args] = [next(csv.reader([line]))
                      for line in done.stdout.decode().splitlines()]

    bad = []
    for index, (a, b) in enumerate(pairs):
        forwards = duration_field(runs["-d0,1"][index])
        backwards = duration_field(runs["-d1,0"][index])
        if forwards != backwards:
            bad.append(f"{a} {b}: -d0,1 says {forwards}, -d1,0 says "
                       f"{backwards}")
    return bad


def carried_forward(rng, count):
    """One column measures back to the previous row that *parsed*.

    The rule the help states, modelled here rather than borrowed: the first
    row that parses is PT0S, since it has nothing before it to measure
    against; a row that does not parse is parse_err and leaves the last good
    timestamp where it was, so the next good row measures across the gap
    rather than starting again from zero.
    """
    # No blank rows among the junk: the reader skips them by design, so one
    # would produce no output line and shift every comparison after it by
    # one. That is csv_roundtrip's business, not this check's -- and it is
    # what 455 of these divergences turned out to be the first time round.
    rows = [generate(rng) if rng.random() < 0.7 else
            rng.choice(["junk", "not-a-time", "Jan  1 00:00:00",
                        "2024-13-01T00:00:00Z", "2024-01-32T00:00:00Z"])
            for _ in range(count)]
    data = "".join(f"{row}\n" for row in rows).encode()
    done = subprocess.run([CSVDT, "-d0"], input=data, capture_output=True)
    if done.returncode not in (0, 1):
        return [f"-d0: csvdt failed: {done.stderr.decode()[:120]}"]
    got = [next(csv.reader([line]))[-1]
           for line in done.stdout.decode().splitlines()]

    bad, previous = [], None
    for index, row in enumerate(rows):
        now = nanoseconds(row)
        if now is None:
            want = "parse_err"
        elif previous is None:
            want = None            # the first that parses: PT0S, unmeasured
        else:
            want = abs(now - previous)
        if now is not None:
            previous = now
        if index >= len(got):
            bad.append(f"row {index}: no output line")
            continue
        if want == "parse_err":
            if got[index] != "parse_err":
                bad.append(f"row {index} {row!r}: {got[index]!r}, want parse_err")
        elif want is None:
            if got[index] != "PT0S":
                bad.append(f"row {index} {row!r}: {got[index]!r}, want PT0S "
                           f"(nothing before it)")
        else:
            measured = span(got[index])
            if measured != want:
                bad.append(f"row {index} {row!r}: {got[index]} is {measured} "
                           f"ns, want {want} across the gap")
    return bad


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    count = int(sys.argv[2]) if len(sys.argv) > 2 else 500
    rng = random.Random(seed)
    pairs = [(generate(rng), generate(rng)) for _ in range(count)]
    data = "".join(f"{a},{b}\n" for a, b in pairs).encode()
    p = subprocess.run([CSVDT, "-d0,1"], input=data, capture_output=True)
    if p.returncode != 0:
        print(f"exact durations: csvdt failed: {p.stderr.decode()[:200]}")
        return 1

    bad = []
    for (a, b), line in zip(pairs, p.stdout.decode().splitlines()):
        row = next(csv.reader([line]))
        first, second = nanoseconds(a), nanoseconds(b)
        if first is None or second is None:
            continue
        if row[-1] == "parse_err":
            bad.append(f"{a} {b}: parse_err, both readable")
            continue
        got, want = span(row[-1]), abs(second - first)
        if got is None:
            bad.append(f"{a} {b}: {row[-1]!r} is not a duration")
        elif got != want:
            bad.append(f"{a} {b}: {row[-1]} is {got} ns, want {want} "
                       f"(off by {got - want})")
    order = either_order(rng, count)
    carry = carried_forward(rng, count)
    print(f"exact durations: seed {seed}, {count} pairs, {len(bad)} "
          f"divergences; either order, {len(order)}; carried forward, "
          f"{len(carry)}")
    for line in (bad + order + carry)[:8]:
        print(f"  {line}")
    return 1 if bad or order or carry else 0


if __name__ == "__main__":
    sys.exit(main())
