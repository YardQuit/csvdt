#!/usr/bin/env bash
# Sets csvdt's version everywhere it is written down.
#
# The version lives in three places that are checked (Cargo.toml, with the
# entry for the package in Cargo.lock, the Emacs package's header, and the
# RPM spec) and in a handful of README lines that are not. Bumping only the
# first is caught by CI, eventually, on the leg that runs the Emacs suite or
# builds the RPM; the README lines are caught by nobody and simply go on
# naming the last release.
#
#   packaging/set-version.sh 1.3.0
#
# What it will not touch, on purpose:
#
#   the spec's %changelog, and CHANGELOG.md's released sections, are a
#   record of what happened. A version written there was true when it was
#   written and stays true. Only the '## Unreleased' heading moves.
#
#   the spec's %changelog entry for the new version. That is a sentence
#   about what changed, which nothing here knows. It is reported as the one
#   thing left to do by hand.
set -euo pipefail

cd "$(dirname "$0")/.."

new=${1-}
if [ -z "$new" ]; then
    echo "usage: packaging/set-version.sh <version>   e.g. 1.3.0" >&2
    exit 2
fi
# Three numbers and nothing else, which is what a release tag, the RPM spec's
# Version and the workflows' tag filter all accept: a pre-release suffix would
# be refused by the spec and never start a build.
if ! printf '%s' "$new" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "'$new' does not look like a version (expected 1.3.0)" >&2
    exit 2
fi

old=$(grep -m1 '^version = ' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
if [ -z "$old" ]; then
    echo "could not read the current version from Cargo.toml" >&2
    exit 1
fi
if [ "$old" = "$new" ]; then
    echo "already $new; nothing to do"
    exit 0
fi
echo "== $old -> $new"

# The old version as a pattern: its dots are literal, not "any character".
old_re=$(printf '%s' "$old" | sed 's/[.]/\\./g')

# The three that are checked. Each is matched by its own field rather than
# by the version string, so a line that merely mentions the old version
# cannot be hit by accident.
sed -i "s/^version = \"$old_re\"/version = \"$new\"/" Cargo.toml
sed -i "s/^;; Version: .*/;; Version: $new/" emacs/csvdt.el
# Cargo.lock names the package's own version, and --locked builds refuse a
# lock that disagrees with Cargo.toml. Offline: only this entry changes.
cargo update --workspace --offline >/dev/null 2>&1 \
    || { echo "cargo could not update Cargo.lock for $new" >&2; exit 1; }

# The spec, above its %changelog only. Release goes back to 1: the number
# counts packagings of one upstream version, so a new version starts over.
changelog_at=$(grep -n '^%changelog' packaging/rpm/csvdt.spec | cut -d: -f1)
sed -i "1,$((changelog_at - 1)){
    s/^Version:\( *\).*/Version:\1$new/
    s/^Release:\( *\)[0-9]*/Release:\11/
    s/csvdt-$old_re-/csvdt-$new-/g
}" packaging/rpm/csvdt.spec

# The prose. Most mentions in these two are an example naming the current
# release (a built filename, a --version line, a table row) so the whole
# file is substituted rather than picked over. Not all of them are: a
# sentence about what went wrong at an earlier release names that release,
# and rewriting it moves the blame onto this one. Nothing here can tell the
# two apart, so the lines are printed below and read by a person.
rewrote=$(grep -Fn "$old" README.md packaging/rpm/README.md || true)
sed -i "s/$old_re/$new/g" README.md packaging/rpm/README.md

# And the changelog's heading, which is the one line in that file that is
# about the release being made rather than about one already made.
sed -i "0,/^## Unreleased$/s//## $new/" CHANGELOG.md

echo
echo "== where it now says $new"
grep -n "^version = \"$new\"" Cargo.toml | sed 's/^/   Cargo.toml:/'
grep -n "^;; Version: $new" emacs/csvdt.el | sed 's/^/   emacs\/csvdt.el:/'
grep -n "^Version: *$new" packaging/rpm/csvdt.spec | sed 's/^/   packaging\/rpm\/csvdt.spec:/'
{ grep -Fc "$new" README.md packaging/rpm/README.md || true; } | sed 's/^/   mentions in /'

echo
echo "== the prose lines this rewrote, as they read before"
printf '%s\n' "$rewrote" | sed 's/^/   /'
echo "   Each should be an example naming the release being made. One that is"
echo "   a statement about an earlier release now names this one instead."

# Anything left saying the old version is either history, which is right,
# or a place this script does not know about, which is worth seeing.
echo
echo "== still says $old (history below %changelog and in released sections is expected)"
grep -rnF "$old" --include='*.toml' --include='*.md' --include='*.el' \
    --include='*.spec' . 2>/dev/null \
    | grep -v '^./target' | grep -v '^./Cargo.lock' | sed 's/^/   /' || true

echo
echo "== left to do by hand"
echo "   packaging/rpm/csvdt.spec  -- a %changelog entry for $new-1, which the"
echo "                               RPM workflow fails without"
echo "   CHANGELOG.md              -- the heading moved, but the paragraph under"
echo "                               it still reads as though $new were unreleased"
echo
echo "   then: cargo build --release && emacs/run-tests.sh   (the Emacs suite"
echo "   fails if csvdt.el's header has drifted from the crate, and the RPM"
echo "   workflow fails if the spec has)"
