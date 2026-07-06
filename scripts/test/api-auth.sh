#!/usr/bin/env bash
# Integration test for password login / cookie sessions (feature: login).
# Modes depending on the running stack's state:
#   - setup_required=true : SETUP_PASSWORD が必要（初期セットアップから一周検証）。
#   - setup_required=false: AUTH_PASSWORD が必要（既存パスワードでログイン検証）。
# いずれも最後に logout するので、実運用インスタンスに残るセッションは無い。
# 注意: 誤パスワード試行を1回行う（グローバルバックオフは5連続失敗から）。
# Requires: running stack (nginx :8081), jq, curl.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
JAR="$(mktemp)"
trap 'rm -f "$JAR"' EXIT

fail() { echo "FAIL: $1"; exit 1; }
code() { curl -s -m 10 -o /dev/null -w '%{http_code}' "$@"; }
CT='Content-Type: application/json'

# health is always public
[ "$(code "$BASE/api/health")" = "200" ] || fail "health should be 200 without auth"

# status is always public and exposes setup_required/authenticated
st="$(curl -s -m 5 "$BASE/api/auth/status")"
echo "$st" | jq -e 'has("setup_required") and has("authenticated")' >/dev/null \
  || fail "auth/status shape unexpected: $st"
setup_required="$(echo "$st" | jq -r '.setup_required')"

# protected routes are 401 without a cookie (secure by default, even pre-setup)
[ "$(code "$BASE/api/feeds")" = "401" ] || fail "feeds should be 401 without session"

if [ "$setup_required" = "true" ]; then
  [ -n "${SETUP_PASSWORD:-}" ] || fail "setup_required=true: set SETUP_PASSWORD to test setup"
  # short password is rejected
  [ "$(code -X POST -H "$CT" "$BASE/api/auth/setup" -d '{"password":"short"}')" = "400" ] \
    || fail "setup with short password should be 400"
  # setup succeeds and logs in (Set-Cookie)
  [ "$(code -c "$JAR" -X POST -H "$CT" "$BASE/api/auth/setup" \
      -d "{\"password\":\"$SETUP_PASSWORD\"}")" = "200" ] || fail "setup should be 200"
  # second setup is rejected
  [ "$(code -X POST -H "$CT" "$BASE/api/auth/setup" -d '{"password":"whatever123"}')" = "409" ] \
    || fail "second setup should be 409"
else
  [ -n "${AUTH_PASSWORD:-}" ] || fail "setup done: set AUTH_PASSWORD to test login"
  # wrong password -> 401 (counts 1 towards the global backoff; threshold is 5)
  [ "$(code -X POST -H "$CT" "$BASE/api/auth/login" -d '{"password":"wrong-password"}')" = "401" ] \
    || fail "login with wrong password should be 401"
  # correct password -> 200 + Set-Cookie
  [ "$(code -c "$JAR" -X POST -H "$CT" "$BASE/api/auth/login" \
      -d "{\"password\":\"$AUTH_PASSWORD\"}")" = "200" ] || fail "login should be 200"
fi

# cookie is HttpOnly + SameSite=Strict (attributes live in the jar file)
grep -q "rss_session" "$JAR" || fail "session cookie missing from jar"
grep -qi "HttpOnly" "$JAR" || fail "session cookie should be HttpOnly"

# with the session cookie: protected route ok, status reflects authentication
[ "$(code -b "$JAR" "$BASE/api/feeds")" = "200" ] || fail "feeds should be 200 with session"
auth_now="$(curl -s -m 5 -b "$JAR" "$BASE/api/auth/status" | jq -r '.authenticated')"
[ "$auth_now" = "true" ] || fail "status.authenticated should be true with session"

# sessions list shows the current session
cur="$(curl -s -m 5 -b "$JAR" "$BASE/api/auth/sessions" | jq -r '[.[] | select(.current)] | length')"
[ "$cur" = "1" ] || fail "exactly one current session expected, got: $cur"

# CSRF: state-changing request with a foreign Origin is rejected before auth
[ "$(code -b "$JAR" -X POST -H "Origin: http://evil.example" \
    "$BASE/api/auth/logout")" = "403" ] || fail "cross-origin POST should be 403"

# logout kills the session
[ "$(code -b "$JAR" -c "$JAR" -X POST "$BASE/api/auth/logout")" = "204" ] \
  || fail "logout should be 204"
[ "$(code -b "$JAR" "$BASE/api/feeds")" = "401" ] || fail "feeds should be 401 after logout"

echo "PASS: /api/auth password login, cookie session, CSRF guard, logout verified"
