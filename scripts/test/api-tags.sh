#!/usr/bin/env bash
# Integration test for tags (feature 24). Avoids calling Anthropic.
# Requires: running stack (nginx :8081), jq.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
CT='Content-Type: application/json'
ZERO="00000000-0000-0000-0000-000000000000"
fail() { echo "FAIL: $1"; exit 1; }
code() { curl -s -m 10 -o /dev/null -w '%{http_code}' "$@"; }

# create
id="$(curl -s -m 10 -X POST -H "$CT" -d '{"name":"test-rust"}' "$BASE/api/tags" | jq -r '.id')"
[ -n "$id" ] && [ "$id" != "null" ] || fail "create tag returned no id"

# case-insensitive upsert returns same id
id2="$(curl -s -m 10 -X POST -H "$CT" -d '{"name":"TEST-RUST"}' "$BASE/api/tags" | jq -r '.id')"
[ "$id" = "$id2" ] || fail "case-insensitive upsert returned different id ($id vs $id2)"

# empty name -> 400
[ "$(code -X POST -H "$CT" -d '{"name":"   "}' "$BASE/api/tags")" = "400" ] \
  || fail "empty tag name should be 400"

# list includes article_count
curl -s -m 10 "$BASE/api/tags" | jq -e 'type=="array" and (any(.[]; .id==$ID and has("article_count")))' \
  --arg ID "$id" >/dev/null || fail "list missing tag/article_count"

# patch rename
[ "$(code -X PATCH -H "$CT" -d '{"name":"test-rustlang"}' "$BASE/api/tags/$id")" = "200" ] \
  || fail "patch rename should be 200"

# article tags of a nonexistent article -> [] (200)
curl -s -m 10 "$BASE/api/articles/$ZERO/tags" | jq -e 'type=="array"' >/dev/null \
  || fail "article tags should be an array"

# suggest-tags without key -> 503 (gate, no Anthropic call). With key it'd be 404
# (article missing). Accept either so the smoke is key-agnostic.
c="$(code -X POST "$BASE/api/articles/$ZERO/suggest-tags")"
case "$c" in 503|404) ;; *) fail "suggest-tags expected 503/404, got $c" ;; esac

# delete + idempotency
[ "$(code -X DELETE "$BASE/api/tags/$id")" = "204" ] || fail "delete should be 204"
[ "$(code -X DELETE "$BASE/api/tags/$id")" = "404" ] || fail "re-delete should be 404"

echo "PASS: /api/tags CRUD + case-insensitive + article tags + suggest gate ($c)"
