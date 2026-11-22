#!/usr/bin/env python3
"""Writes the licence texts of the crates compiled into csvdt for one target.

    third-party-licences.py TARGET OUTPUT [--manifest-path Cargo.toml]

MIT and the other licences csvdt's dependencies use require their notice to
travel with a binary, so a release carries this file.  It is built from
`cargo metadata --locked --filter-platform TARGET`, which needs the lock file
to be current and the crate sources to be downloaded (cargo does that), but not
a toolchain for TARGET.

Only what ends up in the binary is listed: the packages reachable from csvdt
through normal dependencies.  Build-script and dev dependencies are tools of
the build and are not followed.  Neither are proc-macro crates, nor anything
reached only through them: a proc-macro runs in the compiler and what it
leaves in the binary is code csvdt's own crates generated.  jiff-static is
the one csvdt meets, and it is behind a feature csvdt does not enable.
csvdt itself is left out, since the release carries its LICENSE.

For each crate the file gives the name, version, licence expression and the
full text of every licence file the crate ships (LICENSE*, COPYING*,
UNLICENSE*, NOTICE*, and the file named by `license-file`), read from the
source directory cargo unpacked.  If a licence the expression names has no
file, the script lists the crate and the licence and exits 1: the text is not
supplied from memory.  An expression naming a licence this script cannot
recognise in a file fails the same way, and is dealt with by adding it to
LICENCES.
"""
import argparse
import json
import os
import re
import subprocess
import sys

# SPDX identifier -> (pattern for a file's name, pattern for its text).  A
# licence is shipped if some file of the crate matches either: the name for
# LICENSE-MIT, the text for a plain LICENSE that holds the Boost text.
LICENCES = {
    "MIT": (r"mit", r"permission is hereby granted, free of charge"),
    "Apache-2.0": (r"apache", r"apache license\s+version 2\.0"),
    "Unlicense": (r"unlicense", r"this is free and unencumbered software"),
    "BSL-1.0": (r"bsl|boost", r"boost software license"),
    "Unicode-3.0": (r"unicode", r"unicode license v3"),
    "BSD-2-Clause": (r"bsd", r"redistribution and use in source and binary"),
    "BSD-3-Clause": (r"bsd", r"neither the name of"),
    "ISC": (r"isc", r"permission to use, copy, modify, and/or distribute"),
    "Zlib": (r"zlib", r"this software is provided 'as-is'"),
    "0BSD": (r"0bsd", r"permission to use, copy, modify, and/or distribute"),
    "CC0-1.0": (r"cc0", r"cc0 1\.0 universal"),
    "MPL-2.0": (r"mpl", r"mozilla public license"),
}

FILE_NAME = re.compile(r"^(licen[cs]e|copying|unlicense|notice)", re.I)


def licence_ids(expression):
    """The licence identifiers named by the SPDX EXPRESSION, sorted.

    Operators, parentheses and "WITH" exceptions are dropped; crates.io also
    still holds old expressions that join with "/", read here as OR.
    """
    words = [w for w in re.split(r"[\s()/]+", expression or "") if w]
    ids = set()
    for index, word in enumerate(words):
        # The word after WITH is an exception to the licence before it.
        if word not in ("AND", "OR", "WITH") and words[index - 1] != "WITH":
            ids.add(word)
    return sorted(ids)


def binary_crates(metadata):
    """The packages of `cargo metadata` output METADATA that the binary holds.

    The packages reachable from the root through normal dependencies, never
    entering a proc-macro crate.  The root is not part of the result.
    """
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    root = metadata["resolve"]["root"]
    seen = {root}
    todo = [root]
    while todo:
        for dep in nodes[todo.pop()]["deps"]:
            if not any(kind["kind"] is None for kind in dep["dep_kinds"]):
                continue
            package = packages[dep["pkg"]]
            kinds = {k for target in package["targets"] for k in target["kind"]}
            if "proc-macro" in kinds or dep["pkg"] in seen:
                continue
            seen.add(dep["pkg"])
            todo.append(dep["pkg"])
    seen.discard(root)
    return sorted((packages[i] for i in seen),
                  key=lambda p: (p["name"], p["version"]))


def licence_files(package):
    """The licence files of PACKAGE as a sorted list of (name, text).

    Taken from the top of the directory holding its Cargo.toml, plus the
    file its `license-file` names.  The text is read as UTF-8; a file that is
    not, or is empty, raises ValueError naming it.
    """
    directory = os.path.dirname(package["manifest_path"])
    names = {n for n in os.listdir(directory)
             if FILE_NAME.match(n) and os.path.isfile(os.path.join(directory, n))}
    if package.get("license_file"):
        names.add(os.path.relpath(
            os.path.join(directory, package["license_file"]), directory))
    files = []
    for name in sorted(names):
        path = os.path.join(directory, name)
        try:
            with open(path, encoding="utf-8") as handle:
                text = handle.read()
        except (OSError, UnicodeDecodeError) as error:
            raise ValueError(f"{package['name']} {package['version']}: "
                             f"cannot read {name}: {error}")
        if not text.strip():
            raise ValueError(f"{package['name']} {package['version']}: "
                             f"{name} is empty")
        files.append((name, text))
    return files


def missing_licences(package, files):
    """The licences PACKAGE claims that none of its FILES holds, sorted.

    FILES is `licence_files` output.  A licence this script does not know
    counts as missing, since nothing can be said about it.
    """
    missing = []
    for licence in licence_ids(package["license"]):
        patterns = LICENCES.get(licence)
        found = patterns is not None and any(
            re.search(patterns[0], name, re.I)
            or re.search(patterns[1], re.sub(r"\s+", " ", text), re.I)
            for name, text in files)
        if not found:
            missing.append(licence)
    return missing


def render(packages):
    """The text of the output file for the PACKAGES.

    Raises ValueError listing every crate that lacks a licence file or whose
    file cannot be read.
    """
    problems = []
    sections = []
    for package in packages:
        label = f"{package['name']} {package['version']}"
        try:
            files = licence_files(package)
        except ValueError as error:
            problems.append(str(error))
            continue
        missing = missing_licences(package, files)
        if missing:
            problems.append(f"{label}: no licence file for "
                            + ", ".join(missing)
                            + f" (licence: {package['license']})")
            continue
        parts = [f"{'=' * 72}\n{label}\nLicence: {package['license']}\n"]
        for name, text in files:
            parts.append(f"\n--- {name} ---\n\n{text.rstrip()}\n")
        sections.append("".join(parts))
    if problems:
        raise ValueError("\n".join(problems))
    # Nothing about the target, so that targets holding the same crates give
    # the same file and the release can carry one copy.
    header = (f"Licences of the third-party crates compiled into csvdt.\n"
              f"{len(packages)} crates, each with the text of the licence "
              f"files it ships.\n\n")
    return header + "\n".join(sections)


def metadata_for(target, manifest_path):
    """`cargo metadata` output for TARGET, from the locked dependency set."""
    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1",
         "--filter-platform", target, "--manifest-path", manifest_path],
        check=True, stdout=subprocess.PIPE, text=True)
    return json.loads(result.stdout)


def main(argv):
    parser = argparse.ArgumentParser(description="Write third-party licences.")
    parser.add_argument("target", help="target triple, such as "
                                       "x86_64-unknown-linux-musl")
    parser.add_argument("output", help="file to write")
    parser.add_argument("--manifest-path", default="Cargo.toml")
    args = parser.parse_args(argv)
    packages = binary_crates(metadata_for(args.target, args.manifest_path))
    try:
        text = render(packages)
    except ValueError as error:
        print(f"Licence texts are missing for {args.target}:\n{error}",
              file=sys.stderr)
        return 1
    with open(args.output, "w", encoding="utf-8") as handle:
        handle.write(text)
    print(f"{args.target}: {len(packages)} crates -> {args.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
