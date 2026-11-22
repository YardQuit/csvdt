"""What -l/--local produces, which nothing else here asks about.

Every other conversion is held against Python: -r, -u, -o and -s in
timestamps.py, -d in durations.py. -l appeared only in exit_status.py, which
asks whether it fails correctly, and in option_matrix.py, which asks whether
it runs. What it writes had no model behind it, and it is the conversion the
help calls the difficult one, the only one whose answer depends on more than
the record.

Three properties, held differently, because they do not rest on the same
things.

Held everywhere, at any date:

  the instant does not move.  -l re-expresses a moment in some zone's clock;
  the moment is the record's and no database has an opinion about it.  Read
  the output back and it must name the instant the input named.

  no offset carries a seconds part.  RFC3339 writes +HH:MM and has nowhere to
  put seconds, so where a zone's offset is not whole minutes the help says the
  result is parse_err rather than a timestamp naming a different instant.  That
  the written ones are all whole minutes can be checked without asking anybody
  what the offset should have been.

Held where the two databases agree, which is modern dates:

  the clock reading matches Python's.  This is the part with a caveat, and the
  caveat is the point of the window.  csvdt carries its own copy of the IANA
  data (the help says so, and says why) and a host carries another.  For
  recent decades they agree; before standard time they do not, and not
  narrowly.  Measured here: chrono-tz 2025b has Europe/Amsterdam on +00:17:30
  until 1892 and +00:00 after it, where this host's tzdata has +00:19:32
  through 1937.  A differential run over those years would report twenty
  minutes of divergence a hundred times over, and none of it would be a fault
  in csvdt.  So the comparison runs from FIRST_COMPARABLE_YEAR, and the older
  dates are still exercised: by the two properties above, which no database
  can disagree with.

Held unless the two releases differ:

  a clock reading that differs only because the host's tzdata and the copy
  inside csvdt are different releases.  A zone's rules change between
  releases, including rules for dates still to come (this host's 2026f
  disagrees with the bundled 2025b about America/Inuvik in 2031), and no
  amount of care in csvdt can make it agree with a newer or older database.
  Where the two releases are known and differ, an offset the host disagrees
  with is reported as a difference between releases, naming both, and does
  not fail the check.  Where they are the same release, or either is not
  known, it is a divergence.  The other two properties, and the minutes of a
  clock reading given the offset, are never excused this way.

Reported rather than held:

  which copy of the data answered.  The help promises the rules come "from the
  IANA data compiled into this binary rather than the machine's", and the run
  above cannot test that: inside the window the two agree, so a csvdt
  reading the host's files passes every line of it, and so would one that
  swapped the named zone for `chrono::Local', which honours TZ and the host's
  files.  Europe/Amsterdam in 1900 can tell them apart on this host, and the
  summary says which answered.  It can confirm the binary's own data and never
  quite the opposite: where both copies would refuse the instant, or both give
  the same offset at that one instant, the answer fits either story, whatever
  the two releases say elsewhere.  So it is printed, and a run
  that stops saying "the binary's" is the thing to look at.

The zones are the ones both sides know, since csvdt refuses a zone its own
data lacks and Python cannot referee one it lacks either.
"""
import os, random, re, sys
from datetime import datetime, timedelta, timezone
from zoneinfo import TZPATH, ZoneInfo, available_timezones

import harness


# Where the bundled data and a host's can be relied on to say the same thing.
# The disagreements are in the era before standard time, when an offset was a
# town's longitude and the databases record different guesses at it.
FIRST_COMPARABLE_YEAR = 1970

# How many differences between releases are listed in full.
RELEASE_DIFFERENCES_SHOWN = 5

# Zones worth naming rather than drawing: a southern-hemisphere DST, the
# half-hour and three-quarter-hour offsets, one that abolished DST recently,
# one that never had it, and the two the help names for their sub-minute
# history.
NAMED = ["Europe/Oslo", "America/New_York", "Australia/Sydney", "Asia/Kolkata",
         "Pacific/Chatham", "Asia/Tokyo", "Europe/Moscow", "America/Sao_Paulo",
         "Africa/Monrovia", "Europe/Amsterdam", "Pacific/Kiritimati", "UTC"]


def host_tzdata_release():
    """The release of the tzdata this Python reads, such as "2026f", or None.

    Taken from the `tzdata.zi` file (or `+VERSION`) in the first directory of
    `zoneinfo.TZPATH` that has one, else from the `tzdata` package.  None
    where the host does not say, which the caller treats as unknown.
    """
    for directory in TZPATH:
        try:
            with open(os.path.join(directory, "tzdata.zi")) as handle:
                found = re.match(r"# version (\S+)", handle.readline())
            if found:
                return found.group(1)
        except OSError:
            pass
        try:
            with open(os.path.join(directory, "+VERSION")) as handle:
                return handle.read().strip() or None
        except OSError:
            pass
    try:
        import tzdata
        return tzdata.IANA_VERSION
    except (ImportError, AttributeError):
        return None


def bundled_tzdata_release():
    """The release of the tzdata inside the binary, from its `--version`."""
    found = re.search(r"IANA tzdata (\S+?)\)",
                      harness.text(harness.run(["--version"]).stdout))
    return found.group(1) if found else None


def releases_differ(host, bundled):
    """Whether the two tzdata releases are both known and not the same."""
    return bool(host and bundled and host != bundled)


def zones_both_sides_know():
    """The zones this Python and this csvdt can both speak for."""
    mine = available_timezones()
    known = []
    for zone in sorted(mine):
        if zone in NAMED:
            continue
        known.append(zone)
    return known


def run(rows, zone):
    """Convert column 0 in ZONE, and give back what came out of it."""
    body = "when\n" + "".join(row + "\n" for row in rows)
    environment = dict(os.environ, TZ=zone)
    done = harness.run(["-H", "-p", "-l0"], body, env=environment, timeout=120)
    # Exit 1 where nothing converted is a documented answer, not a failure to
    # read: a row whose zone has a sub-minute offset is written parse_err, and
    # a file of nothing else converted nothing. The output is still the
    # output, so what decides here is whether the rows came back.
    out = done.stdout.decode().splitlines()[1:]
    if len(out) != len(rows):
        return None
    return [line.split(",")[1] if "," in line else line for line in out]


def transitions(zone, rng, how_many):
    """Instants either side of the zone's DST changes, and some drawn ones.

    The changes are where an implementation earns its keep: an hour that
    happens twice, an hour that never happens, and the minute on each side of
    both.  Found by walking the offset rather than by asking the database for
    its transition list, which zoneinfo does not offer.
    """
    picked = []
    here = ZoneInfo(zone)
    year = rng.randint(FIRST_COMPARABLE_YEAR, 2035)
    moment = datetime(year, 1, 1, tzinfo=timezone.utc)
    was = moment.astimezone(here).utcoffset()
    for _ in range(366 * 2):
        moment += timedelta(hours=12)
        now = moment.astimezone(here).utcoffset()
        if now != was:
            # The half-day the change fell inside, from both sides and close.
            for delta in (-timedelta(hours=13), -timedelta(minutes=1),
                          timedelta(0), timedelta(minutes=1)):
                picked.append(moment + delta)
            was = now
        if len(picked) >= how_many:
            break
    while len(picked) < how_many:
        picked.append(datetime(rng.randint(FIRST_COMPARABLE_YEAR, 2035),
                               rng.randint(1, 12), rng.randint(1, 28),
                               rng.randint(0, 23), rng.randint(0, 59),
                               tzinfo=timezone.utc))
    return picked[:how_many]


def old_instants(rng, how_many):
    """Dates before the window, for the properties no database can dispute."""
    return [datetime(rng.randint(1, 1969), rng.randint(1, 12),
                     rng.randint(1, 28), rng.randint(0, 23), rng.randint(0, 59),
                     tzinfo=timezone.utc)
            for _ in range(how_many)]


def offset_is_whole_minutes(text):
    """Whether a written timestamp's offset has no seconds part.

    True of anything RFC3339 can express at all: the format stops at minutes.
    So this is really asking whether csvdt wrote something outside the format,
    which is the failure the whole-minute rule exists to prevent.
    """
    tail = text[-6:]
    return (len(text) >= 6 and tail[0] in "+-" and tail[3] == ":") or \
        text.endswith("Z")


def which_database(bad):
    """Whether the rules came from the binary, as the help says they do.

    "The rules, from the IANA data compiled into this binary rather than the
    machine's" is a promise about where an answer comes from, and the run
    above cannot test it: inside the comparison window the two databases
    agree, so a csvdt reading the host's data would pass every line of it.
    A csvdt that replaced the named zone with `chrono::Local' (which honours
    TZ and the host's files) would pass it.

    Where they disagree, the disagreement is the instrument.  An older host's
    tzdata had Europe/Amsterdam on +00:19:32 in 1900; chrono-tz 2025b has it
    on +00:00.  So csvdt writing the host's answer there is csvdt reading the
    host's database.

    Nothing to be done on a host whose tzdata happens to agree with the
    bundled copy at this instant: there is no difference to see, and saying
    so is better than a check that quietly tests nothing.  That says nothing
    about other zones or dates, where the releases can still differ.
    """
    zone, when = "Europe/Amsterdam", datetime(1900, 6, 15, 12, tzinfo=timezone.utc)
    host = when.astimezone(ZoneInfo(zone)).utcoffset()
    sub_minute = host.total_seconds() % 60 != 0
    produced = run([when.isoformat().replace("+00:00", "Z")], zone)
    if not produced:
        bad.append((zone, when.isoformat(), "-", "the run failed"))
        return "could not be asked"
    wrote = produced[0]

    if wrote == "parse_err":
        # A sub-minute offset in whichever copy answered. If the host's is
        # sub-minute too, both stories fit and this instant settles nothing.
        return ("the machine's" if not sub_minute
                else "not distinguishable here (both copies refuse this one)")

    try:
        back = datetime.fromisoformat(wrote)
    except ValueError:
        bad.append((zone, when.isoformat(), wrote, "unreadable"))
        return "could not be asked"

    if sub_minute:
        # The host would have had to refuse it, and this did not.
        return "the binary's"
    if back.utcoffset() != host:
        return "the binary's"
    return ("not distinguishable here (at the one instant tried, this host's "
            "tzdata gives the same answer as the bundled copy)")


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    rng = random.Random(seed)

    drawn = zones_both_sides_know()
    rng.shuffle(drawn)
    zones = NAMED + drawn[:28]
    zones = [zone for zone in zones if zone in available_timezones()]

    host_release, bundled_release = host_tzdata_release(), bundled_tzdata_release()
    excused = releases_differ(host_release, bundled_release)
    bad, releases = [], []
    compared = moved = old = refused = 0

    for zone in zones:
        here = ZoneInfo(zone)
        modern = transitions(zone, rng, 12)
        ancient = old_instants(rng, 4)
        moments = modern + ancient
        rows = [when.isoformat().replace("+00:00", "Z") for when in moments]
        produced = run(rows, zone)
        if produced is None:
            bad.append((zone, "-", "the run failed", ""))
            continue

        for when, wrote in zip(moments, produced):
            if wrote == "parse_err":
                # Refused, which the help says happens where the zone's offset
                # is not whole minutes.  Counted rather than judged: what the
                # offset was is the database's word, and before the window the
                # two databases do not share one.
                refused += 1
                continue

            if not offset_is_whole_minutes(wrote):
                bad.append((zone, rows[moments.index(when)], wrote,
                            "offset is not whole minutes, which RFC3339 "
                            "cannot express"))
                continue

            try:
                back = datetime.fromisoformat(wrote)
            except ValueError:
                bad.append((zone, when.isoformat(), wrote, "unreadable"))
                continue

            # Held at any date: the moment is the record's, not a database's.
            if back != when:
                bad.append((zone, when.isoformat(), wrote,
                            f"instant moved, {back.isoformat()} != "
                            f"{when.isoformat()}"))
                continue
            moved += 1

            if when.year < FIRST_COMPARABLE_YEAR:
                old += 1
                continue

            # And in the window, the clock reading itself.
            want = when.astimezone(here)
            if back.utcoffset() != want.utcoffset():
                found = (zone, when.isoformat(), wrote,
                         f"offset {back.utcoffset()} where this host's "
                         f"tzdata says {want.utcoffset()}")
                (releases if excused else bad).append(found)
                continue
            if back.replace(tzinfo=None) != want.replace(tzinfo=None):
                bad.append((zone, when.isoformat(), wrote,
                            f"clock reads {back.replace(tzinfo=None)} where "
                            f"Python reads {want.replace(tzinfo=None)}"))
                continue
            compared += 1

    source = which_database(bad)

    print(f"local: seed {seed}, {len(zones)} zones, {moved} instants preserved "
          f"({old} of them before {FIRST_COMPARABLE_YEAR}), {compared} clock "
          f"readings against Python, {refused} refused for a sub-minute "
          f"offset, {len(bad)} divergences; rules read from {source}")
    if releases:
        print(f"local: {len(releases)} offsets differ because the releases do, "
              f"not because of csvdt: this host has tzdata {host_release}, "
              f"csvdt bundles {bundled_release}")
        for zone, source, wrote, why in releases[:RELEASE_DIFFERENCES_SHOWN]:
            print(f"   {zone} {source} -> {wrote}: {why}")
    for zone, source, wrote, why in bad[:20]:
        print(f"   {zone} {source} -> {wrote}: {why}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
