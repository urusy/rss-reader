#!/usr/bin/env bash
# Integration test for feed_health: seeds health columns via psql, asserts the
# computed classification of GET /api/feeds/health.
# Requires: running stack (nginx :8081), docker compose, jq.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
URL="$BASE/api/feeds/health"
DB_SVC="${DB_SVC:-db}"
PGUSER="${POSTGRES_USER:-rss}"
PGDB="${POSTGRES_DB:-rssreader}"
DEAD="00000000-0000-0000-0000-0000000000dd"
STALE="00000000-0000-0000-0000-000000000055"
HEALTHY="00000000-0000-0000-0000-0000000000ab"
NEVER="00000000-0000-0000-0000-0000000000ee"

psql() { docker compose exec -T "$DB_SVC" psql -v ON_ERROR_STOP=1 -U "$PGUSER" -d "$PGDB" "$@"; }
cleanup() { psql -q -c "DELETE FROM feeds WHERE id IN ('$DEAD','$STALE','$HEALTHY','$NEVER');" >/dev/null 2>&1 || true; }
fail() { echo "FAIL: $1"; cleanup; exit 1; }
trap cleanup EXIT

psql -q <<SQL || { echo "FAIL: seed"; exit 1; }
DELETE FROM feeds WHERE id IN ('$DEAD','$STALE','$HEALTHY','$NEVER');
INSERT INTO feeds (id, url, title, last_fetch_status, last_error, consecutive_failures, last_fetch_attempted_at) VALUES
  ('$DEAD',    'https://example.test/dead.xml',    'dead',    'error', 'boom', 5, now()),
  ('$STALE',   'https://example.test/stale.xml',   'stale',   'ok',    NULL,   0, now()),
  ('$HEALTHY', 'https://example.test/healthy.xml', 'healthy', 'ok',    NULL,   0, now()),
  ('$NEVER',   'https://example.test/never.xml',   'never',   'ok',    NULL,   0, now());
INSERT INTO articles (id, feed_id, url, title, content, published_at, is_read) VALUES
  (gen_random_uuid(), '$DEAD',    'https://example.test/d1', 'd1', '', now() - interval '1 day',  false),
  (gen_random_uuid(), '$STALE',   'https://example.test/s1', 's1', '', now() - interval '40 days', false),
  (gen_random_uuid(), '$HEALTHY', 'https://example.test/h1', 'h1', '', now() - interval '2 days',  false);
SQL

body="$(curl -s -m 5 -w '\n%{http_code}' "$URL")"
code="${body##*$'\n'}"; json="${body%$'\n'*}"
[ "$code" = "200" ] || fail "expected 200, got $code ($json)"

assert_health() {
  echo "$json" | jq -e --arg id "$1" --arg h "$2" \
    '(map(select(.feed_id==$id)) | first) as $r | $r != null and $r.health == $h' \
    >/dev/null || fail "feed $1 expected health=$2"
}
assert_health "$DEAD" "dead"
assert_health "$STALE" "stale"
assert_health "$HEALTHY" "healthy"
assert_health "$NEVER" "stale"

echo "$json" | jq -e --arg id "$DEAD" \
  '(map(select(.feed_id==$id)) | first) as $r
   | $r.consecutive_failures == 5 and $r.last_fetch_status == "error" and $r.last_error == "boom"' \
  >/dev/null || fail "dead feed raw fields wrong"

echo "PASS: /api/feeds/health classification + raw fields"
