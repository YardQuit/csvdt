#!/usr/bin/env python3
"""Moves the time zone database crate forward in Cargo.lock, and nothing else.

    update-tzdata.py [--dry-run-lock OLD NEW]

Run in a checkout whose Cargo.lock is the committed one.  It runs
`cargo update -p chrono-tz`, which takes the newest chrono-tz release the
requirement in Cargo.toml allows, and then compares the lock before and after.
Only chrono-tz, and packages that are reachable from csvdt solely through
chrono-tz, may differ.  Anything else that changed (another crate bumped,
added or removed) means the lock moved for a reason this job was not asked to
act on: the script lists those packages and exits 1, and the caller does not
publish.  The change is made in the working tree only; nothing is committed.

chrono-tz carries the IANA database itself (its own `tz` directory, compiled
in by its build script) and no separate crate does, so in practice the set is
chrono-tz alone.  The closure rule exists so that a chrono-tz release that
needs a newer phf is not refused for it, while a bump of a crate csvdt also
uses is.

With --dry-run-lock the two lock files named are compared and cargo is not run,
which is how the rule is tested.
"""
import subprocess
import sys
import tomllib

TZDATA_CRATE = "chrono-tz"


def packages(path):
    """The packages of the lock file at PATH, as {(name, version): entry}."""
    with open(path, "rb") as handle:
        entries = tomllib.load(handle).get("package", [])
    return {(entry["name"], entry["version"]): entry for entry in entries}


def reachable(entries, roots):
    """Names of the packages reachable from the package names in ROOTS.

    ENTRIES is `packages` output.  Dependencies in a lock file are written
    "name" or "name version" (or with a source after that); only the name is
    used, which is enough since the sets are compared by name.
    """
    by_name = {}
    for (name, _), entry in entries.items():
        by_name.setdefault(name, []).append(entry)
    seen, todo = set(), list(roots)
    while todo:
        name = todo.pop()
        if name in seen:
            continue
        seen.add(name)
        for entry in by_name.get(name, []):
            todo.extend(dep.split(" ")[0] for dep in entry.get("dependencies", []))
    return seen


def tzdata_only(entries, root):
    """The names that may change with the tzdata crate in the lock ENTRIES.

    chrono-tz, and the packages reachable from the root package only through
    it.  ROOT is the name of the package that owns the lock (csvdt).
    """
    by_name = {name: entry for (name, _), entry in entries.items()}
    direct = [dep.split(" ")[0] for dep in by_name[root].get("dependencies", [])]
    others = reachable(entries, [d for d in direct if d != TZDATA_CRATE])
    return (reachable(entries, [TZDATA_CRATE]) - others) | {TZDATA_CRATE}


def unexpected_changes(old, new, root="csvdt"):
    """What differs between two lock files beyond the tzdata crate.

    OLD and NEW are `packages` output.  Returns a sorted list of lines
    describing each package added, removed or moved to another version or
    checksum, leaving out the names `tzdata_only` allows.
    """
    allowed = tzdata_only(old, root)
    problems = []
    for key in sorted(set(old) | set(new)):
        name = key[0]
        if name in allowed or old.get(key) == new.get(key):
            continue
        if key not in new:
            gone = [version for (n, version) in new if n == name]
            if not gone:
                problems.append(f"{name} {key[1]}: removed")
        elif key not in old:
            had = [version for (n, version) in old if n == name]
            problems.append(f"{name}: {', '.join(had) or 'absent'} -> {key[1]}")
        else:
            problems.append(f"{name} {key[1]}: checksum or dependencies changed")
    return problems


def main(argv):
    """Update the lock in the working directory, or compare two given locks."""
    if argv[:1] == ["--dry-run-lock"] and len(argv) == 3:
        old_path, new_path = argv[1], argv[2]
        old, new = packages(old_path), packages(new_path)
    elif not argv:
        try:
            old = packages("Cargo.lock")
        except FileNotFoundError:
            print("This version has no committed Cargo.lock, so there is "
                  "nothing to update the time zone crate in. A tag cut before "
                  "the lock was committed cannot be rebuilt by this job.",
                  file=sys.stderr)
            return 1
        subprocess.run(["cargo", "update", "-p", TZDATA_CRATE], check=True)
        new = packages("Cargo.lock")
    else:
        print(__doc__, file=sys.stderr)
        return 2

    before = sorted(v for (n, v) in old if n == TZDATA_CRATE)
    after = sorted(v for (n, v) in new if n == TZDATA_CRATE)
    problems = unexpected_changes(old, new)
    if problems:
        print("Packages other than the time zone crate changed in Cargo.lock; "
              "nothing will be published:", file=sys.stderr)
        for line in problems:
            print(f"  {line}", file=sys.stderr)
        return 1
    print(f"{TZDATA_CRATE}: {', '.join(before)} -> {', '.join(after)}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
