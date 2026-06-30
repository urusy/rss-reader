#!/usr/bin/env bash
# Integration test for OPML import/export (feature 17).
# Verifies export wiring, import summary, idempotency, and 400 on bad/empty XML.
# Requires: running stack (nginx :8081), jq. Cleans up the test feed at the end.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
fail() { echo "FAIL: $1"; exit 1; }
code() { curl -s -m 10 -o /dev/null -w '%{http_code}' "$@"; }

# export returns OPML
hdr="$(curl -s -m 10 -D - -o /tmp/feeds.opml "$BASE/api/opml/export")"
echo "$hdr" | grep -qi 'opml' || fail "export content-type not opml"
grep -q '<opml' /tmp/feeds.opml || fail "export body has no <opml"

TESTURL="https://opml-smoke.test/$(date +%s).xml"
OPML="<opml version=\"2.0\"><body><outline text=\"OPMLTest\"><outline type=\"rss\" text=\"t\" xmlUrl=\"$TESTURL\"/></outline></body></opml>"

before="$(curl -s -m 10 "$BASE/api/feeds" | jq 'length')"
sum="$(curl -s -m 10 -X POST -H 'Content-Type: application/xml' -d "$OPML" "$BASE/api/opml/import")"
echo "$sum" | jq -e '.imported_feeds >= 1' >/dev/null || fail "import summary wrong: $sum"
after="$(curl -s -m 10 "$BASE/api/feeds" | jq 'length')"
[ "$after" -gt "$before" ] || fail "feed count did not grow ($before -> $after)"

# idempotent re-import: feed count unchanged
curl -s -m 10 -X POST -H 'Content-Type: application/xml' -d "$OPML" "$BASE/api/opml/import" >/dev/null
again="$(curl -s -m 10 "$BASE/api/feeds" | jq 'length')"
[ "$again" = "$after" ] || fail "re-import changed feed count ($after -> $again)"

# bad XML -> 400, empty -> 400
[ "$(code -X POST -H 'Content-Type: application/xml' -d '<opml><body><outline' "$BASE/api/opml/import")" = "400" ] \
  || fail "malformed XML should be 400"
[ "$(code -X POST -H 'Content-Type: application/xml' -d '' "$BASE/api/opml/import")" = "400" ] \
  || fail "empty body should be 400"

# cleanup: delete the test feed
fid="$(curl -s -m 10 "$BASE/api/feeds" | jq -r --arg u "$TESTURL" '.[] | select(.url==$u) | .id')"
[ -n "$fid" ] && curl -s -m 10 -X DELETE "$BASE/api/feeds/$fid" >/dev/null

echo "PASS: /api/opml (export, import summary, idempotent, 400 bad/empty)"
