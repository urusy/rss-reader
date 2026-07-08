#!/usr/bin/env bash
# Integration smoke for the Google Reader compatible sync API (feature 29).
# Requires: running stack (nginx :8081) with SYNC_API_ENABLED=true, jq, curl,
# and AUTH_PASSWORD (the login password; ClientLogin uses the same credential).
# 副作用: テスト用トークンを発行し最後に（Cookie ログインできれば）失効する。
# 記事の既読状態を1件だけ 既読→未読 と往復させ、元の状態（未読）に戻す。
set -uo pipefail

BASE="${BASE:-${1:-http://localhost:8081}}"

fail() { echo "FAIL: $1"; exit 1; }
code() { curl -s -m 15 -o /dev/null -w '%{http_code}' "$@"; }

[ -n "${AUTH_PASSWORD:-}" ] || fail "set AUTH_PASSWORD (ClientLogin uses the login password)"

# ---- ClientLogin -----------------------------------------------------------

# GET は 405（Passwd がクエリ文字列に載る経路を塞いでいる）
[ "$(code "$BASE/accounts/ClientLogin")" = "405" ] || fail "GET ClientLogin should be 405"

# 誤パスワード → 403 Error=BadAuthentication（グローバルバックオフ1回分を消費）
wrong="$(curl -s -m 10 -w '\n%{http_code}' "$BASE/accounts/ClientLogin" \
  --data-urlencode "Email=smoke-test" --data-urlencode "Passwd=wrong-password-xx")"
[ "$(echo "$wrong" | tail -1)" = "403" ] || fail "wrong password should be 403"
echo "$wrong" | grep -q "Error=BadAuthentication" || fail "403 body should be Error=BadAuthentication"

# 正しいパスワード → 200、Auth= 行からトークンを得る
resp="$(curl -s -m 10 "$BASE/accounts/ClientLogin" \
  --data-urlencode "Email=smoke-test" --data-urlencode "Passwd=$AUTH_PASSWORD")"
TOKEN="$(echo "$resp" | sed -n 's/^Auth=//p')"
[ -n "$TOKEN" ] || fail "ClientLogin should return Auth= line, got: $resp"
echo "$resp" | grep -q "^SID=$TOKEN$" || fail "SID should equal Auth"
echo "$resp" | grep -q "^LSID=null$" || fail "LSID=null literal expected"
AH="Authorization: GoogleLogin auth=$TOKEN"

# ---- 認証境界 ----------------------------------------------------------------

# ヘッダ無し → 401 + 両綴りの Bad-Token ヘッダ
h="$(curl -s -m 10 -D - -o /dev/null "$BASE/reader/api/0/tag/list")"
echo "$h" | grep -q "^HTTP/1.1 401" || fail "no auth should be 401"
echo "$h" | grep -qi "^Google-Bad-Token: true" || fail "Google-Bad-Token header missing"
echo "$h" | grep -qi "^X-Reader-Google-Bad-Token: true" || fail "X-Reader-Google-Bad-Token header missing"

# 未知パスも未認証なら 401（catch-all が認証の内側にある証明）
[ "$(code "$BASE/reader/api/0/definitely-not-implemented")" = "401" ] \
  || fail "unknown path without auth should be 401"

# 認証済みの未知パスは 200 []（プローブ吸収）
probe="$(curl -s -m 10 -H "$AH" "$BASE/reader/api/0/definitely-not-implemented")"
[ "$probe" = "[]" ] || fail "authenticated unknown path should return [], got: $probe"

# ---- 読み取り面 ----------------------------------------------------------------

# token: 提示トークンをそのまま返す
t="$(curl -s -m 10 -H "$AH" "$BASE/reader/api/0/token")"
[ "$t" = "$TOKEN" ] || fail "token endpoint should echo the auth token"

# user-info: 全値文字列の固定形
ui="$(curl -s -m 10 -H "$AH" "$BASE/reader/api/0/user-info")"
echo "$ui" | jq -e '.userId == "1" and .userName == "reader"' >/dev/null \
  || fail "user-info shape unexpected: $ui"

# tag/list: starred 行は必ずある
tl="$(curl -s -m 10 -H "$AH" "$BASE/reader/api/0/tag/list")"
echo "$tl" | jq -e '.tags | map(.id) | index("user/-/state/com.google/starred") != null' >/dev/null \
  || fail "tag/list must contain starred: $tl"

# subscription/list: url と categories が全行にある
sl="$(curl -s -m 10 -H "$AH" "$BASE/reader/api/0/subscription/list")"
echo "$sl" | jq -e '.subscriptions | all(has("url") and has("categories") and has("htmlUrl"))' >/dev/null \
  || fail "subscription/list shape unexpected"

# stream/items/ids（未読・n=5）
ids="$(curl -s -m 10 -H "$AH" \
  "$BASE/reader/api/0/stream/items/ids?s=user/-/state/com.google/reading-list&xt=user/-/state/com.google/read&n=5")"
echo "$ids" | jq -e 'has("itemRefs")' >/dev/null || fail "items/ids shape unexpected: $ids"
first_id="$(echo "$ids" | jq -r '.itemRefs[0].id // empty')"

# unread-count: max とreading-list 合計行の一致
uc="$(curl -s -m 10 -H "$AH" "$BASE/reader/api/0/unread-count")"
echo "$uc" | jq -e '.max == (.unreadcounts[] | select(.id == "user/-/state/com.google/reading-list") | .count)' >/dev/null \
  || fail "unread-count max should equal reading-list row"
unread_before="$(echo "$uc" | jq -r '.max')"

if [ -n "$first_id" ]; then
  # stream/items/contents: 返った短形式 id をそのまま i= に（decimal 形の受理）
  contents="$(curl -s -m 10 -H "$AH" "$BASE/reader/api/0/stream/items/contents" -d "i=$first_id")"
  long_id="$(echo "$contents" | jq -r '.items[0].id // empty')"
  echo "$long_id" | grep -q "^tag:google.com,2005:reader/item/" || fail "long-form item id expected: $long_id"
  echo "$contents" | jq -e '.items[0] | (.crawlTimeMsec | type == "string") and (.published | type == "number")' >/dev/null \
    || fail "items/contents timestamp types unexpected"

  # edit-tag 既読化: 応答は literal OK（text/plain）
  et="$(curl -s -m 10 -D /tmp/greader-headers.$$ -H "$AH" "$BASE/reader/api/0/edit-tag" \
    --data-urlencode "i=$long_id" --data-urlencode "a=user/-/state/com.google/read")"
  [ "$et" = "OK" ] || fail "edit-tag should answer literal OK, got: $et"
  grep -qi "content-type: text/plain" /tmp/greader-headers.$$ || fail "edit-tag should be text/plain"
  rm -f /tmp/greader-headers.$$

  # 未読数が減った（GReader 経由の書き込みが数に反映 = UI と同じ真実を共有）
  unread_after="$(curl -s -m 10 -H "$AH" "$BASE/reader/api/0/unread-count" | jq -r '.max')"
  [ "$unread_after" -lt "$unread_before" ] || fail "unread count should drop after edit-tag ($unread_before -> $unread_after)"

  # 未読へ戻す（実運用インスタンスの状態を変えない）
  [ "$(curl -s -m 10 -H "$AH" "$BASE/reader/api/0/edit-tag" \
      --data-urlencode "i=$long_id" --data-urlencode "r=user/-/state/com.google/read")" = "OK" ] \
    || fail "edit-tag unread rollback failed"
else
  echo "  (no unread articles; skipping contents/edit-tag round-trip)"
fi

# ---- 後始末: Cookie ログインしてテスト用トークンを失効 --------------------------

JAR="$(mktemp)"
trap 'rm -f "$JAR"' EXIT
if [ "$(code -c "$JAR" -X POST -H 'Content-Type: application/json' \
    "$BASE/api/auth/login" -d "{\"password\":\"$AUTH_PASSWORD\"}")" = "200" ]; then
  tok_id="$(curl -s -m 10 -b "$JAR" "$BASE/api/sync/tokens" \
    | jq -r '[.[] | select(.label == "smoke-test")][0].id // empty')"
  if [ -n "$tok_id" ]; then
    [ "$(code -b "$JAR" -X DELETE "$BASE/api/sync/tokens/$tok_id")" = "204" ] \
      || fail "token revoke should be 204"
    # 失効済みトークンは 401 に戻る
    [ "$(code -H "$AH" "$BASE/reader/api/0/tag/list")" = "401" ] \
      || fail "revoked token should be 401"
  fi
  curl -s -m 10 -b "$JAR" -X POST "$BASE/api/auth/logout" -o /dev/null
fi

echo "PASS: GReader ClientLogin, auth boundary, read surface, edit-tag round-trip, token revoke verified"
