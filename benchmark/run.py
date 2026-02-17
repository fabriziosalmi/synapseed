#!/usr/bin/env python3
"""Convenience launcher — run any benchmark from inside benchmark/.

Usage (from benchmark/ directory):
    source venv/bin/activate
    python run.py coding --quick
    python run.py grounding --quick
    python run.py search
    python run.py niah --quick
    python run.py coding --quick --all-models
"""

import os
import sys

# Ensure project root is in sys.path so `from benchmark.*` imports work
PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, PROJECT_ROOT)

BENCHMARKS = {
    "coding": "benchmark.coding.run",
    "grounding": "benchmark.grounding.run",
    "search": "benchmark.search.run",
    "niah": "benchmark.niah.run",
}


def main():
    if len(sys.argv) < 2 or sys.argv[1] in ("-h", "--help"):
        print("Usage: python run.py <benchmark> [options]")
        print(f"\nAvailable benchmarks: {', '.join(BENCHMARKS)}")
        print("\nExamples:")
        print("  python run.py coding --quick")
        print("  python run.py grounding --quick --all-models")
        print("  python run.py search")
        print("  python run.py niah --quick")
        sys.exit(0)

    name = sys.argv[1]
    if name not in BENCHMARKS:
        print(f"Unknown benchmark: {name}")
        print(f"Available: {', '.join(BENCHMARKS)}")
        sys.exit(1)

    # Rewrite argv so argparse in the sub-module sees the right args
    sys.argv = [BENCHMARKS[name]] + sys.argv[2:]

    # Import and run
    import importlib
    mod = importlib.import_module(BENCHMARKS[name])
    mod.main()


if __name__ == "__main__":
    main()
