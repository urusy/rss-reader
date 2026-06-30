#!/usr/bin/env bash
# Integration test for on-demand full-content extraction (feature 13).
# Verifies wiring without hitting external sites:
#   1) POST /api/articles/<nonexistent>/extract -> 404 (slice merged & routed)
#   2) GET /api/articles -> every Article JSON carries full_content/extracted_at
#      (response-contract extension; values may be null)
# Requires: running stack (nginx :8081), jq.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
fail() { echo "FAIL: $1"; exit 1; }

# --- case 1: extract on a missing article id is a 404 ---
MISSING="00000000-0000-0000-0000-000000000000"
out="$(curl -s -m 5 -o /dev/null -w '%{http_code}' -X POST \
  "$BASE/api/articles/$MISSING/extract")"
[ "$out" = "404" ] || fail "extract on missing id: expected 404, got $out"

# --- case 2: Article response contract includes the new columns ---
json="$(curl -s -m 5 "$BASE/api/articles")"
echo "$json" | jq -e 'type == "array"' >/dev/null \
  || fail "GET /api/articles did not return an array: $json"

# Empty list is acceptable (fresh DB); only assert keys when rows exist.
count="$(echo "$json" | jq 'length')"
if [ "$count" -gt 0 ]; then
  echo "$json" | jq -e 'all(.[]; has("full_content") and has("extracted_at"))' \
    >/dev/null || fail "Article JSON missing full_content/extracted_at keys"
fi

echo "PASS: /api/articles/{id}/extract (404 on missing) + Article contract (full_content/extracted_at)"
