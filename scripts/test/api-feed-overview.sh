#!/usr/bin/env bash
# Integration test for feed_overview: seeds deterministic rows via psql, then
# asserts the COMPUTED VALUES of GET /api/feeds/overview (not just key presence).
# Requires: running stack (nginx :8081), docker compose, jq.
set -uo pipefail

URL="${URL:-http://localhost:8081/api/feeds/overview}"
DB_SVC="${DB_SVC:-db}"
PGUSER="${POSTGRES_USER:-rss}"
PGDB="${POSTGRES_DB:-rssreader}"
A="00000000-0000-0000-0000-0000000000aa"
B="00000000-0000-0000-0000-0000000000bb"

psql() { docker compose exec -T "$DB_SVC" psql -v ON_ERROR_STOP=1 -U "$PGUSER" -d "$PGDB" "$@"; }
cleanup() { psql -q -c "DELETE FROM feeds WHERE id IN ('$A','$B');" >/dev/null 2>&1 || true; }
fail() { echo "FAIL: $1"; cleanup; exit 1; }
trap cleanup EXIT

# --- seed (idempotent) ---
psql -q <<SQL || { echo "FAIL: seed"; exit 1; }
DELETE FROM feeds WHERE id IN ('$A','$B');
INSERT INTO feeds (id, url, title) VALUES
  ('$A', 'https://example.test/feed-a.xml', 'feed A'),
  ('$B', 'https://example.test/feed-b.xml', 'feed B');
INSERT INTO articles (id, feed_id, url, title, published_at, is_read) VALUES
  (gen_random_uuid(), '$A', 'https://example.test/a1', 'a1', now() - interval '1 day',  false),
  (gen_random_uuid(), '$A', 'https://example.test/a2', 'a2', now() - interval '5 days', false),
  (gen_random_uuid(), '$A', 'https://example.test/a3', 'a3', now() - interval '40 days', true),
  (gen_random_uuid(), '$A', 'https://example.test/a4', 'a4', NULL,                       true);
SQL

# --- fetch ---
body="$(curl -s -m 5 -w '\n%{http_code}' "$URL")"
code="${body##*$'\n'}"; json="${body%$'\n'*}"
[ "$code" = "200" ] || fail "expected 200, got $code ($json)"
case "$json" in "["*) : ;; *) fail "not a JSON array: $json";; esac

# --- assert feed A computed values ---
echo "$json" | jq -e --arg id "$A" '
  (map(select(.feed_id==$id)) | first) as $a
  | $a != null
    and $a.total_count == 4
    and $a.unread_count == 2
    and $a.last_published_at != null
    and ((($a.posts_per_week * 10) | round) == 5)   # 0.5
' >/dev/null || fail "feed A aggregates wrong: $(echo "$json" | jq -c --arg id "$A" 'map(select(.feed_id==$id))')"

# --- assert feed B (zero-article feed STILL returns a row) ---
echo "$json" | jq -e --arg id "$B" '
  (map(select(.feed_id==$id)) | first) as $b
  | $b != null
    and $b.total_count == 0
    and $b.unread_count == 0
    and $b.last_published_at == null
    and ((($b.posts_per_week * 10) | round) == 0)   # 0.0
' >/dev/null || fail "feed B zero-row wrong: $(echo "$json" | jq -c --arg id "$B" 'map(select(.feed_id==$id))')"

echo "PASS: /api/feeds/overview computed values (feed A 4/2/0.5, feed B 0/0/null/0)"
