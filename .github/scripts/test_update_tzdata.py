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


def lock(csvdt, chrono_tz, phf, serde="1.0.1"):
    """The text of a small lock: csvdt uses chrono-tz and serde; chrono-tz uses phf."""
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
version = "{serde}"
checksum = "serde{serde}"
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
        self.assertEqual(problems(lock(0, "0.10.4", "0.12.1"),
                                  lock(0, "0.10.5", "0.12.1")), [])

    def test_its_private_dependency(self):
        self.assertEqual(problems(lock(0, "0.10.4", "0.12.1"),
                                  lock(0, "0.10.5", "0.12.2")), [])

    def test_another_crate_is_listed(self):
        found = problems(lock(0, "0.10.4", "0.12.1"),
                         lock(0, "0.10.5", "0.12.1", serde="1.0.2"))
        self.assertEqual(len(found), 1)
        self.assertIn("serde", found[0])

    def test_no_change(self):
        self.assertEqual(problems(lock(0, "0.10.4", "0.12.1"),
                                  lock(0, "0.10.4", "0.12.1")), [])


if __name__ == "__main__":
    unittest.main()
