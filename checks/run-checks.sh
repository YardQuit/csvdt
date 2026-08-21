#!/usr/bin/env bash
# Runs the differential checks against a real csvdt.
#
# These are not the test suite. `cargo test` and emacs/run-tests.sh pin
# behaviour that is decided; these ask a second implementation whether the
# behaviour is right, over generated input, and are meant to be run before a
# release rather than on every commit.
#
#   ./run-checks.sh                 # everything, against ../target/release/csvdt
#   ./run-checks.sh columns         # one check, by name
#   CSVDT=/path/to/csvdt ./run-checks.sh
#   SEED=7 ./run-checks.sh          # a different draw of generated input
#
# Every check is seeded, so a failure repeats. Exits non-zero if any check
# finds something.
set -uo pipefail

cd "$(dirname "$0")"
root=$(cd .. && pwd)

seed=${SEED:-0}
emacs=${EMACS:-emacs}

if [ -z "${CSVDT:-}" ]; then
    CSVDT="$root/target/release/csvdt"
    # Every time, not only when the binary is missing. Building only what was
    # absent meant an edited source beside a stale binary was never noticed:
    # the checks ran against the previous build and said ALL CLEAR, which is
    # the one answer that must never be wrong here. cargo is a no-op when the
    # binary is current, so this costs a second and buys the answer meaning
    # what it says. CSVDT set by hand is left alone -- that names a binary on
    # purpose, and may not have a source tree behind it at all.
    echo "== building csvdt"
    (cd "$root" && cargo build --release) || exit 2
fi
export CSVDT
[ -x "$CSVDT" ] || { echo "No csvdt at $CSVDT. Set CSVDT." >&2; exit 2; }

if ! command -v python3 >/dev/null; then
    echo "python3 is needed: it is the second implementation these compare against." >&2
    exit 2
fi

echo "== $("$CSVDT" --version | head -1)"
echo "== $(python3 --version), seed $seed"

wanted=${1:-}
failed=0
ran=0

# A heading only when the whole set is running; asking for one check by name
# should print that one check and nothing to scroll past.
section() { [ -z "$wanted" ] && printf '\n== %s\n' "$1"; return 0; }

run_check() {
    local name=$1 kind=$2
    if [ -n "$wanted" ] && [ "$wanted" != "$name" ]; then return 0; fi
    ran=$((ran + 1))
    local output status
    case $kind in
        python) output=$(python3 "$name.py" "$seed" 2>&1); status=$? ;;
        emacs)
            output=$(CSVDT_ROOT="$root" "$emacs" -Q --batch \
                --eval "(progn (add-to-list 'load-path \"$root/emacs\")
                               (setq csvdt-executable \"$CSVDT\"))" \
                -l "$name.el" 2>&1)
            status=$?
            # csvdt's own run summaries reach stderr under --batch; keep the
            # check's own lines, which are the ones that say anything here.
            output=$(printf '%s\n' "$output" | grep -v '^csvdt')
            ;;
    esac
    if [ $status -eq 0 ]; then
        # A check's summary lines start at the margin; anything it indented is
        # detail about a divergence, and there is none to show.
        printf '%s\n' "$output" | grep -v '^ ' | sed 's/^/   ok  /'
    else
        failed=$((failed + 1))
        printf '  FAIL %s\n' "$name"
        printf '%s\n' "$output" | sed 's/^/       /'
    fi
}

section "csvdt against another implementation"
run_check csv_roundtrip python
run_check quoting       python
run_check timestamps    python
run_check durations     python
run_check local         python

section "csvdt against its own documentation"
run_check columns       python
run_check list_options  python
run_check canonical     python
run_check peek          python
run_check exit_status   python
run_check tallies       python
run_check memory        python
run_check timing        python
run_check option_matrix python
run_check readme_examples python

if command -v "$emacs" >/dev/null; then
    section "the front end against the binary it wraps"
    run_check emacs_whole_buffer emacs
    run_check emacs_selection    emacs
    run_check emacs_compat       emacs
else
    # Named one of them and there is no Emacs to run it: say that, rather than
    # letting it fall through to the "no such check" below, which names the
    # one thing that is not the matter with it.
    case " emacs_whole_buffer emacs_selection emacs_compat " in
        *" $wanted "*)
            printf "\n'%s' needs Emacs, and '%s' is not on the path. Set EMACS.\n" \
                   "$wanted" "$emacs"
            exit 2
            ;;
    esac
    if [ -z "$wanted" ]; then
        printf '\n== no Emacs found, skipping the front-end checks (set EMACS)\n'
    fi
fi

echo
if [ "$ran" -eq 0 ]; then
    echo "No check named '$wanted'."
    exit 2
fi
if [ "$failed" -eq 0 ]; then
    echo "ALL CLEAR ($ran checks)"
else
    echo "$failed of $ran checks found something"
fi
exit $((failed > 0))
