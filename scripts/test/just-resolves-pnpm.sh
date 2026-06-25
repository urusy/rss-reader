#!/usr/bin/env bash
# Regression test for: `just lint` failing with `sh: pnpm: command not found`.
#
# Hypothesis: the justfile sets no `shell`, so just runs recipes via the default
# `sh`, whose environment may not resolve pnpm. This test exercises just's actual
# recipe shell via the `_pnpm-version` recipe.
#
#   Red  (bug present): just's shell cannot find pnpm -> non-zero exit.
#   Green (after fix):  prints a pnpm version (e.g. 10.18.2) and exits 0.
set -uo pipefail
cd "$(dirname "$0")/../.."

output="$(just _pnpm-version 2>&1)"
status=$?

if [ "$status" -eq 0 ] && printf '%s' "$output" | grep -qE '[0-9]+\.[0-9]+'; then
    echo "PASS: just recipe shell resolves pnpm -> $output"
    exit 0
fi

echo "FAIL: just recipe shell cannot resolve pnpm"
echo "  exit=$status"
echo "  output=$output"
exit 1
