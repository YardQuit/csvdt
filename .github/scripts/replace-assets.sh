#!/usr/bin/env bash
# Brings a release's assets in line with a directory.
#
#   replace-assets.sh TAG DIRECTORY NAME...
#
# Uploads DIRECTORY/NAME for each NAME in the order given, replacing any asset
# of that name, then deletes every asset of the release that DIRECTORY does
# not hold. The caller chooses the order so that an upload dying partway
# leaves checksums over the previous packages, never this run's checksums over
# last run's: files first, then what describes them. A file the caller wants
# strictly last is left out of NAME and uploaded after this returns; it is
# still kept here, because it is in DIRECTORY.
#
# The release is never removed, so a failure between steps leaves a release
# that is part new and part old rather than none.
set -euo pipefail

[ $# -ge 3 ] || { echo "usage: replace-assets.sh TAG DIRECTORY NAME..." >&2; exit 2; }
tag=$1
directory=$2
shift 2

for asset in "$@"; do
    gh release upload "$tag" "$directory/$asset" --clobber
done

produced=$(cd "$directory" && ls)
gh release view "$tag" --json assets --jq '.assets[].name' \
    | while read -r asset; do
        if ! printf '%s\n' "$produced" | grep -qxF -- "$asset"; then
            echo "Removing asset no longer produced: $asset"
            gh release delete-asset "$tag" "$asset" --yes
        fi
    done
