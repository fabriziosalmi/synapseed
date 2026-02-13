#!/usr/bin/env python3
"""Search Benchmark: evaluate Tantivy ranking quality.

Runs queries through synapseed's MCP protocol (JSON-RPC) and measures:
- Mean Reciprocal Rank (MRR): how high the correct result ranks
- Precision@K: fraction of top-K results that are relevant
- Recall@K: fraction of relevant results found in top-K

Uses a persistent synapseed serve session so the Tantivy index stays
warm across all queries (hoist once, search many).

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
import threading
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


# ── MCP Client (JSON-RPC over stdio) ────────────────────────────────

class McpSession:
    """Lightweight MCP client: talks to `synapseed serve` over JSON-RPC."""

    def __init__(self, repo_path: str):
        self.repo_path = repo_path
        self._proc: subprocess.Popen | None = None
        self._req_id = 0
        self._lock = threading.Lock()
        self._stderr_thread: threading.Thread | None = None
        self._stderr_lines: list[str] = []

    def start(self):
        self._proc = subprocess.Popen(
            ["synapseed", "serve"],
            cwd=self.repo_path,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env={**os.environ, "RUST_LOG": "off"},
        )
        # Drain stderr in background to prevent deadlock
        self._stderr_thread = threading.Thread(
            target=self._drain_stderr, daemon=True
        )
        self._stderr_thread.start()

        # Send MCP initialize
        self._send_request("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "benchmark", "version": "1.0"},
        })
        # Send initialized notification
        self._send_notification("notifications/initialized", {})

    def stop(self):
        if self._proc:
            try:
                self._proc.stdin.close()
                self._proc.wait(timeout=5)
            except Exception:
                self._proc.kill()

    def _drain_stderr(self):
        for line in self._proc.stderr:
            self._stderr_lines.append(line.rstrip())

    def _send_request(self, method: str, params: dict) -> dict:
        with self._lock:
            self._req_id += 1
            req = {
                "jsonrpc": "2.0",
                "id": self._req_id,
                "method": method,
                "params": params,
            }
            line = json.dumps(req)
            self._proc.stdin.write(line + "\n")
            self._proc.stdin.flush()

            # Read response (skip notifications)
            while True:
                resp_line = self._proc.stdout.readline()
                if not resp_line:
                    return {"error": "EOF from server"}
                try:
                    resp = json.loads(resp_line.strip())
                except json.JSONDecodeError:
                    continue
                if "id" in resp:
                    return resp
                # else it's a notification, skip

    def _send_notification(self, method: str, params: dict):
        with self._lock:
            notif = {
                "jsonrpc": "2.0",
                "method": method,
                "params": params,
            }
            self._proc.stdin.write(json.dumps(notif) + "\n")
            self._proc.stdin.flush()

    def call_tool(self, name: str, arguments: dict) -> dict:
        resp = self._send_request("tools/call", {
            "name": name,
            "arguments": arguments,
        })
        return resp.get("result", {})

    def hoist(self) -> dict:
        return self.call_tool("hoist", {})

    def wait_for_search_ready(self, probe_query: str = "fn", timeout: int = 30):
        """Poll search until Tantivy background indexing completes."""
        import time
        deadline = time.time() + timeout
        while time.time() < deadline:
            result = self.call_tool("search", {"query": probe_query, "limit": 1})
            text = result.get("content", [{}])[0].get("text", "")
            if "Found" in text and "result" in text:
                return True
            time.sleep(0.5)
        return False

    def search(self, query: str, limit: int = 10) -> list[dict]:
        result = self.call_tool("search", {"query": query, "limit": limit})
        content = result.get("content", [])
        if not content:
            return []
        text = content[0].get("text", "")
        if "No results" in text:
            return []
        # The search tool returns: "Found N result(s) for ...\n[\n  {...}, ...]"
        # Extract the JSON array after the header line.
        bracket_pos = text.find("[")
        if bracket_pos >= 0:
            try:
                return json.loads(text[bracket_pos:])
            except json.JSONDecodeError:
                pass
        # Fallback: try parsing the whole text as JSON
        try:
            return json.loads(text)
        except json.JSONDecodeError:
            return []


# ── Metrics ──────────────────────────────────────────────────────────

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


# ── Benchmark Runner ─────────────────────────────────────────────────

def run_benchmark(queries: list[SearchQuery], top_k: int, reporter: Reporter):
    """Run the search quality benchmark using a persistent MCP session."""
    repo_path = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    reporter.header(f"Top-K: {top_k} | Queries: {len(queries)}")

    # Start MCP session and hoist once
    reporter.console.print("  Starting MCP session...")
    session = McpSession(repo_path)
    try:
        session.start()
        reporter.console.print("  Waiting for Tantivy index (background thread)...")
        if session.wait_for_search_ready(probe_query="struct", timeout=30):
            reporter.console.print("  Tantivy index ready.")
        else:
            reporter.console.print("  [WARN] Tantivy index not ready after 30s — results may be empty")

        results = []
        for q in queries:
            reporter.console.print(f"\n  [{q.difficulty.upper()}] {q.id}: '{q.query}'")

            search_results = session.search(q.query, limit=top_k)
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

    finally:
        session.stop()

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
