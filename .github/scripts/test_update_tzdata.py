"""Tests of update-tzdata.py on small lock files.  Run: python3 -m unittest."""
import importlib.util
import os
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location(
    "update_tzdata", os.path.join(HERE, "update-tzdata.py"))
update = importlib.util.module_from_spec(spec)
spec.loader.exec_module(update)


def lock(tzdb, private, jiff="1.0.2", serde="1.0.1"):
    """The text of a small lock laid out as csvdt's is.

    csvdt uses jiff, jiff-tzdb and serde; jiff uses jiff-tzdb too; jiff-tzdb
    uses a package of its own, PRIVATE, which the real one does not have but
    a later release might.
    """
    return f'''version = 4

[[package]]
name = "csvdt"
version = "1.0.0"
dependencies = ["jiff", "jiff-tzdb", "serde"]

[[package]]
name = "jiff"
version = "{jiff}"
checksum = "jiff{jiff}"
dependencies = ["jiff-tzdb"]

[[package]]
name = "jiff-tzdb"
version = "{tzdb}"
checksum = "tz{tzdb}"
dependencies = ["private"]

[[package]]
name = "private"
version = "{private}"
checksum = "private{private}"

[[package]]
name = "serde"
version = "{serde}"
checksum = "serde{serde}"
'''


def chrono_lock(chrono_tz, phf):
    """The text of a lock from before the move to jiff: csvdt uses chrono-tz
    and serde; chrono-tz uses phf."""
    return f'''version = 4

[[package]]
name = "csvdt"
version = "1.0.0"
dependencies = ["chrono-tz", "serde"]

[[package]]
name = "chrono-tz"
version = "{chrono_tz}"
checksum = "tz{chrono_tz}"
dependencies = ["phf"]

[[package]]
name = "phf"
version = "{phf}"
checksum = "phf{phf}"

[[package]]
name = "serde"
version = "1.0.1"
checksum = "serde1.0.1"
'''


def problems(old_text, new_text):
    """Run the comparison on two lock texts."""
    with tempfile.TemporaryDirectory() as directory:
        paths = []
        for name, text in (("old", old_text), ("new", new_text)):
            paths.append(os.path.join(directory, name))
            with open(paths[-1], "w") as handle:
                handle.write(text)
        return update.unexpected_changes(update.packages(paths[0]),
                                         update.packages(paths[1]))


class RuleTest(unittest.TestCase):
    """Only the tzdata crate, and what is reachable only through it, may move."""

    def test_tzdata_crate_alone(self):
        self.assertEqual(problems(lock("1.0.0", "0.1.0"),
                                  lock("1.0.1", "0.1.0")), [])

    def test_its_private_dependency(self):
        # Reached through jiff as well as directly, but only ever by way of
        # jiff-tzdb, so it is the tzdata crate's own business.
        self.assertEqual(problems(lock("1.0.0", "0.1.0"),
                                  lock("1.0.1", "0.1.1")), [])

    def test_another_crate_is_listed(self):
        found = problems(lock("1.0.0", "0.1.0"),
                         lock("1.0.1", "0.1.0", serde="1.0.2"))
        self.assertEqual(len(found), 1)
        self.assertIn("serde", found[0])

    def test_the_crate_that_reads_the_data_is_listed(self):
        # jiff depends on jiff-tzdb, not the other way round: moving it is a
        # change of code, which this job was not asked to make.
        found = problems(lock("1.0.0", "0.1.0"),
                         lock("1.0.1", "0.1.0", jiff="1.0.3"))
        self.assertEqual(len(found), 1)
        self.assertIn("jiff", found[0])

    def test_a_lock_from_before_jiff_moves_chrono_tz(self):
        # The newest version tag may predate the move, and is still rebuilt.
        self.assertEqual(problems(chrono_lock("0.10.4", "0.12.1"),
                                  chrono_lock("0.10.5", "0.12.2")), [])
        found = problems(chrono_lock("0.10.4", "0.12.1"),
                         lock("1.0.0", "0.1.0"))
        self.assertTrue(any("jiff" in line for line in found), found)

    def test_no_change(self):
        self.assertEqual(problems(lock("1.0.0", "0.1.0"),
                                  lock("1.0.0", "0.1.0")), [])


if __name__ == "__main__":
    unittest.main()
