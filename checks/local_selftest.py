"""Tests of the checks themselves, run with `python3 -m unittest`.

The differential checks are only worth reading if a bad answer from csvdt
fails them, so what can be tested without a second implementation is tested
here: that a csvdt which crashes fails each check, and that `local.py` tells a
difference between tzdata releases from a fault in csvdt.
"""
import os
import re
import subprocess
import sys
import tempfile
import unittest

import harness
import local

HERE = os.path.dirname(os.path.abspath(__file__))

# Stands in for csvdt.  It exits 101 on every run.
CRASHING = "#!/bin/sh\ncat >/dev/null\necho 'thread main panicked' >&2\nexit 101\n"

# Stands in for csvdt with two lies told: America/New_York is converted as if
# it were America/Chicago, so every offset disagrees with the host's by an
# hour while the instants stay right, and `--version` names the tzdata release
# in STUB_RELEASE, or none when that is empty.
SHIFTED = """#!/usr/bin/env python3
import os, subprocess, sys
real = [os.environ["REAL_CSVDT"]] + sys.argv[1:]
if sys.argv[1:] == ["--version"]:
    release = os.environ["STUB_RELEASE"]
    print("csvdt 1.0.0" + (" (IANA tzdata %s)" % release if release else ""))
    sys.exit(0)
env = dict(os.environ)
if env.get("TZ") == "America/New_York":
    env["TZ"] = "America/Chicago"
sys.exit(subprocess.run(real, env=env).returncode)
"""

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


@unittest.skipUnless(local.host_tzdata_release(),
                     "this host does not say which tzdata release it has")
class ReleaseTest(unittest.TestCase):
    """`local.py` excuses a disagreement only between different releases."""

    def run_local(self, release):
        """Run local.py on the shifted stub claiming tzdata RELEASE."""
        with tempfile.TemporaryDirectory() as directory:
            stub = write_stub(directory, "csvdt", SHIFTED)
            return run_check("local", stub, REAL_CSVDT=harness.CSVDT,
                             STUB_RELEASE=release)

    def test_other_release_is_named_not_failed(self):
        host = local.host_tzdata_release()
        done = self.run_local(host + "-other")
        self.assertEqual(done.returncode, 0, done.stdout)
        self.assertRegex(done.stdout, r"differ because the releases do")
        self.assertIn(host, done.stdout)
        self.assertIn(host + "-other", done.stdout)

    def test_same_release_fails(self):
        done = self.run_local(local.host_tzdata_release())
        self.assertEqual(done.returncode, 1, done.stdout)
        self.assertIsNone(re.search(r"differ because the releases", done.stdout))

    def test_unknown_release_fails(self):
        done = self.run_local("")
        self.assertEqual(done.returncode, 1, done.stdout)


class ReleaseRuleTest(unittest.TestCase):
    """`releases_differ` needs both releases, and that they are unequal."""

    def test_rule(self):
        self.assertTrue(local.releases_differ("2026f", "2025b"))
        self.assertFalse(local.releases_differ("2025b", "2025b"))
        self.assertFalse(local.releases_differ(None, "2025b"))
        self.assertFalse(local.releases_differ("2026f", None))


if __name__ == "__main__":
    unittest.main()
