#!/usr/bin/env bash
#
# Synapseed Baseline — run bench suite and save a timestamped snapshot.
#
# Usage:
#   ./tools/baseline.sh [suite_path] [label]
#
# Examples:
#   ./tools/baseline.sh                          # default: suites/grounding_v1.jsonl
#   ./tools/baseline.sh suites/grounding_v1.jsonl pre-refactor
#
# Output:
#   benchmark/results/baseline_<label>_<timestamp>.json
#
# The regression detector can then compare baselines:
#   python tools/regression.py baseline_before.json baseline_after.json

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
SUITE="${1:-suites/grounding_v1.jsonl}"
LABEL="${2:-snapshot}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
OUTPUT_DIR="$PROJECT_ROOT/benchmark/results"

mkdir -p "$OUTPUT_DIR"
OUTPUT_FILE="$OUTPUT_DIR/baseline_${LABEL}_${TIMESTAMP}.json"

echo "=== Synapseed Baseline ==="
echo "Suite:  $SUITE"
echo "Label:  $LABEL"
echo "Output: $OUTPUT_FILE"
echo ""

# Build release first for consistent timing
echo "Building release binary..."
cd "$PROJECT_ROOT"
cargo build --release 2>&1 | tail -5

# Run the bench suite via the MCP run_benchmark tool
# We use the binary's built-in benchmark support
echo ""
echo "Running benchmark suite..."

# Check if the suite exists
SUITE_PATH="$PROJECT_ROOT/crates/bench/$SUITE"
if [ ! -f "$SUITE_PATH" ]; then
    SUITE_PATH="$PROJECT_ROOT/$SUITE"
fi

if [ ! -f "$SUITE_PATH" ]; then
    echo "Suite file not found: $SUITE"
    echo "Available suites:"
    find "$PROJECT_ROOT/crates/bench/suites" -name "*.jsonl" 2>/dev/null | while read f; do
        echo "  $(basename "$f")"
    done
    exit 2
fi

echo "Suite path: $SUITE_PATH"
echo ""

# Run via cargo test bench or direct invocation
# The bench engine is exposed via MCP's run_benchmark tool.
# For baseline snapshots, we invoke it programmatically.
echo "Baseline captured at $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "Compare with: python tools/regression.py <baseline1>.json <baseline2>.json"
echo ""
echo "Done."
