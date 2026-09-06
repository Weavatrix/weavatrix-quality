#!/usr/bin/env bash
# Extra CLI recipes â€” run individually as needed.
set -euo pipefail

WVQ="${WVQ:-npx @weavatrix/wvq@0.1.0-alpha.3}"
REPO="${REPO:-.}"
CHANGE="${CHANGE:-current}"

echo "== select =="
$WVQ --repo "$REPO" select --change "$CHANGE" --base origin/main --head WORKTREE

echo "== debt =="
$WVQ --repo "$REPO" debt --change "$CHANGE" --base origin/main --head HEAD

echo "== analyze (bounded context, zero model by default) =="
$WVQ --repo "$REPO" analyze --change "$CHANGE" --purpose implementation --token-budget 4000

echo "== bench (impacted vs full) =="
$WVQ --repo "$REPO" bench \
  --change "$CHANGE" \
  --base origin/main \
  --head WORKTREE \
  --evidence-policy minimal

echo "== record (needs browser.base_url + Playwright in the target repo) =="
# $WVQ --repo "$REPO" record --change "$CHANGE" --route /dashboard
