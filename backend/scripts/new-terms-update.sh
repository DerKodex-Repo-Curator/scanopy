#!/usr/bin/env bash
# Create backend/release-notes/terms/<effective>.md, the file that makes Scanopy
# Cloud email notice of a terms update. Asks for the effective date and the
# documents, prefilling each, and applies the
# same date rules as `check_dates` in src/server/release_notes/terms.rs so a
# bad date is caught here rather than in CI.
#
# Answers can also come from the environment (EFFECTIVE, DOCUMENTS); without a terminal the prefilled values are used as they are.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TERMS_DIR="$SCRIPT_DIR/../release-notes/terms"

MIN_NOTICE_DAYS=30
MAX_LEAD_DAYS=180
# The notice goes out when the release carrying the file deploys, so the
# prefilled effective date adds this many days to the minimum notice for that
# release to be cut and rolled out.
SHIP_DAYS=15
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

INTERACTIVE=false
[ -t 0 ] && INTERACTIVE=true

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
mkdir -p "$TERMS_DIR"
latest="$(find "$TERMS_DIR" -maxdepth 1 -name '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9].md' -exec basename {} .md \; | sort | tail -1)"

earliest="$(add_days "$today" "$MIN_NOTICE_DAYS")"
latest_allowed="$(add_days "$today" "$MAX_LEAD_DAYS")"
effective="${EFFECTIVE:-$(ask "Day the update takes effect (YYYY-MM-DD, $earliest at the earliest; the notice goes out when the release deploys)" "$(add_days "$earliest" "$SHIP_DAYS")")}"
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
for id in $documents; do
    [[ " $DOCUMENT_IDS " == *" $id "* ]] || die "unknown document \`$id\`; choose from: $DOCUMENT_IDS"
done

document_name() {
    case "$1" in
        terms) echo "Terms of Service" ;;
        privacy) echo "Privacy Policy" ;;
        refund) echo "Refund Policy" ;;
        security) echo "Security page" ;;
    esac
}

file="$TERMS_DIR/$effective.md"
{
    echo "---"
    echo "effective: $effective"
    echo "date: $today"
    echo "documents: $(echo "$documents" | sed 's/ /, /g')"
    echo "---"
    echo ""
    for id in $documents; do
        echo "- $(document_name "$id"), section N: $PLACEHOLDER"
    done
} > "$file"

echo "Created backend/release-notes/terms/$effective.md"

if $INTERACTIVE && [ -n "${EDITOR:-}" ]; then
    read -r -p "Open it in $EDITOR to write what changed? [Y/n]: " open
    if [[ ! "$open" =~ ^[Nn] ]]; then
        "$EDITOR" "$file"
    fi
fi

if grep -qF "$PLACEHOLDER" "$file"; then
    echo "Replace each \"$PLACEHOLDER\" with what changes, in plain terms. CI fails until you do."
fi
echo "Set each changed page's effective date on the website to $effective."
echo "Check it with: cd backend && cargo test --lib every_embedded_terms_file"
