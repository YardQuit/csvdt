#!/usr/bin/env bash
# Tests packaging/set-version.sh on a scratch copy of the tree, which it
# leaves untouched: a dot in the old version is not a wildcard, and anything
# but three numbers is refused.
#
#   packaging/test-set-version.sh
set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
(cd "$here" && git ls-files -z | xargs -0 cp --parents -t "$scratch")
cd "$scratch"

old=$(grep -m1 '^version = ' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
lookalike=$(printf '%s' "$old" | tr '.' 'x')
echo "text naming $lookalike" >> README.md

if packaging/set-version.sh "$old-rc1" >/dev/null 2>&1; then
    echo "FAIL: a pre-release suffix was accepted" >&2
    exit 1
fi
packaging/set-version.sh 9.8.7 >/dev/null
if ! grep -qF "text naming $lookalike" README.md; then
    echo "FAIL: '$lookalike' was rewritten as though its x were a dot" >&2
    exit 1
fi
grep -q '^version = "9.8.7"' Cargo.toml || { echo "FAIL: Cargo.toml" >&2; exit 1; }
if ! grep -A1 '^name = "csvdt"$' Cargo.lock | grep -q '^version = "9.8.7"'; then
    echo "FAIL: Cargo.lock still names the old version of csvdt" >&2
    exit 1
fi
echo "set-version: ok"
