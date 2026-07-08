#!/usr/bin/env bash
# Run all HTTP integration tests against the running stack (nginx :8081 by default).
# Prereq: the stack is up with current code (`just up`). Run from anywhere; this
# cd's to the repo root so the per-script `docker compose exec db psql` resolves.
set -u
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root" || exit 2
BASE="${1:-http://localhost:8081}"

# Order matters slightly: stateless/read first, destructive read-all last (its
# whole-DB case is guarded behind RUN_DESTRUCTIVE and skipped by default).
scripts=(
  api-auth.sh
  api-stats.sh
  api-feeds.sh
  api-feeds-discover.sh
  api-feed-overview.sh
  api-feed-health.sh
  api-folders.sh
  api-instapaper.sh
  api-search.sh
  api-saved-views.sh
  api-rules.sh
  api-annotations.sh
  api-llm-settings.sh
  api-ask.sh
  api-tags.sh
  api-digest.sh
  api-relevance.sh
  api-clustering.sh
  api-extraction.sh
  api-opml.sh
  api-mute-rules.sh
  api-backup.sh
  api-greader.sh
  read-later.sh
  read-on-save.sh
  api-articles-read-all.sh
)

fails=0
for s in "${scripts[@]}"; do
  echo
  echo "==================== $s ===================="
  # api-stats.sh は $1 に「完全URL」を取る既存規約。他は $1=ベースURL / env BASE。
  case "$s" in
    api-stats.sh) arg="$BASE/api/stats" ;;
    *) arg="$BASE" ;;
  esac
  BASE="$BASE" bash "scripts/test/$s" "$arg"
  [ $? -ne 0 ] && fails=$((fails + 1))
done

echo
echo "============================================================"
if [ "$fails" -eq 0 ]; then
  echo "ALL INTEGRATION SUITES PASSED"
else
  echo "$fails SUITE(S) FAILED"
fi
exit "$fails"
