#!/usr/bin/env bash
# Copyright (C) 2025-2026 Michael A Jones.  GPL-3.0-or-later; see LICENSE.
# Run the csvdt package's tests in a Tuatara loaded from source.
# Usage: TUATARA=/path/to/tuatara tuatara/run-tests.sh
# TUATARA defaults to a checkout beside this repository's. The runs need a
# csvdt: CSVDT=/path/to/csvdt, else target/release/csvdt (cargo build
# --release), else the one on the PATH.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
export TUATARA="${TUATARA:-$here/../../tuatara}"
if [ ! -f "$TUATARA/tuatara.asd" ]; then
  echo "No Tuatara checkout at $TUATARA: set TUATARA" >&2
  exit 2
fi
# the toolbox Tuatara builds in, when there is one
. "$TUATARA/tools/inside.sh"
inside env TUATARA="$TUATARA" ${CSVDT:+CSVDT="$CSVDT"} sbcl --dynamic-space-size 4GB --non-interactive --load "$here/run-tests.lisp"
