#!/usr/bin/env bash
# Byte-compiles csvdt.el, treating any warning as a failure, then runs the
# batch tests against a real csvdt.
#
#   ./run-tests.sh                  # uses ../target/release/csvdt
#   CSVDT=/path/to/csvdt ./run-tests.sh
set -euo pipefail

cd "$(dirname "$0")"

emacs=${EMACS:-emacs}
if ! command -v "$emacs" >/dev/null; then
    echo "No Emacs found. Set EMACS to its path." >&2
    exit 2
fi

echo "== $("$emacs" --version | head -1)"

report=$(mktemp "${TMPDIR:-/tmp}/csvdt-tests.XXXXXX")
trap 'rm -f "$report"' EXIT

# batch-byte-compile reports warnings on stderr and still exits 0, so the
# output is what has to be checked rather than the status.
rm -f csvdt.elc
warnings=$("$emacs" -Q --batch -f batch-byte-compile csvdt.el 2>&1 || true)
rm -f csvdt.elc
if [ -n "$warnings" ]; then
    echo "== byte-compile warnings:" >&2
    echo "$warnings" >&2
    exit 1
fi
echo "== byte-compiles clean"

# Through a pipe rather than straight out, so the verdict can be insisted on.
# The count above catches checks that stopped being reached; it cannot catch a
# file whose tail stopped being reached, because the line that prints the
# verdict and the one that sets the exit status are both in that tail. Wrapping
# everything from the middle down in a `when nil' left Emacs exiting 0 with no
# verdict at all, and this script passed it on.
"$emacs" -Q --batch -l test-csvdt.el 2>&1 | tee "$report"
status=${PIPESTATUS[0]}
if ! grep -qE '^(ALL PASS \(|ONLY |[0-9]+ FAILURE)' "$report"; then
    echo "== the tests ended without a verdict, so nothing here is proven" >&2
    exit 1
fi
exit "$status"
