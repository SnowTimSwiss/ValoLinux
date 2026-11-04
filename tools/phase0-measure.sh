#!/usr/bin/env bash
# Phase 0: capture memory/disk snapshot on a test device. Read-only, non-destructive.
# Usage: tools/phase0-measure.sh "label, e.g. 'idle' or 'firefox 3 tabs'"
set -euo pipefail
label="${1:-snapshot}"
echo "== ${label} ($(date -Is)) =="
echo "-- free -m"; free -m
echo "-- top 10 RSS processes"; ps -eo rss,comm --sort=-rss | head -n 11
echo "-- disk (/)"; df -h /
echo "-- zram"; (command -v zramctl >/dev/null && zramctl) || echo "zramctl not available"
