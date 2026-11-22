#!/usr/bin/env bash
# Checks Cargo.lock in the current directory against the RustSec advisory
# database, passing over the advisories listed in .github/audit-ignore.
#
#   audit.sh
#
# cargo-audit is installed at a fixed version (cargo checks the crate against
# the index's checksum) and fetches the database afresh, so a runner that
# starts empty audits against today's advisories. A vulnerability is an
# error; an unmaintained or yanked crate is printed and is not.
#
# The ignore list is read from beside this script, not from the checked-out
# tree, so an old version tag is audited by the rules of the workflow that
# builds it.
set -euo pipefail

version=0.22.2
ignore_file=$(dirname "$0")/../audit-ignore

if ! command -v cargo-audit >/dev/null || ! cargo audit --version | grep -qF " $version"; then
    cargo install cargo-audit --version "$version" --locked
fi

args=()
while read -r id; do
    args+=(--ignore "$id")
done < <(sed 's/#.*//' "$ignore_file" | awk 'NF {print $1}')

cargo audit "${args[@]}"
