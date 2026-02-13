#!/usr/bin/env python3
"""Grounding Benchmark Runner: BLIND vs GROUNDED with F1 scoring.

Evaluates MCP tool effectiveness by comparing answers with and without
Synapseed context injection. Produces precision/recall/F1 per question
and aggregated statistics.

Usage:
    python -m benchmark.grounding.run                  # All 15 questions
    python -m benchmark.grounding.run --quick           # First 5 (easy only)
    python -m benchmark.grounding.run --difficulty hard  # Only hard questions
    python -m benchmark.grounding.run --type structural  # Only structural questions
"""

from __future__ import annotations

import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(__file__))))

from dotenv import load_dotenv

from benchmark.shared.llm import LLMClient, get_synapseed_context
from benchmark.shared.scoring import (
    grounding_rate,
    hallucination_count,
    keyword_score,
    file_score,
    symbol_score,
)
from benchmark.shared.reporting import Reporter
from benchmark.grounding.questions import GroundingQuestion, get_questions


def score_response(response: str, q: GroundingQuestion, repo_path: str) -> dict:
    """Score a response on a 0-3 scale plus sub-metrics."""
    ks = keyword_score(response, q.required_keywords)
    fs = file_score(response, q.required_files)
    ss = symbol_score(response, q.required_symbols)
    halluc = hallucination_count(response, q.forbidden_keywords, repo_path)
    gr = grounding_rate(response, repo_path)

    # Composite 0-3 score
    raw = (ks * 0.4 + fs * 0.3 + ss * 0.3) * 3.0
    # Penalize hallucinations
    raw = max(0.0, raw - halluc * 0.5)
    score = min(3.0, round(raw, 1))

    return {
        "score": score,
        "keyword_score": ks,
        "file_score": fs,
        "symbol_score": ss,
        "hallucinations": halluc,
        "grounding_rate": gr,
    }


def run_question(
    q: GroundingQuestion,
    client: LLMClient,
    repo_path: str,
    reporter: Reporter,
) -> dict:
    """Run BLIND + GROUNDED for a single question."""
    reporter.console.print(
        f"\n  [{q.difficulty.upper()}] {q.id}: {q.question[:60]}..."
    )

    # ── BLIND ──
    reporter.console.print("    BLIND...", style="dim")
    blind_resp = client.chat(q.question)
    blind_scores = score_response(blind_resp.content, q, repo_path)

    # ── GROUNDED ──
    reporter.console.print("    GROUNDED...", style="dim")
    context = get_synapseed_context(q.question, repo_path, raw=True)
    grounded_resp = client.chat(q.question, context=context)
    grounded_scores = score_response(grounded_resp.content, q, repo_path)

    delta = grounded_scores["score"] - blind_scores["score"]
    color = "green" if delta > 0 else ("red" if delta < 0 else "white")
    reporter.console.print(
        f"    Score: BLIND={blind_scores['score']:.1f}/3  "
        f"GROUNDED={grounded_scores['score']:.1f}/3  "
        f"[{color}]delta={delta:+.1f}[/{color}]"
    )

    return {
        "id": q.id,
        "question_type": q.question_type,
        "difficulty": q.difficulty,
        "question": q.question,
        "ground_truth": q.ground_truth_answer,
        "blind": {
            "content": blind_resp.content,
            "tokens": blind_resp.tokens_total,
            "latency_s": blind_resp.latency_s,
            **blind_scores,
        },
        "grounded": {
            "content": grounded_resp.content,
            "tokens": grounded_resp.tokens_total,
            "latency_s": grounded_resp.latency_s,
            "context_available": context is not None,
            **grounded_scores,
        },
        "delta": delta,
    }


def compute_f1(results: list[dict], mode: str) -> dict:
    """Compute precision, recall, F1 for a mode (blind/grounded)."""
    if not results:
        return {"precision": 0.0, "recall": 0.0, "f1": 0.0}

    # Precision: fraction of cited info that's correct (grounding rate)
    precisions = [r[mode]["grounding_rate"] for r in results]
    # Recall: fraction of ground truth info found (keyword+file+symbol avg)
    recalls = [
        (r[mode]["keyword_score"] + r[mode]["file_score"] + r[mode]["symbol_score"]) / 3
        for r in results
    ]

    p = sum(precisions) / len(precisions)
    r = sum(recalls) / len(recalls)
    f1 = 2 * p * r / (p + r) if (p + r) > 0 else 0.0

    return {"precision": round(p, 3), "recall": round(r, 3), "f1": round(f1, 3)}


def run_benchmark(questions: list[GroundingQuestion], client: LLMClient, reporter: Reporter):
    """Run the full grounding benchmark."""
    repo_path = os.path.dirname(os.path.dirname(os.path.dirname(__file__)))
    reporter.header(f"Model: {client.model} | Questions: {len(questions)}")

    results = []
    for q in questions:
        results.append(run_question(q, client, repo_path, reporter))

    if not results:
        reporter.console.print("[red]No questions executed.[/red]")
        return

    # ── Aggregate ──
    blind_total = sum(r["blind"]["score"] for r in results)
    grounded_total = sum(r["grounded"]["score"] for r in results)
    max_total = len(results) * 3.0
    blind_halluc = sum(r["blind"]["hallucinations"] for r in results)
    grounded_halluc = sum(r["grounded"]["hallucinations"] for r in results)
    perfect_blind = sum(1 for r in results if r["blind"]["score"] >= 2.5)
    perfect_grounded = sum(1 for r in results if r["grounded"]["score"] >= 2.5)

    blind_f1 = compute_f1(results, "blind")
    grounded_f1 = compute_f1(results, "grounded")

    # Results matrix
    rows = []
    for r in results:
        rows.append([
            r["id"],
            r["difficulty"],
            f"{r['blind']['score']:.1f}",
            f"{r['grounded']['score']:.1f}",
            f"{r['delta']:+.1f}",
        ])

    reporter.console.print()
    reporter.table(
        "Results Matrix",
        ["Question", "Difficulty", "BLIND", "GROUNDED", "Delta"],
        rows,
    )

    # F1 comparison
    reporter.console.print()
    reporter.table(
        "Precision / Recall / F1",
        ["Metric", "BLIND", "GROUNDED", "Delta"],
        [
            ["Precision", f"{blind_f1['precision']:.3f}", f"{grounded_f1['precision']:.3f}",
             f"{grounded_f1['precision'] - blind_f1['precision']:+.3f}"],
            ["Recall", f"{blind_f1['recall']:.3f}", f"{grounded_f1['recall']:.3f}",
             f"{grounded_f1['recall'] - blind_f1['recall']:+.3f}"],
            ["F1", f"{blind_f1['f1']:.3f}", f"{grounded_f1['f1']:.3f}",
             f"{grounded_f1['f1'] - blind_f1['f1']:+.3f}"],
        ],
    )

    reporter.summary_panel([
        f"Questions: {len(results)}",
        f"Model: {client.model}",
        f"BLIND:     {blind_total:.1f}/{max_total:.0f}  (perfect: {perfect_blind}, halluc: {blind_halluc})",
        f"GROUNDED:  {grounded_total:.1f}/{max_total:.0f}  (perfect: {perfect_grounded}, halluc: {grounded_halluc})",
        f"F1 lift:   {blind_f1['f1']:.3f} -> {grounded_f1['f1']:.3f}  ({grounded_f1['f1'] - blind_f1['f1']:+.3f})",
    ])

    reporter.save(
        {
            "model": client.model,
            "results": results,
            "aggregate": {
                "blind_total": blind_total,
                "grounded_total": grounded_total,
                "max_total": max_total,
                "blind_f1": blind_f1,
                "grounded_f1": grounded_f1,
                "blind_hallucinations": blind_halluc,
                "grounded_hallucinations": grounded_halluc,
            },
        },
        suffix=client.model.replace("/", "_").replace(":", "_"),
    )


def main():
    load_dotenv(os.path.join(os.path.dirname(os.path.dirname(__file__)), ".env"))

    parser = argparse.ArgumentParser(description="SYNAPSEED Grounding Benchmark")
    parser.add_argument("--quick", action="store_true", help="Run only first 5 questions")
    parser.add_argument("--difficulty", choices=["easy", "medium", "hard"])
    parser.add_argument("--type", dest="qtype", help="Question type filter")
    parser.add_argument("--model", help="Override LLM model name")
    parser.add_argument("--all-models", action="store_true", help="Run across all models in LLM_MODELS")
    args = parser.parse_args()

    questions = get_questions(difficulty=args.difficulty, question_type=args.qtype)
    if args.quick:
        questions = questions[:5]

    reporter = Reporter("Grounding")

    if args.all_models:
        clients = LLMClient.all_from_env()
        reporter.console.print(f"  Multi-model run: {[c.model for c in clients]}\n")
        for client in clients:
            run_benchmark(questions, client, reporter)
    else:
        client = LLMClient.from_env()
        if args.model:
            client.model = args.model
        run_benchmark(questions, client, reporter)


if __name__ == "__main__":
    main()
