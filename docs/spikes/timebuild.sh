#!/usr/bin/env bash
# Usage: ./timebuild.sh <label> <target_dir> <profile> [extra cargo args...]
# Runs a clean cargo build of src-tauri (package `codedocs`) and prints
# a machine-readable timing line to stderr.
set -euo pipefail

LABEL="$1"; shift
TARGET_DIR="$1"; shift
PROFILE="$1"; shift

WORK=/tmp/opencode/spike-pdf
OUT="/tmp/opencode/spike-pdf/timings"
mkdir -p "$OUT"

rm -rf "$TARGET_DIR"

echo "=== [$LABEL] start $(date -Is) (target=$TARGET_DIR profile=$PROFILE) load=$(cut -d' ' -f1-3 /proc/loadavg) ==="

START=$(date +%s.%N)
(
  cd "$WORK"
  CARGO_TARGET_DIR="$TARGET_DIR" \
    cargo build -p codedocs --profile "$PROFILE" "$@"
) > "$OUT/$LABEL.log" 2>&1
RC=$?
END=$(date +%s.%N)

ELAPSED=$(awk -v a="$START" -v b="$END" 'BEGIN{printf "%.1f", b-a}')
LOAD=$(cut -d' ' -f1-3 /proc/loadavg)
echo "RESULT label=$LABEL profile=$PROFILE rc=$RC seconds=$ELAPSED endload=$LOAD" | tee -a "$OUT/summary.txt"
echo "=== [$LABEL] end, ${ELAPSED}s, rc=$RC ==="
tail -20 "$OUT/$LABEL.log"
exit $RC