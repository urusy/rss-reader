#!/usr/bin/env bash
# Integration test for Read-on-Save settings (feature 16).
# Verifies the settings endpoint contract (Instapaper itself is not called).
# Requires: running stack (nginx :8081), jq.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
URL="$BASE/api/read-later/settings"
CT='Content-Type: application/json'
fail() { echo "FAIL: $1"; exit 1; }
code() { curl -s -m 5 -o /dev/null -w '%{http_code}' "$@"; }

# get returns the key
curl -s -m 5 "$URL" | jq -e 'has("mark_read_on_save")' >/dev/null \
  || fail "GET settings missing mark_read_on_save"

# put true -> echoes true
got="$(curl -s -m 5 -X PUT -H "$CT" -d '{"mark_read_on_save":true}' "$URL")"
echo "$got" | jq -e '.mark_read_on_save == true' >/dev/null \
  || fail "PUT true did not return true: $got"

# persisted
curl -s -m 5 "$URL" | jq -e '.mark_read_on_save == true' >/dev/null \
  || fail "setting did not persist"

# missing field -> 422
c="$(code -X PUT -H "$CT" -d '{}' "$URL")"
[ "$c" = "422" ] || fail "empty body should be 422, got $c"

# cleanup: back to false
curl -s -m 5 -X PUT -H "$CT" -d '{"mark_read_on_save":false}' "$URL" >/dev/null

echo "PASS: /api/read-later/settings (get/put/persist/422)"
