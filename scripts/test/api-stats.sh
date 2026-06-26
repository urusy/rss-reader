#!/usr/bin/env bash
# Integration test for the stats slice: GET /api/stats must return 200 with
# JSON keys feeds/articles/unread. Run against the running stack (nginx :8081).
set -uo pipefail
URL="${1:-http://localhost:8081/api/stats}"
body="$(curl -s -m 5 -w '\n%{http_code}' "$URL")"
code="${body##*$'\n'}"
json="${body%$'\n'*}"
if [ "$code" != "200" ]; then
    echo "FAIL: expected HTTP 200, got $code (body: $json)"
    exit 1
fi
for key in feeds articles unread; do
    echo "$json" | grep -q "\"$key\"" || { echo "FAIL: missing key '$key' in $json"; exit 1; }
done
echo "PASS: /api/stats -> $json"
exit 0
