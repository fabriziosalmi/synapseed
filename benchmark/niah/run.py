#!/usr/bin/env python3
"""NIAH Benchmark: Needle-in-a-Haystack context sensitivity.

Embeds a synthetic "needle" (known fact) at various depths within
a haystack of realistic code comments, then tests whether the LLM
can extract it.

Test matrix: 5 depths x 4 context lengths = 20 tests per mode.

Usage:
    python -m benchmark.niah.run
    python -m benchmark.niah.run --quick   # 3x2 = 6 tests only
"""

from __future__ import annotations

import argparse
import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(__file__))))

from dotenv import load_dotenv

from benchmark.shared.llm import LLMClient
from benchmark.shared.reporting import Reporter

# ── Constants ────────────────────────────────────────────────────────

NEEDLE = (
    "The internal project codename for the next release is PHOENIX-7734 "
    "and the secret deployment key is ZK-ALPHA-9921."
)

PROBE = (
    "Based on the context provided, what is the internal project codename "
    "for the next release, and what is the secret deployment key? "
    "Reply with ONLY the codename and key, nothing else."
)

EXPECTED_CODENAME = "PHOENIX-7734"
EXPECTED_KEY = "ZK-ALPHA-9921"

DEPTHS = [0.0, 0.25, 0.5, 0.75, 1.0]
CONTEXT_LENGTHS = [500, 1000, 2000, 4000]  # approximate token count

QUICK_DEPTHS = [0.0, 0.5, 1.0]
QUICK_LENGTHS = [500, 2000]

# Realistic code comments used as filler material
FILLER_COMMENTS = [
    "// Initialize the connection pool with default settings",
    "// Parse the configuration file and validate all required fields",
    "// The retry logic uses exponential backoff with jitter",
    "// Cache invalidation happens on every write operation",
    "// This module handles authentication via JWT tokens",
    "// The load balancer distributes requests using round-robin",
    "// Database migrations run automatically on startup",
    "// Rate limiting is enforced at the API gateway level",
    "// The event loop processes messages in FIFO order",
    "// Compression is enabled for responses larger than 1KB",
    "// The health check endpoint returns 200 when all services are up",
    "// Graceful shutdown waits for in-flight requests to complete",
    "// The logger writes to both stdout and a rotating file",
    "// Feature flags are loaded from the configuration service",
    "// The circuit breaker opens after 5 consecutive failures",
    "// TLS certificates are rotated every 90 days automatically",
    "// The scheduler runs background jobs every 5 minutes",
    "// Input validation rejects payloads larger than 10MB",
    "// The search index is rebuilt nightly during off-peak hours",
    "// WebSocket connections are kept alive with periodic pings",
    "// The message queue guarantees at-least-once delivery",
    "// Session tokens expire after 24 hours of inactivity",
    "// The CDN caches static assets with a 7-day TTL",
    "// Error responses include a correlation ID for debugging",
    "// The ORM uses lazy loading by default for related entities",
    "// Audit logs capture all admin actions with timestamps",
    "// The deployment pipeline runs integration tests before rollout",
    "// Memory limits are enforced per container using cgroups",
    "// The API versioning strategy uses URL path prefixes",
    "// Batch processing jobs are idempotent and safe to retry",
]


def build_haystack(target_tokens: int, needle: str, depth: float) -> str:
    """Build a haystack with the needle embedded at the specified depth.

    depth=0.0 means needle at the very beginning,
    depth=1.0 means needle at the very end.
    """
    rng = random.Random(42)  # Fixed seed for reproducibility

    # Each filler line is ~10-15 tokens
    lines_needed = target_tokens // 12
    filler_lines = [rng.choice(FILLER_COMMENTS) for _ in range(lines_needed)]

    # Insert needle at the target depth
    insert_pos = int(len(filler_lines) * depth)
    insert_pos = max(0, min(insert_pos, len(filler_lines)))
    filler_lines.insert(insert_pos, f"\n{needle}\n")

    return "\n".join(filler_lines)


def check_extraction(response: str) -> dict:
    """Check if the needle was extracted correctly."""
    resp_upper = response.upper()
    found_codename = EXPECTED_CODENAME in response
    found_key = EXPECTED_KEY in response

    # Partial: found one but not both
    partial = (found_codename or found_key) and not (found_codename and found_key)

    return {
        "found_codename": found_codename,
        "found_key": found_key,
        "found_both": found_codename and found_key,
        "partial": partial,
    }


def run_benchmark(
    client: LLMClient,
    depths: list[float],
    lengths: list[int],
    reporter: Reporter,
):
    """Run the NIAH benchmark across all depth x length combinations."""
    reporter.header(f"Model: {client.model} | {len(depths)}x{len(lengths)} = {len(depths)*len(lengths)} tests")

    results = []
    for ctx_len in lengths:
        for depth in depths:
            reporter.console.print(
                f"  Context={ctx_len} Depth={depth:.2f}...", style="dim", end=" "
            )

            haystack = build_haystack(ctx_len, NEEDLE, depth)

            resp = client.chat(
                PROBE,
                system_message="You are a precise information extractor. Answer ONLY with the requested data.",
                context=haystack,
            )

            extraction = check_extraction(resp.content)
            status = "FOUND" if extraction["found_both"] else ("PARTIAL" if extraction["partial"] else "MISS")
            color = {"FOUND": "green", "PARTIAL": "yellow", "MISS": "red"}[status]

            reporter.console.print(f"[{color}]{status}[/{color}]")

            results.append({
                "context_length": ctx_len,
                "depth": depth,
                "status": status,
                **extraction,
                "latency_s": resp.latency_s,
                "tokens": resp.tokens_total,
                "response": resp.content[:200],
            })

    # ── Heatmap table ──
    reporter.console.print()
    columns = ["Depth \\ Ctx"] + [str(l) for l in lengths]
    rows = []
    for depth in depths:
        row = [f"{depth:.2f}"]
        for ctx_len in lengths:
            r = next(
                (r for r in results if r["context_length"] == ctx_len and r["depth"] == depth),
                None,
            )
            if r:
                status = r["status"]
                emoji_map = {"FOUND": "[green]OK[/green]", "PARTIAL": "[yellow]~[/yellow]", "MISS": "[red]X[/red]"}
                row.append(emoji_map.get(status, "?"))
            else:
                row.append("-")
        rows.append(row)

    reporter.table("NIAH Heatmap (Depth x Context Length)", columns, rows)

    # ── Aggregate ──
    total = len(results)
    found = sum(1 for r in results if r["found_both"])
    partial = sum(1 for r in results if r["partial"])
    missed = total - found - partial

    reporter.summary_panel([
        f"Model: {client.model}",
        f"Total tests: {total}",
        f"Found both: {found}/{total} ({100*found/total:.0f}%)",
        f"Partial:     {partial}/{total}",
        f"Missed:      {missed}/{total}",
    ])

    reporter.save(
        {
            "model": client.model,
            "depths": depths,
            "context_lengths": lengths,
            "results": results,
            "aggregate": {
                "total": total,
                "found": found,
                "partial": partial,
                "missed": missed,
                "found_rate": found / total if total else 0,
            },
        },
    )


def main():
    load_dotenv(os.path.join(os.path.dirname(os.path.dirname(__file__)), ".env"))

    parser = argparse.ArgumentParser(description="SYNAPSEED NIAH Benchmark")
    parser.add_argument("--quick", action="store_true", help="Reduced matrix (3x2 = 6 tests)")
    parser.add_argument("--model", help="Override LLM model name")
    args = parser.parse_args()

    client = LLMClient.from_env()
    if args.model:
        client.model = args.model

    depths = QUICK_DEPTHS if args.quick else DEPTHS
    lengths = QUICK_LENGTHS if args.quick else CONTEXT_LENGTHS

    reporter = Reporter("NIAH")
    run_benchmark(client, depths, lengths, reporter)


if __name__ == "__main__":
    main()
