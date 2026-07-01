#!/usr/bin/env bash
# Integration test for AI daily digest (feature 23). Avoids calling Anthropic.
# Requires: running stack (nginx :8081), jq.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
fail() { echo "FAIL: $1"; exit 1; }
code() { curl -s -m 10 -o /dev/null -w '%{http_code}' "$@"; }

# bad date -> 400
[ "$(code "$BASE/api/digest?date=not-a-date")" = "400" ] || fail "bad date should be 400"

# latest: 200 or 404, both acceptable
c="$(code "$BASE/api/digest/latest")"
case "$c" in 200|404) ;; *) fail "latest expected 200/404, got $c" ;; esac

# refresh without key -> 503 (gate before material)
[ "$(code -X POST "$BASE/api/digest/refresh")" = "503" ] \
  || fail "refresh without ANTHROPIC_API_KEY should be 503"

echo "PASS: /api/digest (400 bad date, latest=$c, refresh gate 503)"
