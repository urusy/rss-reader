#!/usr/bin/env bash
# Integration test: folders CRUD + feeds PATCH(folder assign) + articles folder filter.
# Runs against the running stack (nginx :8081). Requires: jq.
# set -uo pipefail (NOT -e): assertions report and we exit non-zero at the end if any failed.
set -uo pipefail
BASE="${1:-http://localhost:8081}"

pass=0
fail=0
created_folder=""
seeded_feed=""

cleanup() {
  [ -n "$created_folder" ] && curl -s -o /dev/null -X DELETE "$BASE/api/folders/$created_folder"
  [ -n "$seeded_feed" ] && curl -s -o /dev/null -X DELETE "$BASE/api/feeds/$seeded_feed"
}
trap cleanup EXIT

# req METHOD PATH [JSON]  -> sets globals: code, body
req() {
  local method="$1" path="$2" data="${3:-}" out
  if [ -n "$data" ]; then
    out="$(curl -s -m 8 -w $'\n%{http_code}' -X "$method" \
      -H 'Content-Type: application/json' -d "$data" "$BASE$path")"
  else
    out="$(curl -s -m 8 -w $'\n%{http_code}' -X "$method" "$BASE$path")"
  fi
  code="${out##*$'\n'}"
  body="${out%$'\n'*}"
}

want() { # want DESC EXPECTED_CODE
  if [ "$code" = "$2" ]; then echo "PASS: $1 ($code)"; pass=$((pass+1));
  else echo "FAIL: $1 — expected $2 got $code (body: $body)"; fail=$((fail+1)); fi
}

assert() { # assert DESC CONDITION_RESULT(0=ok)
  if [ "$2" = "0" ]; then echo "PASS: $1"; pass=$((pass+1));
  else echo "FAIL: $1 (body: $body)"; fail=$((fail+1)); fi
}

# --- seed a feed to assign (stub URL; fetch is best-effort so 201 regardless) ---
req POST /api/feeds "{\"url\":\"http://127.0.0.1:9/__test_$$.xml\"}"
want "seed feed" 201
seeded_feed="$(echo "$body" | jq -r '.id')"

# A. create folder
req POST /api/folders '{"name":"_t_folder"}'
want "A create folder" 201
created_folder="$(echo "$body" | jq -r '.id')"
[ "$(echo "$body" | jq -r '.name')" = "_t_folder" ]; assert "A name == _t_folder" $?

# B. list contains it
req GET /api/folders
want "B list folders" 200
echo "$body" | jq -e --arg id "$created_folder" 'any(.[]; .id == $id)' >/dev/null; assert "B list contains folder" $?

# C. rename
req PATCH "/api/folders/$created_folder" '{"name":"_t_folder2"}'
want "C rename" 200
[ "$(echo "$body" | jq -r '.name')" = "_t_folder2" ]; assert "C name == _t_folder2" $?

# D. assign feed to folder
req PATCH "/api/feeds/$seeded_feed" "{\"folder_id\":\"$created_folder\"}"
want "D assign feed" 200
[ "$(echo "$body" | jq -r '.folder_id')" = "$created_folder" ]; assert "D folder_id == created" $?

# E. unclassify
req PATCH "/api/feeds/$seeded_feed" '{"folder_id":null}'
want "E unclassify feed" 200
[ "$(echo "$body" | jq -r '.folder_id')" = "null" ]; assert "E folder_id == null" $?

# F. folder filter (re-assign, then filter articles)
req PATCH "/api/feeds/$seeded_feed" "{\"folder_id\":\"$created_folder\"}"
req GET "/api/articles?folder_id=$created_folder"
want "F articles by folder" 200

# G. delete folder -> feed back to unclassified (SET NULL)
req DELETE "/api/folders/$created_folder"
want "G delete folder" 204
created_folder=""  # already deleted; avoid double-delete in cleanup
req GET /api/feeds
echo "$body" | jq -e --arg id "$seeded_feed" 'any(.[]; .id == $id and .folder_id == null)' >/dev/null
assert "G feed unclassified after folder delete (SET NULL)" $?

# H. error cases
req POST /api/folders '{"name":"   "}'
want "H1 whitespace name -> 400" 400
req PATCH "/api/feeds/$seeded_feed" '{"folder_id":"00000000-0000-0000-0000-000000000000"}'
want "H2 assign nonexistent folder -> 400" 400
req PATCH "/api/folders/00000000-0000-0000-0000-000000000000" '{"name":"x"}'
want "H3 patch missing folder -> 404" 404
req DELETE "/api/folders/00000000-0000-0000-0000-000000000000"
want "H4 delete missing folder -> 404" 404

echo "----"
echo "PASS=$pass FAIL=$fail"
[ "$fail" -eq 0 ]
