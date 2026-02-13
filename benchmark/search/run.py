#!/usr/bin/env python3
"""Search Benchmark: evaluate Tantivy ranking quality.

Runs queries through `synapseed search` and measures:
- Mean Reciprocal Rank (MRR): how high the correct result ranks
- Precision@K: fraction of top-K results that are relevant
- Recall@K: fraction of relevant results found in top-K

Usage:
    python -m benchmark.search.run
    python -m benchmark.search.run --top-k 5
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from dataclasses import dataclass, field

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(__file__))))

from dotenv import load_dotenv

from benchmark.shared.reporting import Reporter


@dataclass
class SearchQuery:
    """A search query with known-relevant results."""

    id: str
    query: str
    relevant_symbols: list[str]  # Symbol names that SHOULD be in results
    relevant_files: list[str] = field(default_factory=list)  # Files that should appear
    difficulty: str = "medium"


# ── Query bank ───────────────────────────────────────────────────────
# Each query has known-relevant results verified against the codebase.

QUERIES: list[SearchQuery] = [
    SearchQuery(
        id="s01_router",
        query="intent router classify",
        relevant_symbols=["classify_intent", "Intent"],
        relevant_files=["crates/whisper/src/router/intent.rs"],
    ),
    SearchQuery(
        id="s02_coherence",
        query="coherence gate threshold",
        relevant_symbols=["coherence_gate", "coherence_score"],
        relevant_files=["crates/whisper/src/router/coherence.rs"],
    ),
    SearchQuery(
        id="s03_security_scan",
        query="DLP scanner finding",
        relevant_symbols=["Finding", "scan_content"],
        relevant_files=["crates/husk/src/scanner.rs"],
    ),
    SearchQuery(
        id="s04_search_index",
        query="search index build tantivy",
        relevant_symbols=["SearchIndex", "index_all", "search"],
        relevant_files=["crates/search/src/indexer.rs"],
    ),
    SearchQuery(
        id="s05_plugin_trait",
        query="SynapsePlugin trait",
        relevant_symbols=["SynapsePlugin"],
        relevant_files=["crates/core/src/plugin.rs"],
    ),
    SearchQuery(
        id="s06_session_state",
        query="session state save load",
        relevant_symbols=["SessionState", "save", "load"],
        relevant_files=["crates/core/src/session.rs"],
    ),
    SearchQuery(
        id="s07_momentum",
        query="MomentumEngine model tier",
        relevant_symbols=["MomentumEngine", "ModelTier"],
        relevant_files=["crates/core/src/momentum.rs"],
    ),
    SearchQuery(
        id="s08_gym_sandbox",
        query="gym sandbox evaluate mutation",
        relevant_symbols=["evaluate", "run_mutations"],
        relevant_files=["crates/gym/src/sandbox.rs"],
    ),
    SearchQuery(
        id="s09_visibility",
        query="visibility boost public private",
        relevant_symbols=["Visibility", "visibility_boost"],
        relevant_files=["crates/search/src/indexer.rs"],
        difficulty="hard",
    ),
    SearchQuery(
        id="s10_event_system",
        query="SynapseEvent FileChanged SecurityAlert",
        relevant_symbols=["SynapseEvent", "FileChanged", "SecurityAlert"],
        relevant_files=["crates/core/src/event.rs"],
    ),
    SearchQuery(
        id="s11_camel_case",
        query="HttpServer",
        relevant_symbols=["HttpServer"],
        difficulty="hard",
    ),
    SearchQuery(
        id="s12_pagerank",
        query="pagerank module authority",
        relevant_symbols=["pagerank_boost"],
        relevant_files=["crates/search/src/indexer.rs"],
        difficulty="hard",
    ),
]


def run_search(query: str, repo_path: str, limit: int = 10) -> list[dict]:
    """Run synapseed search and parse JSON results."""
    try:
        result = subprocess.run(
            ["synapseed", "search", query, "--limit", str(limit), "--json"],
            cwd=repo_path,
            capture_output=True,
            text=True,
            timeout=30,
            env={**os.environ, "RUST_LOG": "off"},
        )
        if result.returncode == 0 and result.stdout.strip():
            return json.loads(result.stdout)
    except (subprocess.TimeoutExpired, json.JSONDecodeError, FileNotFoundError):
        pass

    # Fallback: try synapseed lookup for symbol-based queries
    return []


def reciprocal_rank(results: list[dict], relevant_symbols: list[str]) -> float:
    """Compute reciprocal rank: 1/position of first relevant result."""
    for i, r in enumerate(results):
        symbol = r.get("symbol", "")
        if any(s.lower() in symbol.lower() for s in relevant_symbols):
            return 1.0 / (i + 1)
    return 0.0


def precision_at_k(results: list[dict], relevant_symbols: list[str], k: int) -> float:
    """Fraction of top-K results that are relevant."""
    if not results or k == 0:
        return 0.0
    top_k = results[:k]
    hits = sum(
        1 for r in top_k
        if any(s.lower() in r.get("symbol", "").lower() for s in relevant_symbols)
    )
    return hits / k


def recall_at_k(results: list[dict], relevant_symbols: list[str], k: int) -> float:
    """Fraction of relevant symbols found in top-K results."""
    if not relevant_symbols:
        return 1.0
    top_k = results[:k]
    result_symbols = [r.get("symbol", "").lower() for r in top_k]
    found = sum(
        1 for s in relevant_symbols
        if any(s.lower() in rs for rs in result_symbols)
    )
    return found / len(relevant_symbols)


def file_hit_at_k(results: list[dict], relevant_files: list[str], k: int) -> float:
    """Fraction of relevant files found in top-K results."""
    if not relevant_files:
        return 1.0
    top_k = results[:k]
    result_files = [r.get("file", "") for r in top_k]
    found = sum(
        1 for f in relevant_files
        if any(f in rf for rf in result_files)
    )
    return found / len(relevant_files)


def run_benchmark(queries: list[SearchQuery], top_k: int, reporter: Reporter):
    """Run the search quality benchmark."""
    repo_path = os.path.dirname(os.path.dirname(os.path.dirname(__file__)))
    reporter.header(f"Top-K: {top_k} | Queries: {len(queries)}")

    results = []
    for q in queries:
        reporter.console.print(f"\n  [{q.difficulty.upper()}] {q.id}: '{q.query}'")

        search_results = run_search(q.query, repo_path, limit=top_k)
        n = len(search_results)

        rr = reciprocal_rank(search_results, q.relevant_symbols)
        p_k = precision_at_k(search_results, q.relevant_symbols, top_k)
        r_k = recall_at_k(search_results, q.relevant_symbols, top_k)
        fh = file_hit_at_k(search_results, q.relevant_files, top_k)

        reporter.console.print(
            f"    Results: {n}  MRR={rr:.2f}  P@{top_k}={p_k:.2f}  "
            f"R@{top_k}={r_k:.2f}  FileHit={fh:.2f}"
        )

        results.append({
            "id": q.id,
            "query": q.query,
            "difficulty": q.difficulty,
            "results_count": n,
            "mrr": rr,
            f"precision_at_{top_k}": p_k,
            f"recall_at_{top_k}": r_k,
            f"file_hit_at_{top_k}": fh,
            "top_results": [
                {"symbol": r.get("symbol", ""), "file": r.get("file", ""), "score": r.get("score", 0)}
                for r in search_results[:5]
            ],
        })

    # ── Aggregate ──
    mrr_mean = sum(r["mrr"] for r in results) / len(results)
    p_mean = sum(r[f"precision_at_{top_k}"] for r in results) / len(results)
    r_mean = sum(r[f"recall_at_{top_k}"] for r in results) / len(results)
    fh_mean = sum(r[f"file_hit_at_{top_k}"] for r in results) / len(results)

    rows = [
        [r["id"], r["query"][:30], f"{r['mrr']:.2f}",
         f"{r[f'precision_at_{top_k}']:.2f}", f"{r[f'recall_at_{top_k}']:.2f}"]
        for r in results
    ]

    reporter.console.print()
    reporter.table(
        "Search Quality",
        ["Query", "Terms", "MRR", f"P@{top_k}", f"R@{top_k}"],
        rows,
    )

    reporter.summary_panel([
        f"Queries: {len(results)}  |  Top-K: {top_k}",
        f"Mean MRR:        {mrr_mean:.3f}",
        f"Mean P@{top_k}:       {p_mean:.3f}",
        f"Mean R@{top_k}:       {r_mean:.3f}",
        f"Mean File Hit:   {fh_mean:.3f}",
    ])

    reporter.save(
        {
            "top_k": top_k,
            "queries": results,
            "aggregate": {
                "mrr": mrr_mean,
                f"precision_at_{top_k}": p_mean,
                f"recall_at_{top_k}": r_mean,
                f"file_hit_at_{top_k}": fh_mean,
            },
        },
    )


def main():
    load_dotenv(os.path.join(os.path.dirname(os.path.dirname(__file__)), ".env"))

    parser = argparse.ArgumentParser(description="SYNAPSEED Search Quality Benchmark")
    parser.add_argument("--top-k", type=int, default=10, help="Top-K results to evaluate")
    args = parser.parse_args()

    reporter = Reporter("Search")
    run_benchmark(QUERIES, args.top_k, reporter)


if __name__ == "__main__":
    main()
