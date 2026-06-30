#!/usr/bin/env bash
# Integration test for backup/restore (feature 15).
# Modes by whether BACKUP_TOKEN is set on the running stack:
#   - unset: GET export without token -> 503 (feature gated).
#   - set (export BACKUP_TOKEN=...): wrong token -> 400, correct -> 200 NDJSON,
#     self re-import is idempotent (stats unchanged).
# Requires: running stack (nginx :8081), jq.
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"
fail() { echo "FAIL: $1"; exit 1; }
code() { curl -s -m 10 -o /dev/null -w '%{http_code}' "$@"; }

if [ -z "${BACKUP_TOKEN:-}" ]; then
  c="$(code "$BASE/api/backup/export")"
  [ "$c" = "503" ] || fail "export without BACKUP_TOKEN should be 503, got $c"
  echo "PASS: /api/backup (disabled): 503 gate confirmed (set BACKUP_TOKEN to test fully)"
  exit 0
fi

T="$BACKUP_TOKEN"
# wrong token -> 400
c="$(code -H "X-Backup-Token: wrong" "$BASE/api/backup/export")"
[ "$c" = "400" ] || fail "export with wrong token should be 400, got $c"

# correct token -> 200 + NDJSON, first line is meta
hdr="$(curl -s -m 10 -D - -o /tmp/bk.ndjson -H "X-Backup-Token: $T" "$BASE/api/backup/export")"
echo "$hdr" | grep -qi 'application/x-ndjson' || fail "export content-type not ndjson"
head -1 /tmp/bk.ndjson | grep -q '"kind":"meta"' || fail "first line is not meta"

before="$(curl -s -m 10 "$BASE/api/stats" | jq '.articles')"
# self re-import is idempotent
sum="$(curl -s -m 30 -X POST -H "X-Backup-Token: $T" \
  -H 'Content-Type: application/x-ndjson' --data-binary @/tmp/bk.ndjson \
  "$BASE/api/backup/import")"
echo "$sum" | jq -e 'has("articles")' >/dev/null || fail "import response missing 'articles': $sum"
after="$(curl -s -m 10 "$BASE/api/stats" | jq '.articles')"
[ "$before" = "$after" ] || fail "self re-import changed article count ($before -> $after)"

# runs endpoint returns an array
curl -s -m 10 -H "X-Backup-Token: $T" "$BASE/api/backup/runs" | jq -e 'type=="array"' \
  >/dev/null || fail "runs did not return an array"

echo "PASS: /api/backup (enabled): export NDJSON, idempotent self-import, runs[]"
