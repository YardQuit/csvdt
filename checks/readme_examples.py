"""Does the README print what it says csvdt prints?

Every worked example in README.md is a claim about output, and the ones that
need nothing but a pipe can simply be run. That is what this does: parse the
fenced blocks, find the `$ ` commands, and compare what comes back against
the lines shown under them.

Four had drifted when this was written. Two showed a message without the
`csvdt: ` prefix every message carries -- the prefix was added after the
README was, so both had been right once. One showed a --version string no
build has ever printed: the program version of the source paired with the
build metadata of a published binary that reports a different version. The
Emacs README's two are checked here too, since they are the same kind of
claim about the same binary.

Examples reading a file the block does not create are skipped and counted,
rather than passed over quietly: a green line here covers what it ran.
"""
import os, re, subprocess, sys

CSVDT = os.environ.get("CSVDT", "target/release/csvdt")
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# A file named by an example that the example does not itself write. These
# need a fixture whose contents the README does not give, so running them
# would check an invention rather than the claim.
NEEDS_A_FILE = re.compile(r"\b\w+\.csv\b")
# Anything reaching the network, or a hash of output whose input is not here.
NEEDS_THE_WORLD = re.compile(r"\b(curl|md5sum|sha256sum|sudo|dnf|cargo|iconv)\b")
# What a build says about itself is the build's, not the README's: the second
# --version example is a published binary's string, and this is not it.
NEEDS_THAT_BUILD = re.compile(r"--version")
# A line of its own saying the README left something out. The lines around it
# must still appear, in order -- an excerpt is a claim about what is there,
# just not about what is not.
ELISION = "..." 


def blocks(path):
    """Every ```fenced``` block of a markdown file, with its line number."""
    text = open(path, encoding="utf-8").read()
    for match in re.finditer(r"```\w*\n(.*?)```", text, re.S):
        yield text[:match.start()].count("\n") + 1, match.group(1)


def examples(path):
    """Each `$ command` in PATH, with the lines shown as its output.

    A trailing backslash continues the command, as it does in a shell.
    """
    for line_number, body in blocks(path):
        lines = body.split("\n")
        index = 0
        while index < len(lines):
            line = lines[index]
            if not line.startswith("$ "):
                index += 1
                continue
            start, command = index, line[2:]
            while command.endswith("\\") and index + 1 < len(lines):
                index += 1
                command = command[:-1].rstrip() + " " + lines[index].strip()
            shown, index = [], index + 1
            while index < len(lines) and not lines[index].startswith("$ "):
                shown.append(lines[index])
                index += 1
            while shown and not shown[-1].strip():
                shown.pop()
            yield f"{os.path.basename(path)}:{line_number + start}", command, shown


def wanted(shown):
    """The output lines, without the notes the README writes beside them.

    Two or more spaces before a '#' is a margin note -- '3,4' and
    '"#x",2                # quoted, so a second read keeps the row'.
    A single space is not, since a field may end in one.
    """
    return [re.sub(r"\s{2,}#.*$", "", line).rstrip() for line in shown]


def matches(got, expected):
    """Whether GOT is what EXPECTED describes.

    Without an elision that is equality. With one, every shown line must
    still be there and in the order shown, with the elisions standing for
    whatever the README left out.
    """
    if ELISION not in expected:
        return got == expected
    remaining = list(got)
    for line in expected:
        if line == ELISION:
            continue
        if line not in remaining:
            return False
        remaining = remaining[remaining.index(line) + 1:]
    return True


# The transcripts shown without a command in front of them, with what
# produces each. A message quoted in prose is the same claim as one under a
# `$ `, and drifts the same way -- all three that had drifted were of this
# kind, so a check that could only run the commands would have found none of
# them. Written the long way round on purpose: the README line must still be
# in the README, and the binary must still produce it.
MESSAGE_EXAMPLES = [
    # (file, the README's line, argv, stdin, a file to write first)
    ("README.md",
     "csvdt: CSV parse error: record 1 (line 2, field: 1, byte: 8): "
     "invalid utf-8: invalid UTF-8 in field 1 near byte index 3",
     ["cp1252.csv"], None,
     # Sized so the numbers the README's prose explains are these numbers:
     # record 1 begins at byte 8, and the first bad byte is at offset 32,
     # three bytes into field 1.
     ("cp1252.csv", b"name,tx\n" + b"a" * 20 + b"," + b"abc\xe9def\n")),
    ("README.md",
     "csvdt: nothing converted: all 2 values were written as parse_err. "
     "Check the column number, and that the timestamps are in a format "
     "csvdt reads ...",
     ["-H", "-p", "-u1"],
     b"ts,msg\n2024-01-01T00:00:00Z,ok\nJan  1 00:00:00,syslog\n", None),
    # The Emacs front end strips csvdt's own prefix and says which status it
    # exited with, so what it shows is csvdt's message either way.
    ("emacs/README.md",
     "csvdt exited 1: Specified column is out of bound: record 0 "
     "(line: 1) has 2 field(s), counted from 0",
     ["-u9"], b"a,b\n1,2\n", None),
    ("emacs/README.md",
     "csvdt exited 100: '--offset' target +15:00 is not a real UTC offset; "
     "they run from -12:00 to +14:00",
     ["-o0,+15:00"], b"a,b\n1,2\n", None),
]


def transcripts(temporary):
    """Every message the READMEs quote, against the message csvdt gives."""
    bad = []
    for path, shown, argv, stdin, fixture in MESSAGE_EXAMPLES:
        # Collapsed, because the README wraps a long message to its column.
        text = " ".join(open(os.path.join(ROOT, path),
                             encoding="utf-8").read().split())
        if " ".join(shown.split()) not in text:
            bad.append((path, "not in the README any more", shown, ""))
            continue
        if fixture:
            with open(os.path.join(temporary, fixture[0]), "wb") as handle:
                handle.write(fixture[1])
        done = subprocess.run([os.path.abspath(CSVDT)] + argv, input=stdin,
                              cwd=temporary, capture_output=True)
        said = " ".join(done.stderr.decode("utf-8", "replace").split())
        # 'csvdt exited N: ' is the front end's own wrapper round the rest.
        want = shown
        prefix = re.match(r"csvdt exited (\d+): ", want)
        if prefix:
            if str(done.returncode) != prefix.group(1):
                bad.append((path, f"exits {done.returncode}, not "
                            f"{prefix.group(1)}", shown, said))
                continue
            want = "csvdt: " + want[prefix.end():]
        # The README ends a long message with an ellipsis of its own.
        want = " ".join(want.split())
        if want.endswith(" ..."):
            if not said.startswith(want[:-4]):
                bad.append((path, "csvdt no longer starts it that way",
                            want[:-4], said))
        elif said != want:
            bad.append((path, "csvdt says something else", want, said))
    return bad


# The fixtures the help's EXAMPLES section works on. The section shows
# commands over named files, which is what makes it readable; the files have
# to exist somewhere for the commands to be run, and here is beside the
# claim rather than inside the help, where they would be noise to a reader.
HELP_EXAMPLE_FILES = {
    "log.csv": "id,when,level\n7,2023-11-14T22:13:20+01:00,info\n",
    "epoch.csv": "id,when,level\n7,1700000000,info\n",
    "shift.csv": "id,start,end\n"
                 "7,2026-01-01T09:00:00Z,2026-01-01T17:30:00Z\n",
    # Ragged on purpose, and in both directions: a row carrying a field the
    # header has no name for, and one stopping short. That is what gives the
    # fill-log example a header to pad as well as a row.
    "ragged.csv": "id,when,level\n"
                  "7,2023-11-14T22:13:20+01:00,info,restarted\n"
                  "8,2023-11-15T09:00:00+01:00\n",
}


def help_examples(temporary):
    """The EXAMPLES section of --help, run.

    A manual page's examples are the part a reader copies, and copied output
    that no longer matches is worse than none.

    An example is told from the prose around it by its indentation, which is
    how the help corpus marks a block laid out on purpose everywhere else --
    guessing at output by what the lines look like dropped two rows of a
    table on the first try.
    """
    done = subprocess.run([os.path.abspath(CSVDT), "--help"],
                          capture_output=True, text=True)
    lines = done.stdout.split("\n")
    numbered = [(len(line) - len(line.lstrip()), line.strip())
                for line in lines]
    try:
        section = next(index for index, (_, text) in enumerate(numbered)
                       if text == "EXAMPLES")
    except StopIteration:
        return [("--help", "has no EXAMPLES section", [], "")], 0
    prose = next(indent for indent, text in numbered[section + 1:] if text)

    for name, text in HELP_EXAMPLE_FILES.items():
        with open(os.path.join(temporary, name), "w") as handle:
            handle.write(text)

    bad, ran, index = [], 0, section + 1
    while index < len(numbered):
        indent, text = numbered[index]
        # The next section heading ends the examples.
        if text.isupper() and text and indent <= prose and len(text) > 3:
            break
        if not text.startswith("$ "):
            index += 1
            continue
        command, index = text[2:], index + 1
        shown = []
        while index < len(numbered):
            next_indent, next_text = numbered[index]
            if not next_text or next_indent <= prose \
                    or next_text.startswith("$ "):
                break
            shown.append(next_text)
            index += 1
        if not shown:
            continue          # a command shown without its output
        ran += 1
        done = subprocess.run(["bash", "-c", command], cwd=temporary,
                              capture_output=True, text=True,
                              env={**os.environ, "PATH":
                                   os.path.dirname(os.path.abspath(CSVDT))
                                   + os.pathsep + os.environ["PATH"]})
        got = [line.strip() for line in done.stdout.split("\n")
               if line.strip()]
        if got != shown:
            bad.append(("--help EXAMPLES", command, shown, "\n".join(got)))
    return bad, ran


def main():
    ran = skipped = 0
    bad = []
    for path in ("README.md", "emacs/README.md"):
        for where, command, shown in examples(os.path.join(ROOT, path)):
            if (NEEDS_THE_WORLD.search(command)
                    or NEEDS_THAT_BUILD.search(command)
                    or (NEEDS_A_FILE.search(command)
                        and "printf" not in command)):
                skipped += 1
                continue
            ran += 1
            done = subprocess.run(["bash", "-c", command], cwd=ROOT,
                                  capture_output=True, text=True,
                                  env={**os.environ, "PATH":
                                       os.path.dirname(os.path.abspath(CSVDT))
                                       + os.pathsep + os.environ["PATH"]})
            # A block showing a message is showing what reached stderr; one
            # showing rows is showing what reached standard output. Both are
            # offered, and the example is right if it matches either.
            expected = [l for l in wanted(shown) if l.strip()]
            for got in (done.stdout, done.stderr):
                if matches([l.rstrip() for l in got.split("\n") if l.strip()],
                           expected):
                    break
            else:
                bad.append((where, command, expected,
                            done.stdout.strip(), done.stderr.strip()))

    import tempfile
    with tempfile.TemporaryDirectory() as temporary:
        quoted = transcripts(temporary)
    with tempfile.TemporaryDirectory() as temporary:
        from_help, help_ran = help_examples(temporary)

    print(f"README examples: {ran} run, {skipped} needing a file or the "
          f"network skipped, {len(MESSAGE_EXAMPLES)} messages checked, "
          f"{help_ran} from the help's EXAMPLES, "
          f"{len(bad) + len(quoted) + len(from_help)} disagreeing")
    for where, command, shown, got in from_help:
        print(f"  {where}: {command}")
        print(f"    shown  : {shown}")
        print(f"    got    : {got.splitlines()}")
    for path, why, want, said in quoted:
        print(f"  {path}: {why}")
        print(f"    README : {want}")
        print(f"    csvdt  : {said}")
    for where, command, expected, out, err in bad:
        print(f"  {where}: {command}")
        print(f"    README : {expected}")
        print(f"    stdout : {out.splitlines()}")
        print(f"    stderr : {err.splitlines()}")
    return 1 if bad or quoted or from_help else 0


if __name__ == "__main__":
    sys.exit(main())
