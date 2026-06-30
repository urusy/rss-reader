#!/usr/bin/env bash
# Integration test for semantic clustering (feature 26). recluster is trigram-only
# (no Anthropic). Seeds 3 near-duplicate + 1 unrelated article, reclusters, and
# asserts a cluster of size>=3 appears. Requires: running stack (:8081), docker, jq.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
DB_SVC="${DB_SVC:-db}"
PGUSER="${POSTGRES_USER:-rss}"
PGDB="${POSTGRES_DB:-rssreader}"
F="00000000-0000-0000-0000-0000000000c1"

psql() { docker compose exec -T "$DB_SVC" psql -v ON_ERROR_STOP=1 -U "$PGUSER" -d "$PGDB" "$@"; }
cleanup() { psql -q -c "DELETE FROM feeds WHERE id='$F';" >/dev/null 2>&1 || true; }
fail() { echo "FAIL: $1"; cleanup; exit 1; }
trap cleanup EXIT

psql -q <<SQL || { echo "FAIL: seed"; exit 1; }
DELETE FROM feeds WHERE id='$F';
INSERT INTO feeds (id, url, title) VALUES ('$F','https://c.test/f.xml','Cluster Feed');
INSERT INTO articles (id, feed_id, url, title, content, published_at) VALUES
  (gen_random_uuid(),'$F','https://c.test/1','Central bank raises interest rates today','', now()),
  (gen_random_uuid(),'$F','https://c.test/2','Central bank raises interest rates again','', now()),
  (gen_random_uuid(),'$F','https://c.test/3','Central bank raises interest rates sharply','', now()),
  (gen_random_uuid(),'$F','https://c.test/4','Local football team wins championship','', now());
SQL

n="$(curl -s -m 20 -X POST "$BASE/api/clusters/recluster" | jq -r '.clusters')"
[ -n "$n" ] && [ "$n" != "null" ] || fail "recluster returned no count"

# clusters list is an array; expect at least one cluster of size>=3
curl -s -m 10 "$BASE/api/clusters" | jq -e 'type=="array" and any(.[]; .size>=3)' >/dev/null \
  || fail "expected a cluster of size>=3"

# summary gate: no key → 503 (article exists path: pick any cluster id)
cid="$(curl -s -m 10 "$BASE/api/clusters" | jq -r '.[0].id')"
sc="$(curl -s -m 10 -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' \
  -d '{}' "$BASE/api/clusters/$cid/summary")"
case "$sc" in 503|200) ;; *) fail "summary expected 503/200, got $sc" ;; esac

echo "PASS: /api/clusters recluster groups near-dupes (clusters=$n, summary=$sc)"
