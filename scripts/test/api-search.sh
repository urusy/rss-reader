#!/usr/bin/env bash
# Integration test for full-text search: seeds deterministic articles (JP + EN)
# via psql, then asserts GET /api/search?q=… returns the right matches by id.
# Proves pg_trgm handles Japanese substrings AND that content (not just title)
# is searched. Requires: running stack (nginx :8081), docker compose, jq.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
URL="$BASE/api/search"
DB_SVC="${DB_SVC:-db}"
PGUSER="${POSTGRES_USER:-rss}"
PGDB="${POSTGRES_DB:-rssreader}"

FEED="00000000-0000-0000-0000-0000000005f0"
ML="00000000-0000-0000-0000-0000000005a1"   # 日本語タイトル
RS="00000000-0000-0000-0000-0000000005a2"   # 英語タイトル
NO="00000000-0000-0000-0000-0000000005a3"   # どちらにも一致しない記事

psql() { docker compose exec -T "$DB_SVC" psql -v ON_ERROR_STOP=1 -U "$PGUSER" -d "$PGDB" "$@"; }
cleanup() { psql -q -c "DELETE FROM feeds WHERE id = '$FEED';" >/dev/null 2>&1 || true; }
fail() { echo "FAIL: $1"; cleanup; exit 1; }
trap cleanup EXIT

# --- seed (idempotent) ---
psql -q <<SQL || { echo "FAIL: seed"; exit 1; }
DELETE FROM feeds WHERE id = '$FEED';
INSERT INTO feeds (id, url, title) VALUES
  ('$FEED', 'https://example.test/search-feed.xml', 'search test feed');
INSERT INTO articles (id, feed_id, url, title, content, published_at) VALUES
  ('$ML', '$FEED', 'https://example.test/s-ml', '機械学習の最新動向',
     '深層学習とニューラルネットワークの解説', now()),
  ('$RS', '$FEED', 'https://example.test/s-rs', 'Rust async runtime guide',
     'tokio executor and futures', now()),
  ('$NO', '$FEED', 'https://example.test/s-no', '天気予報',
     'sunny tomorrow', now());
SQL

# raw "<query>" -> echoes "<json-body>\n<http-code>". Split in the caller so
# the code/body stay in this shell (command substitution runs in a subshell).
raw() { curl -s -m 5 -G -w '\n%{http_code}' --data-urlencode "q=$1" "$URL"; }

# --- case 1: Japanese title substring hits (pg_trgm tokenizer-free) ---
out="$(raw '機械学習')"; code="${out##*$'\n'}"; json="${out%$'\n'*}"
[ "$code" = "200" ] || fail "JP title: expected 200, got $code ($json)"
echo "$json" | jq -e --arg ml "$ML" --arg rs "$RS" --arg no "$NO" '
  any(.[]; .id==$ml) and all(.[]; .id!=$rs) and all(.[]; .id!=$no)
' >/dev/null || fail "JP title '機械学習' should match only the ML article: $(echo "$json" | jq -c 'map(.id)')"

# --- case 2: English title substring hits ---
out="$(raw 'async')"; code="${out##*$'\n'}"; json="${out%$'\n'*}"
[ "$code" = "200" ] || fail "EN title: expected 200, got $code ($json)"
echo "$json" | jq -e --arg rs "$RS" --arg ml "$ML" '
  any(.[]; .id==$rs) and all(.[]; .id!=$ml)
' >/dev/null || fail "EN title 'async' should match only the RS article: $(echo "$json" | jq -c 'map(.id)')"

# --- case 3: content (not just title) is searched ---
out="$(raw 'ニューラルネットワーク')"; code="${out##*$'\n'}"; json="${out%$'\n'*}"
[ "$code" = "200" ] || fail "content match: expected 200, got $code ($json)"
echo "$json" | jq -e --arg ml "$ML" 'any(.[]; .id==$ml)' >/dev/null \
  || fail "content term should match the ML article via content column: $(echo "$json" | jq -c 'map(.id)')"

# --- case 4: blank query is a 400 (no %%-matches-everything scan) ---
out="$(raw '   ')"; code="${out##*$'\n'}"; json="${out%$'\n'*}"
[ "$code" = "400" ] || fail "blank query should be 400, got $code ($json)"

echo "PASS: /api/search (JP title, EN title, content match, blank=400)"
