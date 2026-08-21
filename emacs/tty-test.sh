#!/usr/bin/env bash
# Keystroke-level tests, which need a real terminal -- `emacs --batch' reads
# its own stdin, so a keyboard macro never reaches a minibuffer there.  Kept
# separate from run-tests.sh for that reason.
#
#   ./tty-test.sh
#   CSVDT=/path/to/csvdt ./tty-test.sh
set -euo pipefail

cd "$(dirname "$0")"
emacs=${EMACS:-emacs}

if ! command -v script >/dev/null; then
    echo "Needs util-linux 'script' to provide a pty." >&2
    exit 2
fi

report=$(mktemp)
trap 'rm -f "$report"' EXIT

# Emacs draws a frame on the pty, so the results come back through a file
# rather than being picked out of the escape sequences. script passes the
# child's exit status through, and the frame itself is of no interest.
status=0
CSVDT_TTY_REPORT="$report" \
    script -qec "\"$emacs\" -Q -nw -l \"$PWD/tty-test.el\"" /dev/null >/dev/null 2>&1 \
    || status=$?

cat "$report"
exit "$status"
