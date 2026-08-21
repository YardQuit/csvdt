"""Every option that combines into a run, against every other.

Each option is well covered on its own. This asks what happens when they
meet: nothing may panic, a run that succeeds must write CSV that parses, and
a filling mode must produce rows that really are rectangular -- header
included. Deliberately cheap invariants, because the point is breadth.

Not every option, despite what this used to say. The modes that answer and
exit rather than converting -- --peek, --list-options, --generate-man,
--help, --version -- write something that is not CSV, so the invariants
below say nothing about them and the groups do not draw them. Each has a
check of its own: peek.py and list_options.py here, the page in the rpm
%check and in tests/cli.rs. Said plainly because the old wording claimed
this file covered them, and --peek went un-checked anywhere for as long as
that sentence was believed.

Statuses 0, 1, 2 and 100 are the documented four; anything else is a finding
in itself. 101 is a Rust panic and never an answer.
"""
import csv, io, os, random, subprocess, sys

CSVDT = os.environ.get("CSVDT", "target/release/csvdt")

INPUTS = {
    "plain": "a,b,c\n2024-06-15T14:00:00Z,1700000000,x\n"
             "2024-06-16T15:30:00+02:00,1700086400,y\n",
    "ragged": "a,b,c\n2024-06-15T14:00:00Z,1700000000\n"
              "2024-06-16T15:30:00+02:00,1700086400,y,extra\n",
    "short first": "a,b\n2024-06-15T14:00:00Z,1700000000,x\n",
    "quoted": 'a,b,c\n"2024-06-15T14:00:00Z","1,700",x\n"q""q",2,"y\nz"\n',
    "comments": "#lead\na,b,c\n2024-06-15T14:00:00Z,1700000000,x\n#tail\n",
    "unparsable": "a,b,c\nnope,notanum,x\n2024-06-15T14:00:00Z,1700000000,y\n",
    "empty fields": "a,b,c\n,,\n2024-06-15T14:00:00Z,,x\n",
}

HEADER = [[], ["-H"], ["-H", "-p"]]
FLEXIBLE = [[], ["-f"], ["-f=fix"], ["-f=widest"]]
TRIM = [[], ["--trim", "all"], ["--trim", "fields"], ["--trim", "headers"]]
ACTION = [[], ["-u0"], ["-l0"], ["-o0,+05:30"], ["-s0"], ["-r1"],
          ["-d0"], ["-d0,0"], ["-r1", "--round-seconds"]]
INSERT = [[], ["-i0,before"], ["-i1,after"], ["-i0,replace"],
          ["-i9,replace"], ["-i-1,replace"]]
SHAPE = [[], ["--remove", "1"], ["-a", "2,0"], ["--remove", "0"],
         ["-a", "1"], ["--remove", "0,1"]]
OTHER = [[], ["--comment", "#"], ["--single-quote"],
         ["--read-delimiter", ";"], ["--quote", "always"]]


def parses(text):
    try:
        list(csv.reader(io.StringIO(text, newline="")))
        return True
    except Exception:
        return False


def rectangular(text):
    rows = [r for r in csv.reader(io.StringIO(text, newline="")) if r != []]
    return len({len(r) for r in rows}) <= 1


def main():
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    rounds = int(sys.argv[2]) if len(sys.argv) > 2 else 1500
    rng = random.Random(seed)
    findings, seen = [], set()

    for _ in range(rounds):
        args = (rng.choice(HEADER) + rng.choice(FLEXIBLE) + rng.choice(TRIM)
                + rng.choice(ACTION) + rng.choice(INSERT) + rng.choice(SHAPE)
                + rng.choice(OTHER))
        name = rng.choice(list(INPUTS))
        try:
            p = subprocess.run([CSVDT] + args, input=INPUTS[name].encode(),
                               capture_output=True, timeout=20)
        except subprocess.TimeoutExpired:
            findings.append(("TIMEOUT", args, name, ""))
            continue
        out = p.stdout.decode("utf-8", "replace")
        err = p.stderr.decode("utf-8", "replace")

        def note(kind, detail=""):
            key = (kind, tuple(args), name)
            if key not in seen:
                seen.add(key)
                findings.append((kind, args, name, detail))

        if p.returncode == 101 or "panicked" in err:
            note("PANIC", err.strip()[:200])
            continue
        if p.returncode not in (0, 1, 2, 100):
            note(f"UNDOCUMENTED STATUS {p.returncode}", err.strip()[:100])
        if p.returncode == 0 and out and not parses(out):
            note("UNPARSABLE OUTPUT", out[:100])
        if (p.returncode == 0 and out
                and ("-f=fix" in args or "-f=widest" in args)
                and "--read-delimiter" not in args
                and not rectangular(out)):
            widths = sorted({len(r) for r in csv.reader(io.StringIO(out))
                             if r != []})
            note("NOT RECTANGULAR", f"widths {widths}")

    print(f"option matrix: seed {seed}, {rounds} runs, {len(findings)} findings")
    for kind, args, name, detail in findings[:10]:
        print(f"  [{kind}] {' '.join(args)}  <{name}>")
        if detail:
            print(f"      {detail}")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
