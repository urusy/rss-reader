#!/usr/bin/env bash
# Integration test for mute_rules: seeds articles, creates rules, applies, and
# asserts the ACTUAL filtering of GET /api/articles.
# Requires: running stack (nginx :8081), docker compose, jq.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
DB_SVC="${DB_SVC:-db}"
PGUSER="${POSTGRES_USER:-rss}"
PGDB="${POSTGRES_DB:-rssreader}"
M="00000000-0000-0000-0000-0000000000cc"

psql() { docker compose exec -T "$DB_SVC" psql -v ON_ERROR_STOP=1 -U "$PGUSER" -d "$PGDB" "$@"; }
cleanup() {
  psql -q -c "DELETE FROM feeds WHERE id='$M';" >/dev/null 2>&1 || true
  psql -q -c "DELETE FROM mute_rules WHERE pattern IN ('Sponsored','ad.example.com');" >/dev/null 2>&1 || true
}
fail() { echo "FAIL: $1"; cleanup; exit 1; }
trap cleanup EXIT

psql -q <<SQL || { echo "FAIL: seed"; exit 1; }
DELETE FROM feeds WHERE id='$M';
INSERT INTO feeds (id, url, title) VALUES ('$M', 'https://news.test/feed.xml', 'feed M');
INSERT INTO articles (id, feed_id, url, title, content, is_read) VALUES
  (gen_random_uuid(), '$M', 'https://news.test/m1',      'Sponsored: 新商品', '', false),
  (gen_random_uuid(), '$M', 'https://ad.example.com/m2', '今日の天気',        '', false),
  (gen_random_uuid(), '$M', 'https://news.test/m3',      '通常記事',          '', false);
SQL

curl -s -m 5 -X POST "$BASE/api/mute-rules" -H 'Content-Type: application/json' \
  -d '{"field":"title","pattern":"Sponsored","action":"hide"}' | jq -e '.id' >/dev/null \
  || fail "create hide rule"
curl -s -m 5 -X POST "$BASE/api/mute-rules" -H 'Content-Type: application/json' \
  -d '{"field":"url","pattern":"ad.example.com","action":"mark_read"}' | jq -e '.id' >/dev/null \
  || fail "create mark_read rule"

# create_rule applies immediately, so a follow-up apply re-counts hide (reset+
# reapply) but finds m2 already read → marked_read=0 here. mark_read's real
# effect is asserted via the filtering checks below.
curl -s -m 5 -X POST "$BASE/api/mute-rules/apply" \
  | jq -e '.rules_evaluated >= 2 and .hidden >= 1' >/dev/null || fail "apply report"

list="$(curl -s -m 5 "$BASE/api/articles?feed_id=$M")"
echo "$list" | jq -e 'all(.[]; .url != "https://news.test/m1")' >/dev/null \
  || fail "m1 should be hidden"
echo "$list" | jq -e 'any(.[]; .url=="https://ad.example.com/m2" and .is_read==true)' >/dev/null \
  || fail "m2 should be present and read"
echo "$list" | jq -e 'any(.[]; .url=="https://news.test/m3" and .is_read==false)' >/dev/null \
  || fail "m3 should be present and unread"

curl -s -m 5 "$BASE/api/articles?feed_id=$M&include_muted=true" \
  | jq -e 'any(.[]; .url=="https://news.test/m1")' >/dev/null \
  || fail "m1 should reappear with include_muted=true"

curl -s -m 5 "$BASE/api/articles?feed_id=$M&unread=true" \
  | jq -e 'map(.url) == ["https://news.test/m3"]' >/dev/null \
  || fail "unread should be only m3"

echo "PASS: mute_rules hide/mark_read filtering"
