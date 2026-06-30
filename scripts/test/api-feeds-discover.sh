#!/usr/bin/env bash
# Contract/error tests for feed autodiscovery (POST /api/feeds/discover).
# 正常系の HTML 解析は backend の #[cfg(test)] が担保。ここは契約と異常系のみ。
# Requires: running stack (nginx :8081), curl.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
URL="$BASE/api/feeds/discover"
fail() { echo "FAIL: $1"; exit 1; }
code() { curl -s -m 15 -o /dev/null -w '%{http_code}' -H 'Content-Type: application/json' "$@"; }

# invalid scheme -> 400
[ "$(code -d '{"url":"not-a-url"}' "$URL")" = "400" ] || fail "invalid scheme should be 400"
# missing url key -> 422
[ "$(code -d '{}' "$URL")" = "422" ] || fail "missing url should be 422"
# unreachable host -> 502
[ "$(code -d '{"url":"http://127.0.0.1:1/nope"}' "$URL")" = "502" ] \
  || fail "unreachable host should be 502"

echo "PASS: /api/feeds/discover contract (400 invalid, 422 missing, 502 unreachable)"
