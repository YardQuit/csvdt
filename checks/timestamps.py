"""Every conversion must name the same instant another library reads.

Instants, not text: how csvdt chooses to write a timestamp is its own
business, but the moment it names is not. So the input is parsed by Python,
the output is parsed by Python, and the two must be the same point in time.
That leaves formatting free to differ and holds the meaning exactly, which
is the only comparison worth making across two implementations.

-d is checked separately in durations.py, where Python's timedelta is too
coarse to referee and the arithmetic has to be done in integers.
"""
import calendar, csv, io, os, random, subprocess, sys
from datetime import datetime, timedelta, timezone

CSVDT = os.environ.get("CSVDT", "target/release/csvdt")

# -12:00 to +14:00 is what real zones use and what -o will write; a file may
# carry anything up to a whole day either way, and reading one is exact
# arithmetic, so those are here too.
OFFSETS = ["Z", "z", "+00:00", "-00:00", "+02:00", "-05:00", "+05:30",
           "+14:00", "-12:00", "+0200", "-0530", "+13:45", "-09:30",
           "+15:00", "+23:59", "-23:59", "-18:00"]


def reference(text):
    try:
        return datetime.fromisoformat(text)
    except ValueError:
        return None


def representable_in_utc(when):
    """Whether the instant can be written as RFC3339 once moved to UTC.

    A conversion can cross the year boundary without moving the instant:
    0001-01-01T00:00:00+05:30 is year 0000 in UTC and 9999-12-31T23:59:59-05:00
    is year 10000, and neither is a year RFC3339's four digits can hold. csvdt
    writes parse_err there rather than ISO 8601's expanded form, which is a
    different format wearing the same shape.

    Python's own datetime stops at the same two ends, so it raises here on
    exactly the values csvdt refuses -- which is what makes it a reference for
    this and not merely a coincidence.
    """
    try:
        when.astimezone(timezone.utc)
    except (OverflowError, OSError, ValueError):
        return False
    return True


def column(args, values):
    """Run one value per row and return the produced column."""
    data = "".join(f"x,{v}\n" for v in values).encode()
    p = subprocess.run([CSVDT] + args, input=data, capture_output=True)
    if p.returncode not in (0, 1):
        return None
    return [next(csv.reader([line]))
            for line in p.stdout.decode().splitlines()]


def generate(rng):
    year = rng.choice([rng.randint(1, 9999), rng.randint(1970, 2035),
                       1, 9999, 1970, 2000, 1900])
    month = rng.randint(1, 12)
    day = rng.randint(1, calendar.monthrange(year, month)[1])
    fraction = rng.choice(["", "", "", ".5", ".500", ".123", ".999999",
                           ".123456789", ".0", ".000000001"])
    return (f"{year:04d}-{month:02d}-{day:02d}"
            # 'T' mostly, but the two forms the help says are also read: a
            # lower-case 't', and the space RFC3339 allows for readability.
            f"{rng.choice(['T'] * 8 + ['t', ' '])}"
            f"{rng.randint(0, 23):02d}:{rng.randint(0, 59):02d}:"
            f"{rng.randint(0, 59):02d}{fraction}{rng.choice(OFFSETS)}")


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    count = int(sys.argv[2]) if len(sys.argv) > 2 else 300
    rng = random.Random(seed)
    values = [generate(rng) for _ in range(count)]
    bad = []

    def note(kind, source, produced, why):
        bad.append(f"[{kind}] {source!r} -> {produced!r}: {why}")

    # -u: the instant must survive, and the offset must be +00:00.
    for source, row in zip(values, column(["-u1"], values) or []):
        want = reference(source)
        if want is None:
            continue
        if row[2] == "parse_err":
            # Not every value Python reads has a UTC form RFC3339 can write.
            if representable_in_utc(want):
                note("-u", source, "parse_err", "Python reads it")
        elif (got := reference(row[2])) is None:
            note("-u", source, row[2], "output unreadable")
        elif got != want:
            note("-u", source, row[2], f"instant moved: {want} != {got}")
        elif got.utcoffset() != timedelta(0):
            note("-u", source, row[2], "not UTC")

    # -o: the instant must survive a move to a named offset.
    for offset in ["+05:30", "-06:00", "Z", "+14:00", "-12:00"]:
        for source, row in zip(values, column([f"-o1,{offset}"], values) or []):
            want = reference(source)
            if want is None or row[2] == "parse_err":
                continue
            got = reference(row[2])
            if got is None:
                note(f"-o{offset}", source, row[2], "output unreadable")
            elif got != want:
                note(f"-o{offset}", source, row[2], f"instant moved: {want} != {got}")

    # -s: date and time-of-day, both read from the record's own offset.
    for source, row in zip(values, column(["-s1"], values) or []):
        want = reference(source)
        if want is None or row[2] == "parse_err":
            continue
        date = f"{want.year:04d}-{want.month:02d}-{want.day:02d}"
        clock = f"{want.hour:02d}:{want.minute:02d}:{want.second:02d}"
        if row[2] != date:
            note("-s", source, row[2], f"date != {date}")
        if row[3].split(" ")[0].split(".")[0] != clock:
            note("-s", source, row[3], f"time != {clock}")

    # -r: Unix epoch to RFC3339, across the documented range.
    epochs = [rng.randint(-62135596800, 253402300799) for _ in range(count)]
    epochs += [0, -1, 1, -62135596800, 253402300799]
    for epoch, row in zip(epochs, column(["-r1"], [str(e) for e in epochs]) or []):
        want = datetime(1970, 1, 1, tzinfo=timezone.utc) + timedelta(seconds=epoch)
        if row[2] == "parse_err":
            note("-r", str(epoch), "parse_err", "inside the documented range")
        elif reference(row[2]) != want:
            note("-r", str(epoch), row[2], f"!= {want}")

    print(f"timestamp instants: seed {seed}, {count} values, {len(bad)} divergences")
    for line in bad[:8]:
        print(f"  {line}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
