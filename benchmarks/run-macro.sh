#!/usr/bin/env bash
# Macrobenchmark + memory baseline for perf-scan.
#
# One process per sample, so each sample's peak working set belongs to that scan
# alone. Writes raw output and a summary to benchmarks/results/<UTC stamp>/.
# Stops if any scan's node count differs from its dataset manifest.
#
# Usage:
#   benchmarks/run-macro.sh [-n RUNS] [-c warm|cold] [-m on|off] [-o OUTDIR] DATASET...
#
#   -n RUNS   samples per dataset (default 5; the plan requires at least 5)
#   -c LABEL  cache label recorded with each run. This script does NOT flush the
#             OS cache; a "cold" label is only valid after a documented flush.
#   -m off    run without the working-set sampler (sampler-overhead baseline)
#   -o DIR    output directory (default benchmarks/results/<stamp>)
#
# Environment:
#   PERF_SCAN  path to the perf-scan binary
#              (default: target/release/perf-scan[.exe] in this repo)

set -euo pipefail

RUNS=5
CACHE=warm
MEMORY=on
OUT=""
while getopts "n:c:m:o:" opt; do
  case "$opt" in
    n) RUNS="$OPTARG" ;;
    c) CACHE="$OPTARG" ;;
    m) MEMORY="$OPTARG" ;;
    o) OUT="$OPTARG" ;;
    *) echo "see header of $0 for usage" >&2; exit 2 ;;
  esac
done
shift $((OPTIND - 1))
if [ "$#" -lt 1 ]; then
  echo "usage: $0 [-n RUNS] [-c warm|cold] [-m on|off] [-o DIR] DATASET..." >&2
  exit 2
fi
case "$CACHE" in warm|cold) ;; *) echo "cache must be warm or cold" >&2; exit 2 ;; esac
case "$MEMORY" in on|off) ;; *) echo "memory must be on or off" >&2; exit 2 ;; esac
if ! [ "$RUNS" -ge 1 ] 2>/dev/null; then echo "runs must be a positive integer" >&2; exit 2; fi

REPO="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${PERF_SCAN:-$REPO/target/release/perf-scan}"
if [ -f "$BIN.exe" ]; then BIN="$BIN.exe"; fi
if [ ! -f "$BIN" ]; then
  echo "perf-scan not found at $BIN; run: cargo build -p timberstat-perf --release --bins" >&2
  exit 1
fi

# Native Windows programs need C:/... paths; bash-style /c/... paths are not translated.
to_native() {
  if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else echo "$1"; fi
}

STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
OUT="${OUT:-$REPO/benchmarks/results/$STAMP}"
mkdir -p "$OUT"

MEMORY_FLAG=""
if [ "$MEMORY" = off ]; then MEMORY_FLAG="--no-memory"; fi

# Method record: what was run, from which tree, with which binary.
{
  echo "stamp_utc=$STAMP"
  echo "commit=$(cd "$REPO" && git rev-parse HEAD)"
  echo "branch=$(cd "$REPO" && git rev-parse --abbrev-ref HEAD)"
  echo "uncommitted_paths=$(cd "$REPO" && git status --porcelain | wc -l | tr -d ' ')"
  echo "binary=$(to_native "$BIN")"
  echo "binary_sha256=$(sha256sum "$BIN" | cut -d' ' -f1)"
  echo "cache_label=$CACHE"
  echo "cache_flush=not performed by this script"
  echo "memory_sampler=$MEMORY"
  echo "samples_per_dataset=$RUNS"
  echo "datasets=$*"
} > "$OUT/method.txt"

"$BIN" --env > "$OUT/environment.json"

for id in "$@"; do
  file="$OUT/runs-$id.jsonl"
  : > "$file"
  for i in $(seq 1 "$RUNS"); do
    # shellcheck disable=SC2086
    "$BIN" --dataset "$id" --runs 1 --cache "$CACHE" $MEMORY_FLAG >> "$file"
  done
  if grep -q '"nodes_match_manifest":false' "$file"; then
    echo "dataset $id: scan node count differs from manifest; see $file" >&2
    exit 1
  fi
done

summary_inputs=()
for f in "$OUT"/runs-*.jsonl; do summary_inputs+=("$(to_native "$f")"); done
"$BIN" summarize "${summary_inputs[@]}" > "$OUT/summary.json"
echo "wrote $OUT"
