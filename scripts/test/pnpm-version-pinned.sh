#!/usr/bin/env bash
# Regression test for: corepack provisioning an ancient default pnpm (4.0.1)
# because frontend/package.json declared no `packageManager`.
#
#   Red  (no pin):   resolved pnpm major != declared major (or none declared).
#   Green (pinned):  pnpm version inside frontend/ matches the declared major.
set -uo pipefail
cd "$(dirname "$0")/../.."

declared="$(grep -oE 'pnpm@[0-9]+\.[0-9]+\.[0-9]+' frontend/package.json | head -1)"
if [ -z "$declared" ]; then
    echo "FAIL: frontend/package.json declares no packageManager (pnpm@x.y.z)"
    exit 1
fi
want_major="${declared#pnpm@}"; want_major="${want_major%%.*}"

actual="$(cd frontend && COREPACK_ENABLE_DOWNLOAD_PROMPT=0 pnpm --version 2>&1)"
got_major="${actual%%.*}"

if [ "$want_major" = "$got_major" ]; then
    echo "PASS: pnpm $actual matches declared $declared"
    exit 0
fi

echo "FAIL: declared $declared but resolved pnpm is $actual"
exit 1
