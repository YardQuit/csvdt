"""The one way these checks run csvdt, and the one place its path is decided.

Each check used to carry its own copy of the path and its own call to
`subprocess.run`, and a few of them never looked at the exit status: a
binary that crashed on every input produced no output, which to a check that
only compares output reads as nothing to compare, and it passed. Going
through `run` here means a crash is seen once, by all of them.
"""
import csv
import io
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# The binary under test: CSVDT from the environment, else the release build
# beside this directory.  Absolute, so that a check which changes directory
# still finds it.
CSVDT = os.path.abspath(os.environ.get(
    "CSVDT", os.path.join(ROOT, "target", "release", "csvdt")))


def crashed(done):
    """Whether a finished run died rather than answered.

    That is a Rust panic (exit 101, or the word in the message), or the
    process being killed by a signal.  csvdt's own failures are the exits 1,
    2 and 100, which are answers.  DONE is a `subprocess.CompletedProcess`.
    """
    return (done.returncode == 101 or done.returncode < 0
            or b"panicked" in done.stderr)


def run(args, data=b"", *, env=None, timeout=60, stdout=subprocess.PIPE,
        cwd=None, crash_ok=False):
    """Run csvdt with ARGS, feeding it DATA, and return the finished run.

    DATA is bytes or text (encoded as UTF-8).  Output is captured as bytes in
    `stdout` and `stderr` unless STDOUT names a file to write to instead.
    If csvdt crashes (see `crashed`) the check stops here, prints what
    happened and exits 1, so no comparison is made over the output of a dead
    process; a check that is looking for crashes itself passes CRASH_OK=True
    and reads the result with `crashed`.  A run over TIMEOUT seconds raises
    `subprocess.TimeoutExpired`, which the caller may want to report.
    """
    if isinstance(data, str):
        data = data.encode()
    done = subprocess.run([CSVDT] + list(args), input=data, stdout=stdout,
                          stderr=subprocess.PIPE, env=env, cwd=cwd,
                          timeout=timeout)
    if not crash_ok and crashed(done):
        shown = " ".join(args)[:100]
        print(f"csvdt crashed (exit {done.returncode}) on: {shown}\n"
              f"  {done.stderr.decode('utf-8', 'replace').strip()[:300]}",
              file=sys.stderr)
        sys.exit(1)
    return done


def fail(message):
    """Stop the check at once: print MESSAGE to stderr and exit 1.

    For a situation in which no comparison can be made at all, such as csvdt
    answering with fewer rows than it was given.
    """
    print(message, file=sys.stderr)
    sys.exit(1)


def records(text, delimiter=",", quotechar='"'):
    """The records Python's csv reads from TEXT, empty lines left out.

    The reference reader of these checks.  None where the text is not CSV
    that Python can read (csv.Error).
    """
    try:
        return [row for row in csv.reader(io.StringIO(text, newline=""),
                                          delimiter=delimiter,
                                          quotechar=quotechar) if row != []]
    except csv.Error:
        return None


def written(directory, name, *parts):
    """Write PARTS (str or bytes, in order) to NAME in DIRECTORY; return the path.

    A part may be a large object repeated by the caller, so a fixture of many
    megabytes goes to disk without being joined in this process.
    """
    path = os.path.join(directory, name)
    with open(path, "wb") as handle:
        for part in parts:
            handle.write(part.encode() if isinstance(part, str) else part)
    return path


def text(raw):
    """RAW bytes as text, with anything that is not UTF-8 replaced."""
    return raw.decode("utf-8", "replace")
