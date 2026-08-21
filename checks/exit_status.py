"""The exit status against what the run actually did.

EXIT STATUS is a contract a script keys on, and the help says several of the
KNOWN LIMITS turn on it: a run that converted nothing is told from one that
worked by the status alone. option_matrix.py asks only that the status is one
of the documented four, which a build returning 0 for everything would pass.
This asks whether it is the right one of the four.

The model is the run's own output. Where an action was given, it writes a
result column, and a value it could not read is the literal parse_err. So the
status follows from counting them:

  some value converted           -> 0
  values were there, none converted -> 1
  no values at all               -> 0, since nothing failed

That last is the case the wording of 1 reads past. A header with no data rows
under it converted nothing, but no value was written as parse_err either, and
an empty input is not an error anywhere else in this program.

Also here: the boundary between 1 and 100, which is not about how bad the
trouble is but about when it was found. 100 is refused before the input is
opened -- nothing read, nothing written. 1 is found by doing the work, reading
or writing either one. A bad TZ was documented under 1 and has always exited
100; it is refused before the file is opened, so 100 is where it belongs and
the help now says so.

And the writing half of that, which the wording of 1 left out for a long time:
a run whose output cannot be written reports 1, and so does every answer csvdt
gives about itself. /dev/full is the way to ask, so the section is skipped
where there is no such device rather than passing on a machine that could not
have run it.
"""
import csv, io, os, random, subprocess, sys

CSVDT = os.environ.get("CSVDT", "target/release/csvdt")

GOOD = ["2024-06-15T14:00:00Z", "2024-06-15T14:00:00+02:00",
        "1999-01-01T00:00:00-05:00", "2016-12-31T23:59:60Z"]
BAD = ["nope", "", "2024-13-45T99:99:99Z", "not a time", "12345x",
       "0000-01-01T00:00:00Z", "2024-06-15"]
# Epoch seconds for -r, which reads a different format entirely.
GOOD_EPOCH = ["0", "1700000000", "-62135596800"]
BAD_EPOCH = ["nope", "", "1.5", "99999999999999999999"]

ACTIONS = [
    (["-u1"], GOOD, BAD),
    (["-l1"], GOOD, BAD),
    (["-o1,+05:30"], GOOD, BAD),
    (["-s1"], GOOD, BAD),
    (["-r1"], GOOD_EPOCH, BAD_EPOCH),
]


def run(args, text, env=None):
    environment = dict(os.environ)
    environment.setdefault("TZ", "UTC")
    if env:
        environment.update(env)
    p = subprocess.run([CSVDT] + args, input=text.encode(),
                       capture_output=True, timeout=20, env=environment)
    return (p.returncode, p.stdout.decode("utf-8", "replace"),
            p.stderr.decode("utf-8", "replace"))


def converted(out, before):
    """How many result values are real, and how many are parse_err.

    The action's column is the one added; everything to its left came from
    the file. BEFORE is how many columns the input had, so the result sits
    at that index onward -- -s writes two, the rest one.
    """
    rows = [r for r in csv.reader(io.StringIO(out, newline="")) if r != []]
    if not rows:
        return 0, 0
    good = bad = 0
    for row in rows[1:] if rows else []:
        for value in row[before:]:
            if value == "parse_err":
                bad += 1
            elif value != "":
                good += 1
    return good, bad


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    rounds = int(sys.argv[2]) if len(sys.argv) > 2 else 300
    rng = random.Random(seed)
    findings = []

    for _ in range(rounds):
        action, good, bad = rng.choice(ACTIONS)
        # Deliberately weighted to the boundary: all-good and all-bad are
        # where 0 and 1 change places, and one row of each is the thinnest
        # margin either answer has.
        rows = rng.randint(0, 4)
        values = [rng.choice(good if rng.random() < 0.5 else bad)
                  for _ in range(rows)]
        text = "id,when\n" + "".join(
            f"{i}," + (v if "," not in v else f'"{v}"') + "\n"
            for i, v in enumerate(values))

        status, out, err = run(["-H", "-p"] + action, text)
        if status == 101 or "panicked" in err:
            findings.append(("PANIC", action, text, err[:80]))
            continue
        if status not in (0, 1, 2, 100):
            findings.append((f"UNDOCUMENTED STATUS {status}", action, text,
                             err[:80]))
            continue
        if status in (2, 100):
            findings.append(("REFUSED A RUN IT SHOULD HAVE MADE", action,
                             text, err[:80]))
            continue

        real, failed = converted(out, 2)
        if real and status != 0:
            findings.append(("CONVERTED BUT NOT 0", action, text,
                             f"{real} converted, exit {status}"))
        elif not real and failed and status != 1:
            findings.append(("CONVERTED NOTHING BUT NOT 1", action, text,
                             f"{failed} parse_err, exit {status}"))
        elif not real and not failed and status != 0:
            findings.append(("NO VALUES BUT NOT 0", action, text,
                             f"exit {status} over {rows} rows"))

    # The 1/100 boundary is about when the trouble was found, not how bad it
    # is. Each of these is refused before the input is opened, so nothing is
    # read and nothing written; each of those is found by reading.
    BEFORE_READING = [
        (["-o0,+99:00"], None),
        (["--read-delimiter", ",,"], None),
        (["-a", "1,1"], None),
        (["--remove", "0,0"], None),
        (["-l1"], {"TZ": "No/Such_Zone"}),
        (["-l1"], {"TZ": "Mars/Olympus"}),
    ]
    WHILE_READING = [
        (["-u9"], None),
        (["-u1"], None),
    ]
    boundary = 0
    for args, env in BEFORE_READING:
        boundary += 1
        status, out, err = run(["-H", "-p"] + args, "id,when\n0,nope\n", env)
        if status != 100:
            findings.append(("REFUSED BEFORE READING BUT NOT 100", args,
                             str(env), f"exit {status}"))
        if out:
            findings.append(("WROTE OUTPUT IT SAID IT HAD NOT", args,
                             str(env), out[:60]))
    for args, env in WHILE_READING:
        boundary += 1
        status, _, err = run(["-H", "-p"] + args, "id,when\n0,nope\n", env)
        if status != 1:
            findings.append(("FOUND BY READING BUT NOT 1", args, str(env),
                             f"exit {status}"))

    # The other way to reach 1, and the one the wording of it left out for a
    # long time: the work was done and could not be written. It holds for what
    # csvdt says about itself as much as for a conversion -- --help and
    # --version are printed by clap rather than by csvdt, and reported 0 over
    # output nobody received until they were routed through csvdt's own writer.
    unwritable = 0
    if os.path.exists("/dev/full"):
        for args in (["-H", "-p", "-r0"], ["--help"], ["-h"], ["--version"],
                     ["--list-options"], ["--generate-man"]):
            unwritable += 1
            with open("/dev/full", "wb") as full:
                done = subprocess.run(
                    [CSVDT] + args, input=b"a\n1700000000\n",
                    stdout=full, stderr=subprocess.PIPE, timeout=20)
            if done.returncode != 1:
                findings.append(("OUTPUT LOST BUT NOT 1", args, "",
                                 f"exit {done.returncode}"))
            elif not done.stderr.startswith(b"csvdt: No space left"):
                findings.append(("OUTPUT LOST AND NOT SAID SO", args, "",
                                 done.stderr.decode("utf-8", "replace")[:60]))

    print(f"exit status: seed {seed}, {rounds} runs, {boundary} boundary "
          f"cases and {unwritable} onto a full device, {len(findings)} findings")
    for kind, args, text, detail in findings[:10]:
        print(f"  [{kind}] {' '.join(args)}")
        print(f"      {detail}")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
