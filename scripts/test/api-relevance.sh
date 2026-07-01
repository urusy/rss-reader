#!/usr/bin/env bash
# Integration test for AI relevance scoring (feature 25). Avoids calling Anthropic.
# Requires: running stack (nginx :8081), jq.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
fail() { echo "FAIL: $1"; exit 1; }
code() { curl -s -m 10 -o /dev/null -w '%{http_code}' "$@"; }

# scores: read model, no LLM → 200 array
curl -s -m 10 "$BASE/api/relevance/scores" | jq -e 'type=="array"' >/dev/null \
  || fail "scores should be an array"

# profile: no LLM → 200 with fields
curl -s -m 10 "$BASE/api/relevance/profile" \
  | jq -e 'has("profile") and has("hash") and has("tag_count")' >/dev/null \
  || fail "profile missing fields"

# score: LLM gate → 503 without key
[ "$(code -X POST "$BASE/api/relevance/score")" = "503" ] \
  || fail "score without ANTHROPIC_API_KEY should be 503"

echo "PASS: /api/relevance (scores[], profile, score gate 503)"
