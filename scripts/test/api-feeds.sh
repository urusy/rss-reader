#!/usr/bin/env bash
# Integration test for feed management (機能01): PATCH rename/folder, refresh, overview.
# Runs against the running stack (nginx :8081). Requires: jq.
set -uo pipefail
BASE="${1:-http://localhost:8081}"
pass=0; fail=0
seeded=""

cleanup() { [ -n "$seeded" ] && curl -s -o /dev/null -X DELETE "$BASE/api/feeds/$seeded"; }
trap cleanup EXIT

req() {
  local m="$1" p="$2" d="${3:-}" out
  if [ -n "$d" ]; then out="$(curl -s -m8 -w $'\n%{http_code}' -X "$m" -H 'Content-Type: application/json' -d "$d" "$BASE$p")"
  else out="$(curl -s -m8 -w $'\n%{http_code}' -X "$m" "$BASE$p")"; fi
  code="${out##*$'\n'}"; body="${out%$'\n'*}"
}
want() { if [ "$code" = "$2" ]; then echo "PASS: $1 ($code)"; pass=$((pass+1)); else echo "FAIL: $1 expected $2 got $code (body: $body)"; fail=$((fail+1)); fi; }
want2() { if [ "$code" = "$2" ] || [ "$code" = "$3" ]; then echo "PASS: $1 ($code)"; pass=$((pass+1)); else echo "FAIL: $1 expected $2|$3 got $code (body: $body)"; fail=$((fail+1)); fi; }
assert() { if [ "$2" = "0" ]; then echo "PASS: $1"; pass=$((pass+1)); else echo "FAIL: $1 (body: $body)"; fail=$((fail+1)); fi; }

# seed a feed (stub URL; immediate fetch is best-effort so 201 regardless)
req POST /api/feeds "{\"url\":\"http://127.0.0.1:9/__feeds_test_$$.xml\"}"
want "seed feed" 201
seeded="$(echo "$body" | jq -r '.id')"

# A. overview shape
req GET /api/feeds/overview
want "A overview -> 200" 200
echo "$body" | jq -e --arg id "$seeded" 'any(.[]; .feed_id==$id and has("unread_count") and has("total_count"))' >/dev/null
assert "A overview has feed with unread_count/total_count" $?

# B. rename
req PATCH "/api/feeds/$seeded" '{"title":"renamed-by-test"}'
want "B rename -> 200" 200
[ "$(echo "$body" | jq -r '.title')" = "renamed-by-test" ]; assert "B title == renamed-by-test" $?

# C. unclassify (folder_id null; double-option)
req PATCH "/api/feeds/$seeded" '{"folder_id":null}'
want "C set folder null -> 200" 200
[ "$(echo "$body" | jq -r '.folder_id')" = "null" ]; assert "C folder_id == null" $?

# D. empty title rejected
req PATCH "/api/feeds/$seeded" '{"title":""}'
want "D empty title -> 400" 400

# E. patch nonexistent
req PATCH "/api/feeds/00000000-0000-0000-0000-000000000000" '{"title":"x"}'
want "E patch missing feed -> 404" 404

# F. refresh nonexistent -> 404
req POST "/api/feeds/00000000-0000-0000-0000-000000000000/refresh"
want "F refresh missing feed -> 404" 404

# G. refresh real (stub URL -> upstream fails => 502; route+handler exercised, not 404)
req POST "/api/feeds/$seeded/refresh"
want2 "G refresh real feed -> 200|502" 200 502

echo "----"; echo "PASS=$pass FAIL=$fail"; [ "$fail" -eq 0 ]
