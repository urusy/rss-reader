#!/usr/bin/env bash
# Integration test for smart views (feature 27). Seeds 2 articles, creates a view
# with a text filter, resolves it, asserts only the matching article comes back.
# Requires: running stack (nginx :8081), docker compose, jq.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
CT='Content-Type: application/json'
DB_SVC="${DB_SVC:-db}"
PGUSER="${POSTGRES_USER:-rss}"
PGDB="${POSTGRES_DB:-rssreader}"
F="00000000-0000-0000-0000-0000000000a1"

psql() { docker compose exec -T "$DB_SVC" psql -v ON_ERROR_STOP=1 -U "$PGUSER" -d "$PGDB" "$@"; }
cleanup() {
  psql -q -c "DELETE FROM feeds WHERE id='$F';" >/dev/null 2>&1 || true
  psql -q -c "DELETE FROM saved_views WHERE name='view-smoke-test';" >/dev/null 2>&1 || true
}
fail() { echo "FAIL: $1"; cleanup; exit 1; }
trap cleanup EXIT

psql -q <<SQL || { echo "FAIL: seed"; exit 1; }
DELETE FROM feeds WHERE id='$F';
INSERT INTO feeds (id, url, title) VALUES ('$F','https://v.test/f.xml','View Feed');
INSERT INTO articles (id, feed_id, url, title, content) VALUES
  (gen_random_uuid(),'$F','https://v.test/1','Learning Rust ownership','about rust'),
  (gen_random_uuid(),'$F','https://v.test/2','Cooking pasta tonight','food');
SQL

# empty query -> 400
[ "$(curl -s -m 10 -o /dev/null -w '%{http_code}' -X POST -H "$CT" -d '{"name":"empty","query":{}}' "$BASE/api/saved-views")" = "400" ] \
  || fail "empty query should be 400"

# create a view with text filter
id="$(curl -s -m 10 -X POST -H "$CT" \
  -d '{"name":"view-smoke-test","query":{"text":"rust"}}' "$BASE/api/saved-views" | jq -r '.id')"
[ -n "$id" ] && [ "$id" != "null" ] || fail "create view returned no id"

# duplicate name -> 400
[ "$(curl -s -m 10 -o /dev/null -w '%{http_code}' -X POST -H "$CT" \
  -d '{"name":"view-smoke-test","query":{"text":"x"}}' "$BASE/api/saved-views")" = "400" ] \
  || fail "duplicate name should be 400"

# resolve: only the rust article
res="$(curl -s -m 10 "$BASE/api/saved-views/$id/articles")"
echo "$res" | jq -e 'any(.[]; .url=="https://v.test/1")' >/dev/null || fail "rust article missing"
echo "$res" | jq -e 'all(.[]; .url != "https://v.test/2")' >/dev/null || fail "pasta should be filtered out"

# list includes it
curl -s -m 10 "$BASE/api/saved-views" | jq -e --arg id "$id" 'any(.[]; .id==$id)' >/dev/null \
  || fail "list missing the view"

# delete + idempotency
[ "$(curl -s -m 10 -o /dev/null -w '%{http_code}' -X DELETE "$BASE/api/saved-views/$id")" = "204" ] \
  || fail "delete should be 204"

echo "PASS: /api/saved-views CRUD + resolve (text filter) + dup/empty 400"
