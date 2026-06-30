#!/usr/bin/env bash
# Integration test for stars + highlights (feature 32). Seeds 1 article, toggles
# a star (idempotent PUT, appears in /api/stars, DELETE removes it), creates a
# highlight (empty quote -> 400), patches its note, lists, deletes (404 after).
# Requires: stack (:8081), docker, jq.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
CT='Content-Type: application/json'
DB_SVC="${DB_SVC:-db}"
PGUSER="${POSTGRES_USER:-rss}"
PGDB="${POSTGRES_DB:-rssreader}"
F="00000000-0000-0000-0000-0000000000c1"
A1="00000000-0000-0000-0000-0000000000c2"
MISSING="00000000-0000-0000-0000-0000000000cf"

psql() { docker compose exec -T "$DB_SVC" psql -v ON_ERROR_STOP=1 -U "$PGUSER" -d "$PGDB" "$@"; }
cleanup() { psql -q -c "DELETE FROM feeds WHERE id='$F';" >/dev/null 2>&1 || true; }
fail() { echo "FAIL: $1"; cleanup; exit 1; }
trap cleanup EXIT

psql -q <<SQL || { echo "FAIL: seed"; exit 1; }
DELETE FROM feeds WHERE id='$F';
INSERT INTO feeds (id, url, title) VALUES ('$F','https://a.test/f.xml','Anno Feed');
INSERT INTO articles (id, feed_id, url, title, content) VALUES
  ('$A1','$F','https://a.test/1','Annotated','Some body text to highlight');
SQL

code() { curl -s -m 10 -o /dev/null -w '%{http_code}' "$@"; }

# star a non-existent article -> 404
[ "$(code -X PUT "$BASE/api/articles/$MISSING/star")" = "404" ] || fail "star missing should be 404"

# PUT star is idempotent -> 204 twice
[ "$(code -X PUT "$BASE/api/articles/$A1/star")" = "204" ] || fail "star PUT #1 should be 204"
[ "$(code -X PUT "$BASE/api/articles/$A1/star")" = "204" ] || fail "star PUT #2 should be 204"

# appears in /api/stars
curl -s -m 10 "$BASE/api/stars" | jq -e --arg a "$A1" 'index($a) != null' >/dev/null \
  || fail "starred id should appear in /api/stars"

# unstar -> 204, then gone from list
[ "$(code -X DELETE "$BASE/api/articles/$A1/star")" = "204" ] || fail "unstar should be 204"
curl -s -m 10 "$BASE/api/stars" | jq -e --arg a "$A1" 'index($a) == null' >/dev/null \
  || fail "unstarred id should be gone from /api/stars"

# empty quote -> 400
[ "$(code -X POST -H "$CT" -d '{"quote":"   "}' "$BASE/api/articles/$A1/highlights")" = "400" ] \
  || fail "empty quote should be 400"

# create highlight -> 201, capture id
hid="$(curl -s -m 10 -X POST -H "$CT" \
  -d '{"quote":"body text","note":"important","start_offset":5,"end_offset":14}' \
  "$BASE/api/articles/$A1/highlights" | jq -r '.id')"
[ -n "$hid" ] && [ "$hid" != "null" ] || fail "create highlight returned no id"

# list -> contains it with the note
curl -s -m 10 "$BASE/api/articles/$A1/highlights" \
  | jq -e --arg h "$hid" 'map(select(.id==$h)) | .[0].note=="important"' >/dev/null \
  || fail "highlight should be listed with its note"

# patch note (color left untouched) -> 200
curl -s -m 10 -X PATCH -H "$CT" -d '{"note":"revised"}' "$BASE/api/highlights/$hid" \
  | jq -e '.note=="revised"' >/dev/null || fail "patch note should update"

# delete -> 204, then patching again -> 404
[ "$(code -X DELETE "$BASE/api/highlights/$hid")" = "204" ] || fail "delete highlight should be 204"
[ "$(code -X PATCH -H "$CT" -d '{"note":"x"}' "$BASE/api/highlights/$hid")" = "404" ] \
  || fail "patch deleted highlight should be 404"

echo "PASS: /api/{stars,highlights} star toggle idempotent + highlight CRUD + 400/404 gates"
cleanup
trap - EXIT
