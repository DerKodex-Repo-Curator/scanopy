#!/usr/bin/env bash
# Refresh the vendored data files embedded into the server binary:
#   backend/assets/oui.csv                       IEEE MAC-address registry
#
# Curl-only file drop: all parsing happens at load time in
# backend/src/server/shared/oui.rs.
#
# Invoked by `make refresh-vendored-data` and by the release workflow
# (.github/workflows/release.yml, test-and-update-fixtures job), which
# commits the refreshed file back to the release branch after `cargo test`.
#
# Failure semantics: best-effort. A failed download or a result below its
# sanity floor keeps the committed copy and warns; the script only exits
# non-zero on environment errors (missing curl).
#
# Requires: curl.

set -u
cd "$(dirname "$0")/.." || exit 1

CURL=(curl -sfL --max-time 300 --retry 5 --retry-delay 70)
UA="ScanopyVendoredDataRefresh/1.0 (release workflow)"

command -v curl >/dev/null || { echo "ERROR: curl is required" >&2; exit 1; }

# install <tmp-file> <dest> <line-floor> — move tmp into place if it clears
# the sanity floor, else keep the committed copy.
install_if_sane() {
    local tmp="$1" dest="$2" floor="$3" lines
    lines=$(wc -l < "$tmp" | tr -d ' ')
    if [ "$lines" -ge "$floor" ]; then
        mv "$tmp" "$dest"
        echo "refreshed $dest ($lines lines)"
    else
        echo "WARN: $dest refresh has $lines lines (floor $floor) — keeping committed copy" >&2
        rm -f "$tmp"
    fi
}

# --- IEEE OUI registry (MAC-address vendor lookup) ---
# IEEE sits behind bot protection that instantly rejects default-UA curl from
# datacenter IPs (seen as a ~300ms failure on GitHub runners) — always send
# the UA, and report HTTP status + curl exit on failure so the release log
# says exactly why a refresh was skipped.
tmp=$(mktemp)
http_code=$("${CURL[@]}" -A "$UA" -o "$tmp" -w '%{http_code}' \
    "https://standards-oui.ieee.org/oui/oui.csv")
curl_exit=$?
if [ "$curl_exit" = 0 ] && head -1 "$tmp" | grep -q "Registry"; then
    install_if_sane "$tmp" assets/oui.csv 20000
elif [ "$curl_exit" != 0 ]; then
    echo "WARN: assets/oui.csv download failed (curl exit $curl_exit, HTTP ${http_code:-?}) — keeping committed copy" >&2
    rm -f "$tmp"
else
    echo "WARN: assets/oui.csv malformed (HTTP $http_code, starts: $(head -c 60 "$tmp" | tr -d '\n')) — keeping committed copy" >&2
    rm -f "$tmp"
fi

echo "done"
