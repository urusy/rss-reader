#!/usr/bin/env bash
# Smoke test for the instapaper slice (no live Instapaper calls).
# Runs against the running stack (nginx :8081). Requires: jq.
# Verifies: DELETE wiring, status (configured:false), and that read-later returns
# 503 (NotEnabled) when credentials are absent — before any external call.
set -uo pipefail
BASE="${1:-http://localhost:8081}"
pass=0; fail=0

req() {
  local method="$1" path="$2" data="${3:-}" out
  if [ -n "$data" ]; then
    out="$(curl -s -m8 -w $'\n%{http_code}' -X "$method" -H 'Content-Type: application/json' -d "$data" "$BASE$path")"
  else
    out="$(curl -s -m8 -w $'\n%{http_code}' -X "$method" "$BASE$path")"
  fi
  code="${out##*$'\n'}"; body="${out%$'\n'*}"
}
want() { if [ "$code" = "$2" ]; then echo "PASS: $1 ($code)"; pass=$((pass+1)); else echo "FAIL: $1 expected $2 got $code (body: $body)"; fail=$((fail+1)); fi; }

# Fix state: ensure credentials absent.
req DELETE /api/instapaper/credentials
want "DELETE credentials -> 204" 204

req GET /api/instapaper/status
want "GET status -> 200" 200
echo "$body" | jq -e '.configured == false' >/dev/null
if [ $? = 0 ]; then echo "PASS: status configured==false"; pass=$((pass+1)); else echo "FAIL: status configured!=false (body: $body)"; fail=$((fail+1)); fi

# read-later with no credentials -> 503 NotEnabled (gate checked before article lookup)
req POST /api/read-later '{"article_id":"00000000-0000-0000-0000-000000000000"}'
want "read-later without credentials -> 503" 503

echo "----"; echo "PASS=$pass FAIL=$fail"; [ "$fail" -eq 0 ]
