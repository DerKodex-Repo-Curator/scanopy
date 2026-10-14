#!/usr/bin/env bash
# Create a terms update: backend/release-notes/terms/<effective>.md, the file that
# makes Scanopy Cloud email notice of it, plus a draft of each changed page in the
# website checkout (src/lib/legal/<page>/<effective>.svelte, copied from the version
# in force), and an issue in scanopy/website, assigned to you, to publish them.
#
# Asks for the effective date and the documents, prefilling each, and applies the
# same date rules as `check_dates` in src/server/release_notes/terms.rs so a bad date
# is caught here rather than in CI. Answers can also come from the environment
# (EFFECTIVE, DOCUMENTS); without a terminal the prefilled values are used.
#
# Needs the website checkout at WEBSITE_DIR (default: ../website beside this repo)
# and a logged-in `gh`. It checks both before writing anything.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TERMS_DIR="$SCRIPT_DIR/../release-notes/terms"

MIN_NOTICE_DAYS=30
MAX_LEAD_DAYS=180
DOCUMENT_IDS="terms privacy refund security"
# Keep in sync with PLACEHOLDER in src/server/release_notes/terms.rs: a body
# still holding it doesn't parse, so an unedited file fails CI.
PLACEHOLDER="<describe the change>"

die() {
    echo "new-terms-update: $*" >&2
    exit 1
}

# `date` differs between macOS (BSD) and Linux (GNU).
if date -v+1d >/dev/null 2>&1; then
    add_days() { date -j -v+"$2"d -f %Y-%m-%d "$1" +%Y-%m-%d; }
    normalize() { date -j -f %Y-%m-%d "$1" +%Y-%m-%d 2>/dev/null; }
else
    add_days() { date -d "$1 + $2 days" +%Y-%m-%d; }
    normalize() { date -d "$1" +%Y-%m-%d 2>/dev/null; }
fi

is_date() {
    [[ "$1" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] && [ "$(normalize "$1")" = "$1" ]
}

document_name() {
    case "$1" in
        terms) echo "Terms of Service" ;;
        privacy) echo "Privacy Policy" ;;
        refund) echo "Refund Policy" ;;
        security) echo "Security page" ;;
    esac
}

# Prerequisites, checked before anything is asked or written.
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
WEBSITE_DIR="${WEBSITE_DIR:-$REPO_ROOT/../website}"
LEGAL_DIR="$WEBSITE_DIR/src/lib/legal"
[ -d "$LEGAL_DIR" ] || die "no website checkout with src/lib/legal at $WEBSITE_DIR. Clone scanopy/website there, or set WEBSITE_DIR."
command -v gh >/dev/null 2>&1 || die "gh is not installed; it opens the website issue (brew install gh)"
gh auth status >/dev/null 2>&1 || die "gh is not logged in; run: gh auth login"

INTERACTIVE=false
[ -t 0 ] && INTERACTIVE=true
$INTERACTIVE && echo "Press Enter to accept the value in [brackets]."

# ask <prompt> <default>: the answer, or the default when there is no terminal
# or the answer is empty.
ask() {
    local answer=""
    if $INTERACTIVE; then
        read -r -p "$1 [$2]: " answer
    fi
    echo "${answer:-$2}"
}

today="$(date +%Y-%m-%d)"
latest="$(find "$TERMS_DIR" -maxdepth 1 -name '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9].md' -exec basename {} .md \; 2>/dev/null | sort | tail -1 || true)"

earliest="$(add_days "$today" "$MIN_NOTICE_DAYS")"
latest_allowed="$(add_days "$today" "$MAX_LEAD_DAYS")"
if [ -z "${EFFECTIVE:-}" ] && $INTERACTIVE; then
    echo "The notice goes out when the release deploys. Cut the release today: the effective date"
    echo "must be $earliest or later ($MIN_NOTICE_DAYS days' notice)."
fi
effective="${EFFECTIVE:-$(ask "Effective date (YYYY-MM-DD)" "$earliest")}"
is_date "$effective" || die "\`$effective\` is not a YYYY-MM-DD date"
[[ "$effective" < "$earliest" ]] && die "$effective is less than $MIN_NOTICE_DAYS days away; the earliest is $earliest"
[[ "$effective" > "$latest_allowed" ]] && die "$effective is more than $MAX_LEAD_DAYS days away; check the year"
[ -e "$TERMS_DIR/$effective.md" ] && die "terms/$effective.md already exists"
if [ -n "$latest" ] && ! [[ "$effective" > "$latest" ]]; then
    die "$effective is not after the latest update, terms/$latest.md; an update written today has to take effect after it"
fi

documents="${DOCUMENTS:-$(ask "Documents it changes, from: $DOCUMENT_IDS" "terms")}"
documents="$(echo "$documents" | tr ',' ' ' | xargs)"
[ -n "$documents" ] || die "name at least one document"
# The version in force of each document, copied as the draft of the new one.
drafts_from=()
for id in $documents; do
    [[ " $DOCUMENT_IDS " == *" $id "* ]] || die "unknown document \`$id\`; choose from: $DOCUMENT_IDS"
    [ -e "$LEGAL_DIR/$id/$effective.svelte" ] && die "the website already has src/lib/legal/$id/$effective.svelte"
    current="$(find "$LEGAL_DIR/$id" -maxdepth 1 -name '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9].svelte' 2>/dev/null | sort | tail -1 || true)"
    [ -n "$current" ] || die "the website has no version of $id under src/lib/legal/$id"
    drafts_from+=("$current")
done

# The terms file, then the website drafts.
written=()
mkdir -p "$TERMS_DIR"
file="$TERMS_DIR/$effective.md"
{
    echo "---"
    echo "effective: $effective"
    echo "date: $today"
    echo "documents: ${documents// /, }"
    echo "---"
    echo ""
    for id in $documents; do
        echo "- $(document_name "$id"), section N: $PLACEHOLDER"
    done
} > "$file"
written+=("backend/release-notes/terms/$effective.md")

i=0
for id in $documents; do
    cp "${drafts_from[$i]}" "$LEGAL_DIR/$id/$effective.svelte"
    written+=("$WEBSITE_DIR/src/lib/legal/$id/$effective.svelte")
    i=$((i + 1))
done

echo "Created:"
printf '  %s\n' "${written[@]}"

# The issue to publish the website half.
# "Terms of Service", "Terms of Service and Privacy Policy", "A, B and C".
names=""
# shellcheck disable=SC2086 # one argument per document id
set -- $documents
while [ $# -gt 0 ]; do
    name="$(document_name "$1")"
    shift
    if [ -z "$names" ]; then
        names="$name"
    elif [ $# -eq 0 ]; then
        names="$names and $name"
    else
        names="$names, $name"
    fi
done
body="$(mktemp)"
trap 'rm -f "$body"' EXIT
{
    echo "Scanopy Cloud emails notice of this update when the release carrying \`backend/release-notes/terms/$effective.md\` deploys. The email links to the pages below, and Prepare Release fails until they are on \`main\`."
    echo ""
    for id in $documents; do
        echo "- [ ] Edit \`src/lib/legal/$id/$effective.svelte\` (drafted from the version in force) into the new $(document_name "$id"). It goes live at scanopy.net/$id/$effective."
    done
    echo "- [ ] Commit and push to \`main\` before cutting the release."
    echo ""
    echo "Nothing to do on $effective itself: \`legal-effective-rebuild.yml\` rebuilds the site that day, and the live pages switch to the new versions."
} > "$body"
if ! issue="$(gh issue create -R scanopy/website --assignee @me \
    --title "Publish updated $names (effective $effective)" --body-file "$body")"; then
    die "could not open the website issue. These files were written and are kept: ${written[*]}"
fi
echo "Opened $issue"

if $INTERACTIVE && [ -n "${EDITOR:-}" ]; then
    read -r -p "Open the terms file in $EDITOR to write what changed? [Y/n]: " open
    if [[ ! "$open" =~ ^[Nn] ]]; then
        "$EDITOR" "$file"
    fi
fi

if grep -qF "$PLACEHOLDER" "$file"; then
    echo "Replace each \"$PLACEHOLDER\" in the terms file with what changes, in plain terms. CI fails until you do."
fi
echo "Check the terms file: cd backend && cargo test --lib every_embedded_terms_file"
