"""Tests of third-party-licences.py on small trees.  Run: python3 -m unittest."""
import importlib.util
import os
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location(
    "third_party_licences", os.path.join(HERE, "third-party-licences.py"))
licences = importlib.util.module_from_spec(spec)
spec.loader.exec_module(licences)

MIT = "Permission is hereby granted, free of charge, to any person\n"
APACHE = "                Apache License\n          Version 2.0, January 2004\n"


def package(root, name, kind="lib", expression="MIT", files=None, version="1.0.0"):
    """A `cargo metadata` package whose source is ROOT/NAME, holding FILES
    ({file name: text}); by default one file, LICENSE-MIT."""
    directory = os.path.join(root, name)
    os.makedirs(directory)
    for file_name, text in (files or {"LICENSE-MIT": MIT}).items():
        with open(os.path.join(directory, file_name), "w") as handle:
            handle.write(text)
    return {"id": name, "name": name, "version": version, "license": expression,
            "manifest_path": os.path.join(directory, "Cargo.toml"),
            "targets": [{"kind": [kind]}]}


def metadata(packages, edges):
    """Metadata for the PACKAGES with a root called "csvdt" added, and EDGES
    given as {package: [(dependency, kind)]}, kind being None for a normal
    dependency, as cargo writes it."""
    root = {"id": "csvdt", "name": "csvdt", "version": "1.0.0",
            "license": "GPL-3.0-or-later", "manifest_path": "/nowhere/Cargo.toml",
            "targets": [{"kind": ["bin"]}]}
    nodes = [{"id": name, "deps": [
        {"pkg": dep, "dep_kinds": [{"kind": kind, "target": None}]}
        for dep, kind in deps]} for name, deps in edges.items()]
    for p in packages:
        if p["id"] not in edges:
            nodes.append({"id": p["id"], "deps": []})
    return {"packages": [root] + packages,
            "resolve": {"root": "csvdt", "nodes": nodes}}


class ExpressionTest(unittest.TestCase):
    def test_operators_and_brackets_are_dropped(self):
        self.assertEqual(
            licences.licence_ids("(MIT OR Apache-2.0) AND Unicode-3.0"),
            ["Apache-2.0", "MIT", "Unicode-3.0"])

    def test_an_old_slash_is_an_or(self):
        self.assertEqual(licences.licence_ids("Unlicense/MIT"),
                         ["MIT", "Unlicense"])

    def test_an_exception_is_not_a_licence(self):
        self.assertEqual(
            licences.licence_ids("Apache-2.0 WITH LLVM-exception OR MIT"),
            ["Apache-2.0", "MIT"])


class SelectionTest(unittest.TestCase):
    """What counts as being in the binary."""

    def crates(self, edges, kinds=None):
        with tempfile.TemporaryDirectory() as root:
            kinds = kinds or {}
            names = {n for deps in edges.values() for n, _ in deps} | set(edges)
            names.discard("csvdt")
            packages = [package(root, n, kinds.get(n, "lib")) for n in sorted(names)]
            found = licences.binary_crates(metadata(packages, edges))
            return [p["name"] for p in found]

    def test_normal_dependencies_and_theirs(self):
        self.assertEqual(self.crates({"csvdt": [("a", None)], "a": [("b", None)]}),
                         ["a", "b"])

    def test_the_root_is_left_out(self):
        self.assertNotIn("csvdt", self.crates({"csvdt": [("a", None)],
                                               "a": [("csvdt", None)]}))

    def test_build_and_dev_dependencies_are_not_followed(self):
        self.assertEqual(self.crates({"csvdt": [("a", None), ("b", "build"),
                                                ("c", "dev")]}), ["a"])
        self.assertEqual(self.crates({"csvdt": [("a", None)],
                                      "a": [("b", "build")]}), ["a"])

    def test_a_proc_macro_and_what_only_it_uses_are_left_out(self):
        # windows-core uses windows-implement, which uses syn.
        found = self.crates({"csvdt": [("core", None)],
                             "core": [("link", None), ("implement", None)],
                             "implement": [("syn", None)]},
                            {"implement": "proc-macro"})
        self.assertEqual(found, ["core", "link"])

    def test_a_crate_reached_both_ways_is_kept(self):
        found = self.crates({"csvdt": [("core", None)],
                             "core": [("implement", None), ("syn", None)],
                             "implement": [("syn", None)]},
                            {"implement": "proc-macro"})
        self.assertEqual(found, ["core", "syn"])

    def test_a_crate_with_a_build_script_is_kept(self):
        with tempfile.TemporaryDirectory() as root:
            crate = package(root, "libc")
            crate["targets"] = [{"kind": ["lib"]}, {"kind": ["custom-build"]}]
            found = licences.binary_crates(
                metadata([crate], {"csvdt": [("libc", None)]}))
            self.assertEqual([p["name"] for p in found], ["libc"])


class TextTest(unittest.TestCase):
    def render(self, **kwargs):
        with tempfile.TemporaryDirectory() as root:
            crate = package(root, "a", **kwargs)
            return licences.render([crate])

    def test_name_version_expression_and_text(self):
        text = self.render(expression="MIT")
        self.assertIn("a 1.0.0\nLicence: MIT\n", text)
        self.assertIn("--- LICENSE-MIT ---", text)
        self.assertIn(MIT, text)

    def test_every_licence_file_is_included(self):
        text = self.render(expression="MIT OR Apache-2.0", files={
            "LICENSE-MIT": MIT, "LICENSE-APACHE": APACHE, "NOTICE": "Notice.\n",
            "Cargo.toml": "not a licence\n"})
        for name in ("LICENSE-MIT", "LICENSE-APACHE", "NOTICE"):
            self.assertIn(f"--- {name} ---", text)
        self.assertNotIn("not a licence", text)

    def test_lower_case_names_are_found(self):
        text = self.render(expression="MIT", files={"license-mit": MIT})
        self.assertIn("--- license-mit ---", text)

    def test_a_plain_licence_file_holding_the_text_counts(self):
        self.assertIn("--- LICENSE ---", self.render(files={"LICENSE": MIT}))

    def test_a_named_license_file_is_read(self):
        with tempfile.TemporaryDirectory() as root:
            crate = package(root, "a", files={"terms.txt": MIT})
            crate["license_file"] = "terms.txt"
            self.assertIn("--- terms.txt ---", licences.render([crate]))

    def test_the_crates_are_listed_in_the_order_given(self):
        with tempfile.TemporaryDirectory() as root:
            both = [package(root, "a"), package(root, "b")]
            text = licences.render(both)
            self.assertLess(text.index("\na 1.0.0\n"), text.index("\nb 1.0.0\n"))
            self.assertIn("2 crates", text)


class ExpressionFilesTest(unittest.TestCase):
    def test_a_compound_expression_needs_all_its_licences(self):
        expression = "(MIT OR Apache-2.0) AND Unicode-3.0"
        files = {"LICENSE-MIT": MIT, "LICENSE-APACHE": APACHE}
        with tempfile.TemporaryDirectory() as root:
            crate = package(root, "a", expression=expression, files=files)
            with self.assertRaises(ValueError) as caught:
                licences.render([crate])
            self.assertIn("Unicode-3.0", str(caught.exception))
        files["LICENSE-UNICODE"] = "UNICODE LICENSE V3\n"
        with tempfile.TemporaryDirectory() as root:
            crate = package(root, "a", expression=expression, files=files)
            self.assertIn("LICENSE-UNICODE", licences.render([crate]))


class MissingTest(unittest.TestCase):
    def failure(self, crates):
        """The ValueError message from rendering CRATES, a list of keyword
        arguments for `package`."""
        with tempfile.TemporaryDirectory() as root:
            packages = [package(root, **crate) for crate in crates]
            with self.assertRaises(ValueError) as caught:
                licences.render(packages)
            return str(caught.exception)

    def test_a_claimed_licence_without_a_file_names_the_crate(self):
        message = self.failure([{"name": "a", "version": "2.3.4",
                                 "expression": "MIT OR Apache-2.0"}])
        self.assertIn("a 2.3.4", message)
        self.assertIn("Apache-2.0", message)
        self.assertNotIn("MIT,", message)

    def test_every_failing_crate_is_named_not_just_the_first(self):
        message = self.failure([{"name": "a", "files": {"README": "x"}},
                                {"name": "b", "files": {"README": "x"}}])
        self.assertIn("a 1.0.0", message)
        self.assertIn("b 1.0.0", message)

    def test_a_licence_the_script_does_not_know_fails(self):
        message = self.failure([{"name": "a", "expression": "Frobnicate-1.0",
                                 "files": {"LICENSE": MIT}}])
        self.assertIn("Frobnicate-1.0", message)

    def test_a_crate_with_no_expression_claims_nothing(self):
        with tempfile.TemporaryDirectory() as root:
            crate = package(root, "a", expression=None, files={"README": "x"})
            self.assertIn("a 1.0.0", licences.render([crate]))

    def test_an_empty_file_fails(self):
        message = self.failure([{"name": "a", "files": {"LICENSE-MIT": " \n"}}])
        self.assertIn("a 1.0.0", message)
        self.assertIn("empty", message)

    def test_a_file_that_is_not_text_fails(self):
        with tempfile.TemporaryDirectory() as root:
            crate = package(root, "a")
            with open(os.path.join(root, "a", "LICENSE-MIT"), "wb") as handle:
                handle.write(b"\xff\xfe")
            with self.assertRaises(ValueError) as caught:
                licences.render([crate])
            self.assertIn("a 1.0.0", str(caught.exception))


if __name__ == "__main__":
    unittest.main()
