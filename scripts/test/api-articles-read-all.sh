#!/usr/bin/env bash
# Integration test: POST /api/articles/read-all (bulk mark-read).
# Runs against the running stack (nginx :8081). Requires: jq, docker compose.
#
# ⚠️ DESTRUCTIVE: this test mutates articles.is_read. Case #4 (no feed_id) marks
# the ENTIRE DB's unread to zero. Do NOT run against a DB you actually read from.
# The whole-DB case only runs when RUN_DESTRUCTIVE=1. Seeds are re-applied before
# each case (ON CONFLICT ... is_read=false) so cases are order-independent.
set -uo pipefail
BASE="${BASE:-http://localhost:8081}"
DB_SVC="${DB_SVC:-db}"
PGUSER="${POSTGRES_USER:-rss}"
PGDB="${POSTGRES_DB:-rssreader}"
A="00000000-0000-0000-0000-0000000000a1"
B="00000000-0000-0000-0000-0000000000b1"

pass=0; fail=0
psql() { docker compose exec -T "$DB_SVC" psql -v ON_ERROR_STOP=1 -U "$PGUSER" -d "$PGDB" "$@"; }
cleanup() { psql -q -c "DELETE FROM feeds WHERE id IN ('$A','$B');" >/dev/null 2>&1 || true; }
trap cleanup EXIT

seed() { # re-seed feeds A,B each with 2 UNREAD articles (idempotent reset to unread)
  psql -q <<SQL
INSERT INTO feeds (id,url,title) VALUES
  ('$A','https://example.test/feed-a','feed A'),
  ('$B','https://example.test/feed-b','feed B')
ON CONFLICT (url) DO NOTHING;
INSERT INTO articles (id,feed_id,url,title,is_read) VALUES
  (gen_random_uuid(),'$A','https://example.test/a1','a1',false),
  (gen_random_uuid(),'$A','https://example.test/a2','a2',false),
  (gen_random_uuid(),'$B','https://example.test/b1','b1',false),
  (gen_random_uuid(),'$B','https://example.test/b2','b2',false)
ON CONFLICT (url) DO UPDATE SET is_read=false;
SQL
}
post() { curl -s -m8 -w '\n%{http_code}' -X POST -H 'Content-Type: application/json' -d "$2" "$BASE$1"; }
want() { if [ "$code" = "$2" ]; then echo "PASS: $1 ($code)"; pass=$((pass+1)); else echo "FAIL: $1 expected $2 got $code"; fail=$((fail+1)); fi; }
unread_of() { curl -s -m8 "$BASE/api/articles?feed_id=$1&unread=true" | jq 'length'; }

# 1. endpoint exists + 204 (feed-scoped so routine runs don't mark OTHER feeds read;
#    the global empty-body `{}` path is exercised only under RUN_DESTRUCTIVE in case 4)
seed
out="$(post /api/articles/read-all "{\"feed_id\":\"$A\"}")"; code="${out##*$'\n'}"
want "1 read-all (feed A) -> 204" 204

# 2. per-feed scope (mark A read, leave B)
seed
out="$(post /api/articles/read-all "{\"feed_id\":\"$A\"}")"; code="${out##*$'\n'}"
want "2 read-all feed A -> 204" 204
[ "$(unread_of "$A")" = "0" ]; if [ $? = 0 ]; then echo "PASS: 2 feed A unread==0"; pass=$((pass+1)); else echo "FAIL: 2 feed A unread!=0"; fail=$((fail+1)); fi
[ "$(unread_of "$B")" != "0" ]; if [ $? = 0 ]; then echo "PASS: 2 feed B still unread"; pass=$((pass+1)); else echo "FAIL: 2 feed B got marked"; fail=$((fail+1)); fi

# 3. idempotent
out="$(post /api/articles/read-all "{\"feed_id\":\"$A\"}")"; code="${out##*$'\n'}"
want "3 idempotent re-run -> 204" 204

# 4. whole-DB (destructive, guarded)
if [ "${RUN_DESTRUCTIVE:-0}" = "1" ]; then
  seed
  out="$(post /api/articles/read-all '{}')"; code="${out##*$'\n'}"
  want "4 read-all all -> 204" 204
  u="$(curl -s -m8 "$BASE/api/stats" | jq '.unread')"
  [ "$u" = "0" ]; if [ $? = 0 ]; then echo "PASS: 4 global unread==0"; pass=$((pass+1)); else echo "FAIL: 4 global unread=$u"; fail=$((fail+1)); fi
else
  echo "SKIP: 4 whole-DB case (set RUN_DESTRUCTIVE=1 to run)"
fi

echo "----"; echo "PASS=$pass FAIL=$fail"; [ "$fail" -eq 0 ]
