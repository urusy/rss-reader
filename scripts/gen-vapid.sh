#!/usr/bin/env bash
# VAPID 鍵ペア（P-256）を生成し、.env に貼れる形で出力する（機能31 / Web Push）。
# 依存: openssl。生成した鍵は秘密。コミットしないこと。
#
# 使い方:
#   scripts/gen-vapid.sh
# 出力の VAPID_PUBLIC_KEY / VAPID_PRIVATE_KEY を .env に貼る。
set -euo pipefail

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# P-256 秘密鍵（PEM）
openssl ecparam -genkey -name prime256v1 -noout -out "$tmp/private.pem" 2>/dev/null

# 公開鍵: 非圧縮点 65 バイト（0x04 || X || Y）を base64url(no pad) に。
public_key="$(openssl ec -in "$tmp/private.pem" -pubout -outform DER 2>/dev/null \
  | tail -c 65 | base64 | tr '/+' '_-' | tr -d '=\n')"

# 秘密鍵: 生の秘密スカラー 32 バイトを base64url(no pad) に。
# 固定オフセットの dd は DER レイアウト変化に脆いので、asn1parse で
# OCTET STRING（priv フィールド）の HEX を取り出す（監査 LOW）。
priv_hex="$(openssl asn1parse -in "$tmp/private.pem" 2>/dev/null \
  | awk -F'\\[HEX DUMP\\]:' '/OCTET STRING/ { print $2; exit }')"
if [ "${#priv_hex}" -ne 64 ]; then
  echo "error: unexpected private-key DER layout (got ${#priv_hex} hex chars, want 64)" >&2
  exit 1
fi
private_key="$(printf '%s' "$priv_hex" | xxd -r -p | base64 | tr '/+' '_-' | tr -d '=\n')"

echo "# --- .env に貼る（機能31 Web Push）。秘密。コミット禁止 ---"
echo "VAPID_PUBLIC_KEY=${public_key}"
echo "VAPID_PRIVATE_KEY=${private_key}"
