
import json
import statistics
import sys
import os
from pathlib import Path


# Import shared logic from results.py

# Import shared logic from results.py
# Import shared logic from results.py
from .results import get_latest_benchmark



def get_binary_size_mb() -> float:
    """Checks the size of the compiled binary."""
    # Path to the synapseed binary (relative to this script location)
    # tools/arxiv-generator/src/generators/performance.py -> ../../../target/release/synapseed
    binary_path = Path(__file__).resolve().parent.parent.parent.parent.parent / "target" / "release" / "synapseed"
    
    if binary_path.exists():
        size_bytes = binary_path.stat().st_size
        return size_bytes / (1024 * 1024)
    return 0.0


def load_performance_metrics() -> dict:
    """Parses grounding benchmark for Latency, Throughput, and Token usage.

    Uses grounding data only — no silent fallback to coding data which
    has a different schema and would produce misleading performance numbers.
    """
    data = get_latest_benchmark("grounding")

    results = data.get("results", [])
    if not results:
        return {}

    latencies = []
    throughputs = []
    blind_tokens = []
    grounded_tokens = []

    for r in results:
        g = r.get("grounded", {})
        b = r.get("blind", {})

        latency = g.get("latency_s", 0.0)
        tokens = g.get("tokens", 0)

        if latency > 0:
            latencies.append(latency)
            throughputs.append(tokens / latency)

        grounded_tokens.append(tokens)
        blind_tokens.append(b.get("tokens", 0))

    if not latencies:
        return {}
    
    avg_blind_tokens = statistics.mean(blind_tokens) if blind_tokens else 0
    avg_grounded_tokens = statistics.mean(grounded_tokens) if grounded_tokens else 0
    
    # Token "Economy" might actually be negative/costly due to RAG context
    # But checking if we save on *hallucinated* tokens is hard without manual checking.
    # We will just report the usage facts.
    
    binary_size = get_binary_size_mb()

    # Use grounding score (0-3) normalized to 0-1 for efficiency calculation
    avg_score = statistics.mean([r.get("grounded", {}).get("score", 0.0) / 3.0 for r in results]) if results else 0.0
    efficiency_score = (avg_score * 1000) / avg_grounded_tokens if avg_grounded_tokens > 0 else 0.0

    return {
        "avg_latency": statistics.mean(latencies),
        "p95_latency": statistics.quantiles(latencies, n=20)[-1] if len(latencies) >= 20 else max(latencies),
        "avg_throughput": statistics.mean(throughputs),
        "max_throughput": max(throughputs),
        "avg_blind_tokens": avg_blind_tokens,
        "avg_grounded_tokens": avg_grounded_tokens,
        "token_increase_ratio": (avg_grounded_tokens / avg_blind_tokens) if avg_blind_tokens > 0 else 0,
        "efficiency_score": efficiency_score,
        "binary_size_mb": binary_size,
        "sample_size": len(latencies)
    }

def generate_performance_table(metrics: dict) -> str:
    """Generates LaTeX code for Table 3 (Performance & Efficiency)."""
    if not metrics:
        return "% No performance data found"
    
    # Format binary size string
    bin_size_str = f"{metrics['binary_size_mb']:.1f} MB" if metrics['binary_size_mb'] > 0 else "N/A (Build missing)"

    return f"""
\\begin{{table}}[h]
    \\centering
    \\caption{{System Performance \\& Efficiency Metrics.}}
    \\label{{tab:performance}}
    \\begin{{tabular}}{{lcc}}
        \\toprule
        \\textbf{{Metric}} & \\textbf{{Value}} \\\\
        \\midrule
        Binary Size (Optimized) & {bin_size_str} \\\\
        Avg Latency (End-to-End) & {metrics['avg_latency']:.2f} s \\\\
        Throughput (Generation) & {metrics['avg_throughput']:.1f} tok/s \\\\
        Avg Tokens/Task & {int(metrics['avg_grounded_tokens'])} \\\\
        Context Expansion Ratio & {metrics['token_increase_ratio']:.1f}x \\\\
        \\textbf{{Efficiency Score}} & \\textbf{{{metrics['efficiency_score']:.2f}}} \\\\
        \\bottomrule
    \\end{{tabular}}
    \\small{{Binary size measured at \\texttt{{target/release/synapseed}}. Efficiency = (Quality Score $\\times$ 1000) / Tokens.}}
\\end{{table}}
"""

if __name__ == "__main__":
    try:
        metrics = load_performance_metrics()
        tex = generate_performance_table(metrics)
        
        assets_dir = Path(__file__).resolve().parent.parent.parent / "assets"
        os.makedirs(assets_dir, exist_ok=True)
        
        with open(assets_dir / "table3_performance.tex", "w") as f:
            f.write(tex)
            
        print("Performance table generated successfully.")
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)
