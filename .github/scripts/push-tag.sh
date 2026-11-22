#!/usr/bin/env bash
# Writes a tag to GitHub with the TAG_PUSH_TOKEN secret, or checks that the
# secret is usable.
#
#   push-tag.sh --check
#   push-tag.sh [--force] COMMIT TAG
#
# GITHUB_REPOSITORY (OWNER/REPO) names the repository, and TAG_PUSH_TOKEN
# holds a fine-grained personal access token for it with Contents and
# Workflows read and write. GITHUB_TOKEN is a GitHub App token, and GitHub
# refuses to let an App write any ref whose workflow files differ from the
# default branch's: that reads as changing the workflows themselves, a
# permission GITHUB_TOKEN cannot hold. An archived build is by nature behind
# master, so from the first workflow edit after a version every tag naming it
# is such a ref, and "Resource not accessible by integration (HTTP 403)" is
# what the API says to it. Pushing with git gets the refusal in plain words.
#
# The tag is written with git and not the ref API, which refuses creating
# refs/tags/NAME with "Reference does not exist (HTTP 422)" here although
# every input is valid, for a reason that has not been established. One push
# creates a tag and moves one, and pushing a tag onto the commit it already
# names changes nothing and says nothing.
#
# --force is for a pointer that is meant to move (rpm-release, release); a
# permanent name carries the short commit and a push that disagrees with it
# should fail loudly.
#
# The token reaches git as an HTTP header set in the environment, so it is in
# no command line and no URL. Run on a checkout made with persist-credentials
# off, or the checkout's own header takes precedence.
set -euo pipefail

usage() {
    echo "usage: push-tag.sh --check | push-tag.sh [--force] COMMIT TAG" >&2
    exit 2
}

force=
check=
while [ $# -gt 0 ]; do
    case $1 in
        --force) force=--force; shift ;;
        --check) check=1; shift ;;
        -*) usage ;;
        *) break ;;
    esac
done

# A secret pasted with a trailing newline arrives still carrying it, which
# git reports as "credential url cannot be parsed", words that point
# anywhere but at the paste. Tokens never contain whitespace.
token=$(printf '%s' "${TAG_PUSH_TOKEN:-}" | tr -d '[:space:]')
if [ -z "$token" ]; then
    echo "::error::The TAG_PUSH_TOKEN secret is empty or not set. Publishing writes tags, which GITHUB_TOKEN is refused once master's workflow files have moved past the built commit. Create a fine-grained personal access token for this repository alone, holding Contents and Workflows read-and-write, and store it as the TAG_PUSH_TOKEN repository secret." >&2
    exit 1
fi
[ -n "$check" ] && exit 0

[ $# -eq 2 ] || usage
commit=$1
tag=$2

header="AUTHORIZATION: basic $(printf 'x-access-token:%s' "$token" | base64 -w0)"
echo "::add-mask::${header#AUTHORIZATION: basic }"
remote=${PUSH_URL:-https://github.com/${GITHUB_REPOSITORY}.git}

GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=http.extraheader GIT_CONFIG_VALUE_0=$header \
    git push ${force:+"$force"} "$remote" "$commit:refs/tags/$tag"
