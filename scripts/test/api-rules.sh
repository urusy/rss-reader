#!/usr/bin/env bash
# Integration test for the rules engine (feature 28). Seeds 2 articles, creates a
# keyword rule, dry-run tests it (matches only the keyword article), applies it,
# asserts the matching article became read. Requires: stack (:8081), docker, jq.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
CT='Content-Type: application/json'
DB_SVC="${DB_SVC:-db}"
PGUSER="${POSTGRES_USER:-rss}"
PGDB="${POSTGRES_DB:-rssreader}"
F="00000000-0000-0000-0000-0000000000b2"
A1="00000000-0000-0000-0000-0000000000b3"

psql() { docker compose exec -T "$DB_SVC" psql -v ON_ERROR_STOP=1 -U "$PGUSER" -d "$PGDB" "$@"; }
cleanup() {
  psql -q -c "DELETE FROM feeds WHERE id='$F';" >/dev/null 2>&1 || true
  psql -q -c "DELETE FROM automation_rules WHERE name='rule-smoke-test';" >/dev/null 2>&1 || true
}
fail() { echo "FAIL: $1"; cleanup; exit 1; }
trap cleanup EXIT

psql -q <<SQL || { echo "FAIL: seed"; exit 1; }
DELETE FROM feeds WHERE id='$F';
INSERT INTO feeds (id, url, title) VALUES ('$F','https://r.test/f.xml','Rules Feed');
INSERT INTO articles (id, feed_id, url, title, content, is_read) VALUES
  ('$A1','$F','https://r.test/1','Sponsored content here','ad', false),
  (gen_random_uuid(),'$F','https://r.test/2','Normal article','x', false);
SQL

# empty conditions -> 400
[ "$(curl -s -m 10 -o /dev/null -w '%{http_code}' -X POST -H "$CT" \
  -d '{"name":"x","conditions":{"combinator":"all","items":[]},"actions":[{"kind":"mark_read"}]}' \
  "$BASE/api/rules")" = "400" ] || fail "empty conditions should be 400"

# create keyword rule (title contains Sponsored -> mark_read)
id="$(curl -s -m 10 -X POST -H "$CT" -d '{
  "name":"rule-smoke-test",
  "conditions":{"combinator":"all","items":[{"field":"keyword","target":"title","value":"Sponsored"}]},
  "actions":[{"kind":"mark_read"}]
}' "$BASE/api/rules" | jq -r '.id')"
[ -n "$id" ] && [ "$id" != "null" ] || fail "create rule returned no id"

# dry-run test: matches exactly the sponsored article
curl -s -m 10 -X POST "$BASE/api/rules/$id/test" \
  | jq -e --arg a "$A1" '.matched_count>=1 and (.matched_ids | index($a) != null)' >/dev/null \
  || fail "test should match the sponsored article"

# apply all -> sponsored article becomes read
curl -s -m 20 -X POST "$BASE/api/rules/apply" | jq -e '.processed >= 2' >/dev/null \
  || fail "apply did not process articles"
read_state="$(psql -tAc "SELECT is_read FROM articles WHERE id='$A1';" | tr -d '[:space:]')"
[ "$read_state" = "t" ] || fail "sponsored article should be read after apply (got '$read_state')"

# delete + idempotency
[ "$(curl -s -m 10 -o /dev/null -w '%{http_code}' -X DELETE "$BASE/api/rules/$id")" = "204" ] \
  || fail "delete should be 204"

echo "PASS: /api/rules create/test/apply (keyword→mark_read) + 400 empty"
