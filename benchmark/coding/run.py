#!/usr/bin/env python3
"""Coding Benchmark Runner: BLIND vs SYNAPSEED.

Usage:
    python -m benchmark.coding.run                    # Self-hosted tasks only
    python -m benchmark.coding.run --quick             # First 3 tasks
    python -m benchmark.coding.run --target actix-web  # External target
    python -m benchmark.coding.run --all               # All tasks, all targets

Results are saved to benchmark/results/ as JSON.
"""

from __future__ import annotations

import argparse
import os
import sys

# Allow running from project root: python -m benchmark.coding.run
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(__file__))))

from dotenv import load_dotenv

from benchmark.shared.llm import LLMClient, get_synapseed_context
from benchmark.shared.scoring import (
    composite_score,
    file_score,
    grounding_rate,
    hallucination_count,
    keyword_score,
    symbol_score,
)
from benchmark.shared.reporting import Reporter
from benchmark.coding.tasks import CodingTask, get_self_tasks, get_tasks


def resolve_repo_path(target: str) -> str:
    """Resolve the absolute path to a target repository."""
    if target == "synapseed":
        return os.path.dirname(os.path.dirname(os.path.dirname(__file__)))
    targets_dir = os.path.join(os.path.dirname(os.path.dirname(__file__)), "targets")
    return os.path.join(targets_dir, target)


def evaluate_response(
    response: str,
    task: CodingTask,
    repo_path: str,
) -> dict:
    """Score a response against ground truth."""
    gt = task.ground_truth
    return {
        "keyword_score": keyword_score(response, gt.keywords),
        "file_score": file_score(response, gt.files),
        "symbol_score": symbol_score(response, gt.symbols),
        "composite": composite_score(response, gt.keywords, gt.files, gt.symbols),
        "hallucinations": hallucination_count(response, gt.forbidden, repo_path),
        "grounding_rate": grounding_rate(response, repo_path),
    }


def run_task(
    task: CodingTask,
    client: LLMClient,
    repo_path: str,
    reporter: Reporter,
) -> dict:
    """Run a single task: BLIND + SYNAPSEED, single-turn + multi-turn."""
    reporter.console.print(
        f"\n  [{task.difficulty.upper()}] {task.id}: {task.question[:60]}..."
    )

    result = {
        "task_id": task.id,
        "difficulty": task.difficulty,
        "target": task.target,
        "question": task.question,
    }

    # ── Single-turn BLIND ──
    reporter.console.print("    BLIND single-turn...", style="dim")
    blind_resp = client.chat(task.question)
    result["single_blind"] = {
        "content": blind_resp.content,
        "tokens": blind_resp.tokens_total,
        "latency_s": blind_resp.latency_s,
        "success": blind_resp.success,
        **evaluate_response(blind_resp.content, task, repo_path),
    }

    # ── Single-turn SYNAPSEED ──
    reporter.console.print("    SYNAPSEED single-turn...", style="dim")
    context = get_synapseed_context(task.question, repo_path, raw=True)
    syn_resp = client.chat(task.question, context=context)
    result["single_synapseed"] = {
        "content": syn_resp.content,
        "tokens": syn_resp.tokens_total,
        "latency_s": syn_resp.latency_s,
        "success": syn_resp.success,
        "context_available": context is not None,
        **evaluate_response(syn_resp.content, task, repo_path),
    }

    # ── Multi-turn (if follow-up exists) ──
    if task.follow_up:
        reporter.console.print("    BLIND multi-turn...", style="dim")
        blind_mt = client.multi_turn([task.question, task.follow_up])
        if len(blind_mt) >= 2:
            result["multi_blind"] = {
                "turn1": {
                    "content": blind_mt[0].content,
                    "tokens": blind_mt[0].tokens_total,
                },
                "turn2": {
                    "content": blind_mt[1].content,
                    "tokens": blind_mt[1].tokens_total,
                    **evaluate_response(blind_mt[1].content, task, repo_path),
                },
            }

        reporter.console.print("    SYNAPSEED multi-turn...", style="dim")
        syn_mt = client.multi_turn(
            [task.question, task.follow_up], context=context
        )
        if len(syn_mt) >= 2:
            result["multi_synapseed"] = {
                "turn1": {
                    "content": syn_mt[0].content,
                    "tokens": syn_mt[0].tokens_total,
                },
                "turn2": {
                    "content": syn_mt[1].content,
                    "tokens": syn_mt[1].tokens_total,
                    **evaluate_response(syn_mt[1].content, task, repo_path),
                },
            }

    # ── Print comparison ──
    blind_c = result["single_blind"]["composite"]
    syn_c = result["single_synapseed"]["composite"]
    delta = syn_c - blind_c
    color = "green" if delta > 0 else ("red" if delta < 0 else "white")
    reporter.console.print(
        f"    Composite: BLIND={blind_c:.2f}  SYNAPSEED={syn_c:.2f}  "
        f"[{color}]delta={delta:+.2f}[/{color}]"
    )

    return result


def run_benchmark(tasks: list[CodingTask], client: LLMClient, reporter: Reporter):
    """Run the full benchmark suite."""
    reporter.header(f"Model: {client.model} | Tasks: {len(tasks)}")

    results = []
    for task in tasks:
        repo_path = resolve_repo_path(task.target)
        if not os.path.isdir(repo_path):
            reporter.console.print(
                f"  [yellow]SKIP[/yellow] {task.id}: target '{task.target}' "
                f"not found at {repo_path}"
            )
            continue
        results.append(run_task(task, client, repo_path, reporter))

    if not results:
        reporter.console.print("[red]No tasks executed.[/red]")
        return

    # ── Aggregate statistics ──
    blind_scores = [r["single_blind"]["composite"] for r in results]
    syn_scores = [r["single_synapseed"]["composite"] for r in results]
    blind_halluc = sum(r["single_blind"]["hallucinations"] for r in results)
    syn_halluc = sum(r["single_synapseed"]["hallucinations"] for r in results)

    blind_mean = sum(blind_scores) / len(blind_scores)
    syn_mean = sum(syn_scores) / len(syn_scores)

    # Per-difficulty breakdown
    difficulties = sorted(set(r["difficulty"] for r in results))
    diff_rows = []
    for diff in difficulties:
        b = [r["single_blind"]["composite"] for r in results if r["difficulty"] == diff]
        s = [r["single_synapseed"]["composite"] for r in results if r["difficulty"] == diff]
        bm = sum(b) / len(b) if b else 0
        sm = sum(s) / len(s) if s else 0
        d = sm - bm
        diff_rows.append([diff, f"{bm:.2f}", f"{sm:.2f}", f"{d:+.2f}"])

    reporter.console.print()
    reporter.table(
        "Results by Difficulty",
        ["Difficulty", "BLIND", "SYNAPSEED", "Delta"],
        diff_rows,
        styles=["bold", None, None, "bold"],
    )

    reporter.summary_panel([
        f"Tasks: {len(results)}",
        f"Model: {client.model}",
        f"BLIND mean composite:     {blind_mean:.3f}",
        f"SYNAPSEED mean composite: {syn_mean:.3f}",
        f"Delta:                    {syn_mean - blind_mean:+.3f}",
        f"BLIND hallucinations:     {blind_halluc}",
        f"SYNAPSEED hallucinations: {syn_halluc}",
    ])

    # ── Persist results ──
    reporter.save(
        {
            "model": client.model,
            "tasks": results,
            "aggregate": {
                "blind_mean": blind_mean,
                "synapseed_mean": syn_mean,
                "delta": syn_mean - blind_mean,
                "blind_hallucinations": blind_halluc,
                "synapseed_hallucinations": syn_halluc,
                "by_difficulty": {
                    diff: {
                        "blind": float(row[1]),
                        "synapseed": float(row[2]),
                    }
                    for diff, row in zip(difficulties, diff_rows)
                },
            },
        },
        suffix=client.model.replace("/", "_").replace(":", "_"),
    )


def main():
    load_dotenv(os.path.join(os.path.dirname(os.path.dirname(__file__)), ".env"))

    parser = argparse.ArgumentParser(description="SYNAPSEED Coding Benchmark")
    parser.add_argument("--quick", action="store_true", help="Run only first 3 tasks")
    parser.add_argument("--target", help="Filter by target repo (e.g., synapseed, actix-web)")
    parser.add_argument("--all", action="store_true", help="Run all tasks including external targets")
    parser.add_argument("--model", help="Override LLM model name")
    args = parser.parse_args()

    client = LLMClient.from_env()
    if args.model:
        client.model = args.model

    if args.all:
        tasks = get_tasks()
    elif args.target:
        tasks = get_tasks(args.target)
    else:
        tasks = get_self_tasks()

    if args.quick:
        tasks = tasks[:3]

    reporter = Reporter("Coding")
    run_benchmark(tasks, client, reporter)


if __name__ == "__main__":
    main()
