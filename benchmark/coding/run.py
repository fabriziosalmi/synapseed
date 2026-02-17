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
import logging
import os
import signal
import sys
import time
import traceback

# Allow running from project root: python -m benchmark.coding.run
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(__file__))))

from dotenv import load_dotenv

log = logging.getLogger(__name__)

# ── Per-task timeout (seconds) ──
# Dynamically computed: conditions × per-call timeout with headroom.
# Override with BENCH_TASK_TIMEOUT env var (0 = no limit).
_TASK_TIMEOUT_OVERRIDE = os.getenv("BENCH_TASK_TIMEOUT", "")
TASK_TIMEOUT_FIXED = int(_TASK_TIMEOUT_OVERRIDE) if _TASK_TIMEOUT_OVERRIDE else 0


def _compute_task_timeout(n_conditions: int, llm_timeout: int) -> int:
    """Compute per-task timeout based on conditions count and LLM timeout.

    Each task runs: n_conditions single-turn + 2 multi-turn calls.
    We give each call llm_timeout seconds + 50% headroom.
    """
    if TASK_TIMEOUT_FIXED > 0:
        return TASK_TIMEOUT_FIXED
    n_calls = n_conditions + 2  # single-turn + 2 multi-turn
    return int(n_calls * llm_timeout * 1.5)


class TaskTimeout(Exception):
    """Raised when a single benchmark task exceeds its time budget."""


def _alarm_handler(signum, frame):
    raise TaskTimeout("Task exceeded its time budget")

from benchmark.shared.llm import LLMClient, build_system_prompt, get_synapseed_context
from benchmark.shared.scoring import (
    coverage_score,
    citation_precision,
    file_recall,
    hallucination_count,
    keyword_recall,
    symbol_recall,
    bootstrap_ci,
    cohens_d,
    wilcoxon_signed_rank,
)
from benchmark.shared.reporting import Reporter
from benchmark.coding.tasks import CodingTask, get_self_tasks, get_tasks
from dataclasses import dataclass as _dataclass


@_dataclass
class RunCondition:
    """A single benchmark evaluation condition."""
    key: str            # Result dict key (e.g. "single_blind")
    label: str          # Console display label
    use_context: bool   # Use SYNAPSEED context?
    optimized: bool     # Use optimized system prompt?
    think: bool         # Allow <think> reasoning?


# ── Condition sets ───────────────────────────────────────────────────
BASELINE_CONDITIONS: list[RunCondition] = [
    RunCondition("single_blind", "BLIND", False, False, True),
    RunCondition("single_synapseed", "SYNAPSEED", True, False, True),
]

PROMPT_OPT_CONDITIONS: list[RunCondition] = [
    RunCondition("single_blind_opt", "BLIND+opt", False, True, True),
    RunCondition("single_synapseed_opt", "SYN+opt", True, True, True),
]

THINK_ABLATION_CONDITIONS: list[RunCondition] = [
    RunCondition("single_synapseed_nothink", "SYN/nothink", True, False, False),
    RunCondition("single_synapseed_opt_nothink", "SYN+opt/nothink", True, True, False),
]

MODE_MAP: dict[str, list[RunCondition]] = {
    "baseline": BASELINE_CONDITIONS,
    "prompt-opt": PROMPT_OPT_CONDITIONS,
    "think-ablation": THINK_ABLATION_CONDITIONS,
}


def get_conditions(modes: str) -> list[RunCondition]:
    """Resolve mode string to list of conditions.

    Accepts comma-separated mode names or 'all'.
    Baseline is always included.
    """
    if modes == "all":
        parts = list(MODE_MAP.keys())
    else:
        parts = [m.strip() for m in modes.split(",")]

    seen_keys: set[str] = set()
    conditions: list[RunCondition] = []
    for part in parts:
        for cond in MODE_MAP.get(part, []):
            if cond.key not in seen_keys:
                seen_keys.add(cond.key)
                conditions.append(cond)

    # Always include baseline
    for cond in BASELINE_CONDITIONS:
        if cond.key not in seen_keys:
            conditions.insert(0, cond)

    return conditions


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
    """Score a response against ground truth.

    Returns coverage_score (weighted recall over keyword/file/symbol)
    plus sub-metrics. NOT an F1 score.
    """
    gt = task.ground_truth
    kr = keyword_recall(response, gt.keywords)
    fr = file_recall(response, gt.files)
    sr = symbol_recall(response, gt.symbols)
    cs = coverage_score(response, gt.keywords, gt.files, gt.symbols)
    cp = citation_precision(response, repo_path)
    halluc = hallucination_count(response, gt.forbidden, repo_path)
    import math
    return {
        "keyword_recall": kr,
        "file_recall": fr,
        "symbol_recall": sr,
        "coverage_score": cs,
        "hallucinations": halluc,
        "citation_precision": cp,
        # Backward compat keys (consumed by paper generator)
        "composite": cs,
        "keyword_score": kr,
        "file_score": fr,
        "symbol_score": sr,
        "grounding_rate": cp if not math.isnan(cp) else 0.5,
    }


def _empty_scores() -> dict:
    """Return zeroed scores for a failed/timed-out evaluation."""
    return {
        "keyword_recall": 0.0,
        "file_recall": 0.0,
        "symbol_recall": 0.0,
        "coverage_score": 0.0,
        "hallucinations": 0,
        "citation_precision": 0.0,
        # Backward compat
        "composite": 0.0,
        "keyword_score": 0.0,
        "file_score": 0.0,
        "symbol_score": 0.0,
        "grounding_rate": 0.0,
    }


def _make_failed_result(task: CodingTask, error: str) -> dict:
    """Build a result dict for a task that failed entirely."""
    failed = {
        "content": "", "tokens": 0, "latency_s": 0.0,
        "success": False, "error": error, **_empty_scores(),
    }
    return {
        "task_id": task.id,
        "difficulty": task.difficulty,
        "target": task.target,
        "question": task.question,
        "single_blind": failed.copy(),
        "single_synapseed": {**failed.copy(), "context_available": False},
    }


def _safe_call(fn, label: str, reporter: Reporter):
    """Call fn(), print elapsed time, catch and log errors."""
    reporter.console.print(f"    {label}...", style="dim")
    t0 = time.time()
    try:
        result = fn()
        elapsed = time.time() - t0
        reporter.console.print(
            f"      done ({elapsed:.1f}s)", style="dim italic",
        )
        return result
    except TaskTimeout:
        raise  # Let per-task handler deal with it
    except Exception as e:
        elapsed = time.time() - t0
        reporter.console.print(
            f"      [red]FAILED[/red] ({elapsed:.1f}s): {str(e)[:80]}",
        )
        log.warning("%s failed after %.1fs: %s", label, elapsed, e)
        return None


def run_task(
    task: CodingTask,
    client: LLMClient,
    repo_path: str,
    reporter: Reporter,
    conditions: list[RunCondition] | None = None,
) -> dict:
    """Run a single task across all conditions, single-turn + multi-turn.

    Resilient: per-task SIGALRM timeout, try/except on every LLM call,
    graceful degradation with zeroed scores on failure.
    """
    if conditions is None:
        conditions = BASELINE_CONDITIONS
    reporter.console.print(
        f"\n  [{task.difficulty.upper()}] {task.id}: {task.question[:60]}..."
    )
    task_start = time.time()

    # Arm per-task timeout (Unix only)
    task_timeout = _compute_task_timeout(len(conditions), client.timeout)
    old_handler = None
    if hasattr(signal, "SIGALRM"):
        old_handler = signal.signal(signal.SIGALRM, _alarm_handler)
        signal.alarm(task_timeout)

    try:
        return _run_task_inner(task, client, repo_path, reporter, conditions)
    except TaskTimeout:
        elapsed = time.time() - task_start
        reporter.console.print(
            f"    [red]TIMEOUT[/red] after {elapsed:.0f}s (budget: {task_timeout}s) — skipping remaining calls"
        )
        return _make_failed_result(task, f"timeout after {elapsed:.0f}s")
    except Exception as e:
        elapsed = time.time() - task_start
        reporter.console.print(
            f"    [red]ERROR[/red] after {elapsed:.0f}s: {str(e)[:80]}"
        )
        log.error("Task %s crashed: %s\n%s", task.id, e, traceback.format_exc())
        return _make_failed_result(task, str(e)[:200])
    finally:
        if hasattr(signal, "SIGALRM"):
            signal.alarm(0)  # Disarm
            if old_handler is not None:
                signal.signal(signal.SIGALRM, old_handler)


def _run_task_inner(
    task: CodingTask,
    client: LLMClient,
    repo_path: str,
    reporter: Reporter,
    conditions: list[RunCondition],
) -> dict:
    """Inner task logic, separated for clean timeout handling."""
    result = {
        "task_id": task.id,
        "difficulty": task.difficulty,
        "target": task.target,
        "question": task.question,
    }

    # ── Fetch SYNAPSEED context once (shared by all conditions that need it) ──
    context = None
    if any(c.use_context for c in conditions):
        context = _safe_call(
            lambda: get_synapseed_context(task.question, repo_path, raw=True),
            "SYNAPSEED context", reporter,
        )

    # ── Run each single-turn condition ──
    for cond in conditions:
        sys_prompt = build_system_prompt(
            optimized=cond.optimized, think=cond.think,
        )
        ctx = context if cond.use_context else None

        resp = _safe_call(
            lambda sp=sys_prompt, cx=ctx: client.chat(
                task.question, system_message=sp, context=cx,
            ),
            f"{cond.label} single-turn", reporter,
        )
        if resp and resp.success:
            entry = {
                "content": resp.content,
                "tokens": resp.tokens_total,
                "latency_s": resp.latency_s,
                "success": True,
                **evaluate_response(resp.content, task, repo_path),
            }
            if cond.use_context:
                entry["context_available"] = context is not None
            result[cond.key] = entry
        else:
            error = (resp.error if resp else "call failed") or "empty"
            entry = {
                "content": "", "tokens": 0, "latency_s": 0.0,
                "success": False, "error": error, **_empty_scores(),
            }
            if cond.use_context:
                entry["context_available"] = context is not None
            result[cond.key] = entry

    # ── Multi-turn (baseline conditions only, if follow-up exists) ──
    if task.follow_up:
        base_prompt = build_system_prompt(optimized=False, think=True)

        blind_mt = _safe_call(
            lambda: client.multi_turn(
                [task.question, task.follow_up],
                system_message=base_prompt,
            ),
            "BLIND multi-turn", reporter,
        )
        if blind_mt and len(blind_mt) >= 2:
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

        syn_mt = _safe_call(
            lambda: client.multi_turn(
                [task.question, task.follow_up],
                system_message=base_prompt,
                context=context,
            ),
            "SYNAPSEED multi-turn", reporter,
        )
        if syn_mt and len(syn_mt) >= 2:
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

    # ── Print comparison matrix ──
    parts = []
    for cond in conditions:
        c = result.get(cond.key, {}).get("composite", 0.0)
        parts.append(f"{cond.label}={c:.2f}")
    reporter.console.print(f"    Composite: {' | '.join(parts)}")

    # Highlight baseline delta
    blind_c = result.get("single_blind", {}).get("composite", 0.0)
    syn_c = result.get("single_synapseed", {}).get("composite", 0.0)
    delta = syn_c - blind_c
    color = "green" if delta > 0 else ("red" if delta < 0 else "white")
    reporter.console.print(
        f"    Baseline delta: [{color}]{delta:+.2f}[/{color}]"
    )

    return result


def run_benchmark(
    tasks: list[CodingTask],
    client: LLMClient,
    reporter: Reporter,
    conditions: list[RunCondition] | None = None,
    modes_tag: str = "baseline",
):
    """Run the full benchmark suite across all conditions."""
    if conditions is None:
        conditions = BASELINE_CONDITIONS

    cond_labels = ", ".join(c.label for c in conditions)
    reporter.header(
        f"Model: {client.model} | Tasks: {len(tasks)} | Modes: {cond_labels}"
    )

    results = []
    bench_start = time.time()
    for i, task in enumerate(tasks, 1):
        repo_path = resolve_repo_path(task.target)
        if not os.path.isdir(repo_path):
            reporter.console.print(
                f"  [yellow]SKIP[/yellow] {task.id}: target '{task.target}' "
                f"not found at {repo_path}"
            )
            continue
        result = run_task(task, client, repo_path, reporter, conditions)
        results.append(result)
        elapsed = time.time() - bench_start
        reporter.console.print(
            f"    [{i}/{len(tasks)}] total elapsed: {elapsed:.0f}s",
            style="dim italic",
        )

    if not results:
        reporter.console.print("[red]No tasks executed.[/red]")
        return

    # ── Aggregate per condition ──
    cond_stats: dict[str, dict] = {}
    for cond in conditions:
        scores = [
            r[cond.key]["composite"]
            for r in results
            if cond.key in r and r[cond.key].get("success", False)
        ]
        halluc = sum(
            r[cond.key].get("hallucinations", 0)
            for r in results
            if cond.key in r
        )
        mean_val, ci_lo, ci_hi = bootstrap_ci(scores) if scores else (0.0, 0.0, 0.0)
        cond_stats[cond.key] = {
            "label": cond.label,
            "mean": mean_val,
            "ci_95": (ci_lo, ci_hi),
            "n": len(scores),
            "hallucinations": halluc,
        }

    # ── Statistical comparison (baseline BLIND vs SYNAPSEED) ──
    stat_results = {}
    if "single_blind" in cond_stats and "single_synapseed" in cond_stats:
        blind_scores = [
            r["single_blind"]["composite"]
            for r in results
            if "single_blind" in r and r["single_blind"].get("success", False)
        ]
        syn_scores = [
            r["single_synapseed"]["composite"]
            for r in results
            if "single_synapseed" in r and r["single_synapseed"].get("success", False)
        ]
        if len(blind_scores) >= 2 and len(syn_scores) >= 2:
            effect_d = cohens_d(blind_scores, syn_scores)
            W, p_value = wilcoxon_signed_rank(blind_scores, syn_scores)
            stat_results = {
                "wilcoxon_W": W,
                "wilcoxon_p": p_value,
                "cohens_d": effect_d,
                "n_paired": min(len(blind_scores), len(syn_scores)),
            }

    # Per-difficulty breakdown (dynamic columns)
    difficulties = sorted(set(r["difficulty"] for r in results))
    headers = ["Difficulty"] + [cond_stats[c.key]["label"] for c in conditions]
    diff_rows = []
    diff_data: dict[str, dict[str, float]] = {}
    for diff in difficulties:
        row = [diff]
        diff_data[diff] = {}
        for cond in conditions:
            s = [
                r[cond.key]["composite"]
                for r in results
                if r["difficulty"] == diff
                and cond.key in r
                and r[cond.key].get("success", False)
            ]
            mean = sum(s) / len(s) if s else 0.0
            row.append(f"{mean:.2f}")
            diff_data[diff][cond.key] = mean
        diff_rows.append(row)

    reporter.console.print()
    reporter.table(
        "Results by Difficulty",
        headers,
        diff_rows,
        styles=["bold"] + [None] * len(conditions),
    )

    # ── Summary panel ──
    summary_lines = [
        f"Tasks: {len(results)}",
        f"Model: {client.model}",
        f"Modes: {cond_labels}",
        "",
    ]
    for cond in conditions:
        stats = cond_stats[cond.key]
        ci = stats.get("ci_95", (0.0, 0.0))
        summary_lines.append(
            f"{stats['label']:24s} coverage: {stats['mean']:.3f}"
            f"  95%CI: [{ci[0]:.3f}, {ci[1]:.3f}]"
            f"  halluc: {stats['hallucinations']}"
        )

    # Baseline delta + statistics
    if "single_blind" in cond_stats and "single_synapseed" in cond_stats:
        delta = (
            cond_stats["single_synapseed"]["mean"]
            - cond_stats["single_blind"]["mean"]
        )
        summary_lines.append(f"")
        summary_lines.append(f"Baseline delta (SYN-BLIND): {delta:+.3f}")
        if stat_results:
            p = stat_results["wilcoxon_p"]
            sig = f"p={p:.4f}" if p < 0.05 else f"p={p:.4f} (n.s.)"
            summary_lines.append(f"Wilcoxon: W={stat_results['wilcoxon_W']}, {sig}")
            summary_lines.append(f"Cohen's d: {stat_results['cohens_d']:.2f}")

    reporter.summary_panel(summary_lines)

    # ── Persist results ──
    aggregate = {}
    for cond in conditions:
        stats = cond_stats[cond.key]
        aggregate[cond.key] = {
            "label": stats["label"],
            "mean": stats["mean"],
            "ci_95": list(stats.get("ci_95", (0.0, 0.0))),
            "n": stats.get("n", 0),
            "hallucinations": stats["hallucinations"],
        }
    aggregate["by_difficulty"] = diff_data
    if stat_results:
        aggregate["statistics"] = stat_results

    # Backward-compatible keys
    if "single_blind" in cond_stats:
        aggregate["blind_mean"] = cond_stats["single_blind"]["mean"]
    if "single_synapseed" in cond_stats:
        aggregate["synapseed_mean"] = cond_stats["single_synapseed"]["mean"]
    if "single_blind" in cond_stats and "single_synapseed" in cond_stats:
        aggregate["delta"] = (
            cond_stats["single_synapseed"]["mean"]
            - cond_stats["single_blind"]["mean"]
        )

    suffix = client.model.replace("/", "_").replace(":", "_")
    if modes_tag != "baseline":
        suffix += f"_{modes_tag}"
    reporter.save(
        {
            "model": client.model,
            "modes": modes_tag,
            "conditions": [c.key for c in conditions],
            "tasks": results,
            "aggregate": aggregate,
        },
        suffix=suffix,
    )


def main():
    load_dotenv(os.path.join(os.path.dirname(os.path.dirname(__file__)), ".env"))

    parser = argparse.ArgumentParser(description="SYNAPSEED Coding Benchmark")
    parser.add_argument("--quick", action="store_true", help="Run only first 3 tasks")
    parser.add_argument("--target", help="Filter by target repo (e.g., synapseed, actix-web)")
    parser.add_argument("--all", action="store_true", help="Run all tasks including external targets")
    parser.add_argument("--model", help="Override LLM model name")
    parser.add_argument("--all-models", action="store_true", help="Run across all models in LLM_MODELS")
    parser.add_argument(
        "--modes",
        default="baseline",
        help=(
            "Comma-separated benchmark modes: baseline, prompt-opt, "
            "think-ablation, or 'all' (default: baseline)"
        ),
    )
    args = parser.parse_args()

    if args.all:
        tasks = get_tasks()
    elif args.target:
        tasks = get_tasks(args.target)
    else:
        tasks = get_self_tasks()

    if args.quick:
        tasks = tasks[:3]

    conditions = get_conditions(args.modes)
    modes_tag = args.modes.replace(",", "+")
    reporter = Reporter("Coding")

    reporter.console.print(
        f"  Conditions: {', '.join(c.label for c in conditions)}\n",
        style="dim",
    )

    if args.all_models:
        clients = LLMClient.all_from_env()
        reporter.console.print(f"  Multi-model run: {[c.model for c in clients]}\n")
        for client in clients:
            run_benchmark(tasks, client, reporter, conditions, modes_tag)
    else:
        client = LLMClient.from_env()
        if args.model:
            client.model = args.model
        run_benchmark(tasks, client, reporter, conditions, modes_tag)


if __name__ == "__main__":
    main()
