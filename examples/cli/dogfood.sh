#!/usr/bin/env bash
# Dogfood against this repository's wvq-invariants OpenSpec change.
set -euo pipefail

WVQ="${WVQ:-npx @weavatrix/wvq@0.1.0-alpha.1}"
REPO="${REPO:-.}"
CHANGE="${CHANGE:-wvq-invariants}"

$WVQ --repo "$REPO" doctor
$WVQ --repo "$REPO" spec validate --change "$CHANGE"
$WVQ --repo "$REPO" run \
  --change "$CHANGE" \
  --base origin/main \
  --head HEAD \
  --scope impacted \
  --evidence-policy minimal
# Stage A: facts stay honest; exit stays 0
$WVQ --repo "$REPO" verify --change "$CHANGE" --observe-only true
