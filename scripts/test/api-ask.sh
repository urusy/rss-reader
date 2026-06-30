#!/usr/bin/env bash
# Integration test for Ask Claude (feature 22). Avoids calling Anthropic.
# GET /notes is external-free (always 200). POST /ask expectation depends on
# whether ANTHROPIC_API_KEY is set on the stack (gate is checked first):
#   - key unset: 503 (NotEnabled) before validation.
#   - key set:   400 (validation) for a bad conversation.
# Requires: running stack (nginx :8081), jq.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
CT='Content-Type: application/json'
ZERO="00000000-0000-0000-0000-000000000000"
fail() { echo "FAIL: $1"; exit 1; }
code() { curl -s -m 10 -o /dev/null -w '%{http_code}' "$@"; }

# notes: external-free, always 200 with a messages array
curl -s -m 10 "$BASE/api/articles/$ZERO/notes" | jq -e 'has("messages")' >/dev/null \
  || fail "GET notes missing messages array"

# ask with empty messages: 503 (key unset) or 400 (key set) — both acceptable
c="$(code -X POST -H "$CT" -d '{"messages":[]}' "$BASE/api/articles/$ZERO/ask")"
case "$c" in
  503|400) ;;
  *) fail "ask empty messages expected 503/400, got $c" ;;
esac

echo "PASS: /api/articles ask+notes wiring (ask=$c)"
