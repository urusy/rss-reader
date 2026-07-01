#!/usr/bin/env bash
# Integration test for per-operation LLM model + prompt settings (feature
# llm_settings). Verifies GET defaults, PUT override round-trip, empty-clears-
# override, and 400 on an invalid model id. No DB seed needed (singleton row).
# Requires: stack (:8081), jq. Restores defaults on exit.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
CT='Content-Type: application/json'
URL="$BASE/api/settings/llm"

code() { curl -s -m 10 -o /dev/null -w '%{http_code}' "$@"; }
clear_all() {
  curl -s -m 10 -X PUT -H "$CT" \
    -d '{"summarize_model":"","summarize_prompt":"","translate_model":"","translate_prompt":""}' \
    "$URL" >/dev/null 2>&1 || true
}
fail() { echo "FAIL: $1"; clear_all; exit 1; }
trap clear_all EXIT

# start from a clean slate
clear_all

# GET: overrides null, default_model present
curl -s -m 10 "$URL" | jq -e \
  '.summarize_model==null and .translate_prompt==null and (.default_model|length>0)
   and (.default_summarize_prompt|length>0)' >/dev/null \
  || fail "GET defaults should have null overrides + non-empty defaults"

# PUT override: summarize model + prompt
curl -s -m 10 -X PUT -H "$CT" \
  -d '{"summarize_model":"claude-opus-4-8","summarize_prompt":"Summarize in {lang}.","translate_model":"","translate_prompt":""}' \
  "$URL" | jq -e '.summarize_model=="claude-opus-4-8"' >/dev/null \
  || fail "PUT should echo the saved summarize_model"

# GET reflects the override; translate still null
curl -s -m 10 "$URL" | jq -e \
  '.summarize_model=="claude-opus-4-8" and .summarize_prompt=="Summarize in {lang}."
   and .translate_model==null' >/dev/null \
  || fail "GET should reflect the summarize override and leave translate null"

# PUT empty clears the override back to null
curl -s -m 10 -X PUT -H "$CT" -d '{"summarize_model":""}' "$URL" \
  | jq -e '.summarize_model==null' >/dev/null \
  || fail "empty summarize_model should clear the override"

# translate override persists INDEPENDENTLY of summarize (guards column/positional
# bind swaps in upsert + resolve_translate reading the wrong field)
curl -s -m 10 -X PUT -H "$CT" \
  -d '{"summarize_model":"","summarize_prompt":"","translate_model":"claude-haiku-4-5-20251001","translate_prompt":"Translate to {lang}."}' \
  "$URL" | jq -e '.translate_model=="claude-haiku-4-5-20251001"' >/dev/null \
  || fail "PUT should echo the saved translate_model"
curl -s -m 10 "$URL" | jq -e \
  '.translate_model=="claude-haiku-4-5-20251001" and .translate_prompt=="Translate to {lang}."
   and .summarize_model==null and .summarize_prompt==null' >/dev/null \
  || fail "translate override should persist while summarize stays null"

# invalid model id (has a slash / space) -> 400
[ "$(code -X PUT -H "$CT" -d '{"summarize_model":"claude opus/4.8"}' "$URL")" = "400" ] \
  || fail "invalid model id should be 400"

echo "PASS: /api/settings/llm defaults + override round-trip + clear + 400 gate"
clear_all
trap - EXIT
