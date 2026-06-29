#!/usr/bin/env bash
# Smoke test for read-later (機能06). Runs against the running stack (nginx :8081).
# Requires: jq, docker compose. Seeds a real article via psql; deletes credentials
# to assert the 503 (NotEnabled) gate. Does NOT call the live Instapaper API.
set -uo pipefail
BASE="${BASE:-http://localhost:8081}"
DB_SVC="${DB_SVC:-db}"
PGUSER="${POSTGRES_USER:-rss}"
PGDB="${POSTGRES_DB:-rssreader}"
F="00000000-0000-0000-0000-00000000fe01"
A="00000000-0000-0000-0000-00000000fe02"
pass=0; fail=0

psql() { docker compose exec -T "$DB_SVC" psql -v ON_ERROR_STOP=1 -U "$PGUSER" -d "$PGDB" "$@"; }
cleanup() { psql -q -c "DELETE FROM feeds WHERE id='$F';" >/dev/null 2>&1 || true; }
trap cleanup EXIT
req() {
  local m="$1" p="$2" d="${3:-}" out
  if [ -n "$d" ]; then out="$(curl -s -m8 -w $'\n%{http_code}' -X "$m" -H 'Content-Type: application/json' -d "$d" "$BASE$p")"
  else out="$(curl -s -m8 -w $'\n%{http_code}' -X "$m" "$BASE$p")"; fi
  code="${out##*$'\n'}"; body="${out%$'\n'*}"
}
want() { if [ "$code" = "$2" ]; then echo "PASS: $1 ($code)"; pass=$((pass+1)); else echo "FAIL: $1 expected $2 got $code (body: $body)"; fail=$((fail+1)); fi; }

# Ensure no credentials (so the gate is exercised) and seed a real article.
curl -s -o /dev/null -X DELETE "$BASE/api/instapaper/credentials"
psql -q <<SQL
DELETE FROM feeds WHERE id='$F';
INSERT INTO feeds (id,url,title) VALUES ('$F','https://example.test/fe','feed FE');
INSERT INTO articles (id,feed_id,url,title) VALUES ('$A','$F','https://example.test/fe1','fe1');
SQL

# 1. nonexistent article -> 404 (article existence checked before the credential gate)
req POST /api/read-later '{"article_id":"00000000-0000-0000-0000-0000000000ff"}'
want "1 nonexistent article -> 404" 404

# 2. real article, no credentials -> 503 NotEnabled
req POST /api/read-later "{\"article_id\":\"$A\"}"
want "2 real article without credentials -> 503" 503

# 3. get read-later for unsaved article -> 404
req GET "/api/read-later/$A"
want "3 unsaved article get -> 404" 404

# 4. list -> 200 array
req GET /api/read-later
want "4 list -> 200" 200
case "$body" in "["*) echo "PASS: 4 list is array"; pass=$((pass+1));; *) echo "FAIL: 4 not array"; fail=$((fail+1));; esac

echo "----"; echo "PASS=$pass FAIL=$fail"; [ "$fail" -eq 0 ]
