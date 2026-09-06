#!/usr/bin/env bash
# Quickstart: discovery â†’ plan â†’ impacted run â†’ verify
set -euo pipefail

WVQ="${WVQ:-npx @weavatrix/wvq@0.1.0-alpha.3}"
REPO="${REPO:-.}"
CHANGE="${CHANGE:-current}"
BASE="${BASE:-origin/main}"
HEAD_REF="${HEAD_REF:-HEAD}"

$WVQ --repo "$REPO" doctor
$WVQ --repo "$REPO" plan --change "$CHANGE"
$WVQ --repo "$REPO" run \
  --change "$CHANGE" \
  --base "$BASE" \
  --head "$HEAD_REF" \
  --scope impacted \
  --evidence-policy minimal
$WVQ --repo "$REPO" status
$WVQ --repo "$REPO" verify --change "$CHANGE"
