#!/usr/bin/env bash
# Integration test for auth/access-control (feature 14).
# Two modes depending on whether the running stack has AUTH_TOKEN set:
#   - AUTH_TOKEN provided (env): assert protection is enforced.
#   - AUTH_TOKEN unset: assert pass-through (auth disabled) + status reflects it.
# Requires: running stack (nginx :8081), jq.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
fail() { echo "FAIL: $1"; exit 1; }
code() { curl -s -m 5 -o /dev/null -w '%{http_code}' "$@"; }

# health is always public
[ "$(code "$BASE/api/health")" = "200" ] || fail "health should be 200 without auth"

# auth status is always public
st="$(curl -s -m 5 "$BASE/api/auth/status")"
echo "$st" | jq -e 'has("required")' >/dev/null || fail "auth/status missing 'required': $st"
required="$(echo "$st" | jq -r '.required')"

if [ -n "${AUTH_TOKEN:-}" ]; then
  [ "$required" = "true" ] || fail "AUTH_TOKEN set but status.required=$required"
  # protected route blocked without header
  [ "$(code "$BASE/api/feeds")" = "401" ] || fail "feeds should be 401 without token"
  # blocked with wrong token
  [ "$(code -H "Authorization: Bearer wrong" "$BASE/api/feeds")" = "401" ] \
    || fail "feeds should be 401 with wrong token"
  # allowed with correct token
  [ "$(code -H "Authorization: Bearer $AUTH_TOKEN" "$BASE/api/feeds")" = "200" ] \
    || fail "feeds should be 200 with correct token"
  # login: wrong -> 401, correct -> 200 (Json extractor needs application/json)
  CT='Content-Type: application/json'
  [ "$(code -X POST -H "$CT" "$BASE/api/auth/login" -d '{"token":"wrong"}')" = "401" ] \
    || fail "login wrong token should be 401"
  [ "$(code -X POST -H "$CT" "$BASE/api/auth/login" -d "{\"token\":\"$AUTH_TOKEN\"}")" = "200" ] \
    || fail "login correct token should be 200"
  echo "PASS: /api/auth (enabled): protection enforced, login verified"
else
  [ "$required" = "false" ] || fail "AUTH_TOKEN unset but status.required=$required"
  # pass-through: protected route reachable without a token
  [ "$(code "$BASE/api/feeds")" = "200" ] || fail "feeds should be 200 when auth disabled"
  echo "PASS: /api/auth (disabled): pass-through confirmed (set AUTH_TOKEN to test enforcement)"
fi
