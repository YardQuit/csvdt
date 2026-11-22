"""Tests of the checks themselves, run with `python3 -m unittest`.

The differential checks are only worth reading if a bad answer from csvdt
fails them, so what can be tested without a second implementation is tested
here: that a csvdt which crashes fails each check.
"""
import os
import subprocess
import sys
import tempfile
import unittest


HERE = os.path.dirname(os.path.abspath(__file__))

# Stands in for csvdt.  It exits 101 on every run.
CRASHING = "#!/bin/sh\ncat >/dev/null\necho 'thread main panicked' >&2\nexit 101\n"

# Checks whose first step is a run of csvdt that has to succeed.  memory.py
# and timing.py build large files and are left out for the time they take.
QUICK = ["csv_roundtrip", "quoting", "timestamps", "durations", "local",
         "columns", "list_options", "canonical", "peek", "exit_status",
         "tallies", "option_matrix", "readme_examples"]


def write_stub(directory, name, body):
    """Write an executable script NAME into DIRECTORY and return its path."""
    path = os.path.join(directory, name)
    with open(path, "w") as handle:
        handle.write(body)
    os.chmod(path, 0o755)
    return path


def run_check(name, csvdt, **extra):
    """Run check NAME against the binary CSVDT and return the finished run."""
    env = dict(os.environ, CSVDT=csvdt, **extra)
    return subprocess.run([sys.executable, os.path.join(HERE, name + ".py"), "0"],
                          capture_output=True, text=True, env=env, timeout=600,
                          stdin=subprocess.DEVNULL)


class CrashTest(unittest.TestCase):
    """A csvdt that dies on every input fails every check."""

    def test_each_check_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            stub = write_stub(directory, "csvdt", CRASHING)
            for name in QUICK:
                with self.subTest(check=name):
                    done = run_check(name, stub)
                    self.assertNotEqual(done.returncode, 0, done.stdout)
                    self.assertRegex(done.stderr + done.stdout, "crashed|PANIC")


if __name__ == "__main__":
    unittest.main()
