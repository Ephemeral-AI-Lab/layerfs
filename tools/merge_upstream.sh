#!/usr/bin/env bash
# merge_upstream.sh - the recurring upstream-follow loop as one entry.
set -euo pipefail
git fetch upstream layerfs_v2_0_0
if git merge-base --is-ancestor upstream/layerfs_v2_0_0 HEAD; then
  echo "already merged; nothing to do"
  exit 0
fi
git merge upstream/layerfs_v2_0_0 --no-edit 2>&1 | tail -3 || true
echo "conflicts:"
git diff --name-only --diff-filter=U || true
