#!/usr/bin/env bash
# Fail unless every terms update's website pages are on scanopy/website main.
#
# A terms notice links each changed document to scanopy.net/<page>/<effective>, which
# the website builds from src/lib/legal/<page>/<effective>.svelte. Prepare Release runs
# this so no release ships a notice whose links 404. Every terms file in the repo is
# checked, which covers both release phases without working out the previous tag.
#
# Usage: check-terms-pages.sh [terms dir]. Needs GH_TOKEN with read access to the
# website repo. WEBSITE_REPO overrides the repo, for testing.
set -euo pipefail

TERMS_DIR="${1:-backend/release-notes/terms}"
REPO="${WEBSITE_REPO:-scanopy/website}"

if [ -z "${GH_TOKEN:-}" ]; then
    echo "::error::GH_TOKEN is not set, so the terms pages on $REPO can't be checked"
    exit 1
fi

shopt -s nullglob
files=("$TERMS_DIR"/[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9].md)
if [ ${#files[@]} -eq 0 ]; then
    echo "No terms updates to check"
    exit 0
fi

missing=0
for file in "${files[@]}"; do
    effective="$(basename "$file" .md)"
    documents="$(sed -n 's/^documents:[[:space:]]*//p' "$file" | head -1 | tr ',' ' ')"
    if [ -z "$documents" ]; then
        echo "::error file=$file::no documents listed"
        missing=1
        continue
    fi
    for id in $documents; do
        path="src/lib/legal/$id/$effective.svelte"
        if output="$(gh api "repos/$REPO/contents/$path?ref=main" --silent 2>&1)"; then
            echo "scanopy.net/$id/$effective: on $REPO main"
        elif grep -q "HTTP 404" <<<"$output"; then
            echo "::error file=$file::scanopy.net/$id/$effective is not on $REPO main ($path). Publish it before releasing: the notice email links there."
            missing=1
        else
            echo "::error::Could not check $path on $REPO: $output"
            exit 1
        fi
    done
done
exit "$missing"
