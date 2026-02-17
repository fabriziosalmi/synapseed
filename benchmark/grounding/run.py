#!/usr/bin/env python3
"""Grounding Benchmark Runner: BLIND vs GROUNDED evaluation.

Evaluates MCP tool effectiveness by comparing answers with and without
Synapseed context injection.

Metrics:
    coverage_score (CS): Weighted recall over keyword/file/symbol ground truth.
    citation_precision (CP): Fraction of cited file paths that exist on disk.
    grounding_quality_index (GQI): Harmonic mean of CP and CS.
        GQI = 2·CP·CS / (CP + CS)  when CP is defined.
        GQI = CS                   when CP is undefined (no citations).
    hallucination_count: Forbidden keywords + non-existent cited paths.

Note: GQI is NOT a standard F1 score. It combines two domain-specific
metrics and should be reported as such in publications.

Usage:
    python -m benchmark.grounding.run                  # All 15 questions
    python -m benchmark.grounding.run --quick           # First 5 (easy only)
    python -m benchmark.grounding.run --difficulty hard  # Only hard questions
    python -m benchmark.grounding.run --type structural  # Only structural questions
"""

from __future__ import annotations

import argparse
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(__file__))))

from dotenv import load_dotenv

from benchmark.shared.llm import LLMClient, get_synapseed_context
from benchmark.shared.scoring import (
    citation_precision,
    hallucination_count,
    keyword_recall,
    file_recall,
    symbol_recall,
    bootstrap_ci,
    cohens_d,
    wilcoxon_signed_rank,
)
from benchmark.shared.reporting import Reporter
from benchmark.grounding.questions import GroundingQuestion, get_questions


def score_response(response: str, q: GroundingQuestion, repo_path: str) -> dict:
    """Score a response on a 0-3 scale plus sub-metrics.

    Returns all raw sub-scores so consumers can recompute aggregates.
    citation_precision may be NaN (no file citations) — callers must handle.
    """
    ks = keyword_recall(response, q.required_keywords)
    fs = file_recall(response, q.required_files)
    ss = symbol_recall(response, q.required_symbols)
    halluc = hallucination_count(response, q.forbidden_keywords, repo_path)
    cp = citation_precision(response, repo_path)

    # Coverage score: weighted recall, range [0, 1]
    cs = ks * 0.4 + fs * 0.3 + ss * 0.3

    # Composite 0-3 score (legacy, for backward compat)
    raw = cs * 3.0
    raw = max(0.0, raw - halluc * 0.5)
    score = min(3.0, round(raw, 1))

    return {
        "score": score,
        "coverage_score": round(cs, 4),
        "keyword_recall": ks,
        "file_recall": fs,
        "symbol_recall": ss,
        "hallucinations": halluc,
        "citation_precision": cp,  # NaN when no citations
        # Backward compat aliases
        "keyword_score": ks,
        "file_score": fs,
        "symbol_score": ss,
        "grounding_rate": cp if not math.isnan(cp) else 0.5,
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


def compute_metrics(results: list[dict], mode: str) -> dict:
    """Compute grounding quality metrics for a mode (blind/grounded).

    Metrics:
        coverage_score (CS): Mean weighted recall over keyword/file/symbol.
        citation_precision (CP): Mean fraction of cited paths that exist.
            Excludes responses with no citations (NaN) from the average.
        GQI: Harmonic mean of CP and CS (like F1 but explicitly not F1).
        hallucinations: Total count across all responses.
        coverage_ci: 95% bootstrap CI for coverage_score.
    """
    if not results:
        return {
            "coverage_score": 0.0, "citation_precision": 0.0,
            "gqi": 0.0, "hallucinations": 0,
            "coverage_ci": (0.0, 0.0, 0.0),
            # Legacy keys for backward compat with paper generator
            "precision": 0.0, "recall": 0.0, "f1": 0.0,
        }

    # Coverage scores (always defined)
    cs_values = [r[mode]["coverage_score"] for r in results]
    cs_mean, cs_lo, cs_hi = bootstrap_ci(cs_values)

    # Citation precision (NaN-aware: exclude responses with no citations)
    cp_raw = [r[mode]["citation_precision"] for r in results]
    cp_defined = [v for v in cp_raw if not math.isnan(v)]
    cp_mean = sum(cp_defined) / len(cp_defined) if cp_defined else float('nan')
    n_with_citations = len(cp_defined)

    # Grounding Quality Index: harmonic mean of CP and CS
    if math.isnan(cp_mean) or cp_mean + cs_mean == 0:
        gqi = cs_mean  # Fallback to coverage when CP is undefined
    else:
        gqi = 2 * cp_mean * cs_mean / (cp_mean + cs_mean)

    # Hallucinations
    total_halluc = sum(r[mode]["hallucinations"] for r in results)

    return {
        "coverage_score": round(cs_mean, 4),
        "coverage_ci": (cs_lo, cs_hi),
        "citation_precision": round(cp_mean, 4) if not math.isnan(cp_mean) else 0.0,
        "n_with_citations": n_with_citations,
        "gqi": round(gqi, 4),
        "hallucinations": total_halluc,
        # Backward compat (paper generator reads these keys)
        "precision": round(cp_mean, 3) if not math.isnan(cp_mean) else 0.0,
        "recall": round(cs_mean, 3),
        "f1": round(gqi, 3),
    }


# Legacy alias
compute_f1 = compute_metrics


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

    blind_metrics = compute_metrics(results, "blind")
    grounded_metrics = compute_metrics(results, "grounded")

    # ── Statistical tests ──
    blind_cs = [r["blind"]["coverage_score"] for r in results]
    grounded_cs = [r["grounded"]["coverage_score"] for r in results]
    effect_d = cohens_d(blind_cs, grounded_cs)
    W, p_value = wilcoxon_signed_rank(blind_cs, grounded_cs)

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

    # Metrics comparison
    reporter.console.print()
    bm, gm = blind_metrics, grounded_metrics
    reporter.table(
        "Citation Precision / Coverage / GQI",
        ["Metric", "BLIND", "GROUNDED", "Delta"],
        [
            ["Citation Prec.", f"{bm['citation_precision']:.3f}", f"{gm['citation_precision']:.3f}",
             f"{gm['citation_precision'] - bm['citation_precision']:+.3f}"],
            ["Coverage Score", f"{bm['coverage_score']:.3f}", f"{gm['coverage_score']:.3f}",
             f"{gm['coverage_score'] - bm['coverage_score']:+.3f}"],
            ["GQI", f"{bm['gqi']:.3f}", f"{gm['gqi']:.3f}",
             f"{gm['gqi'] - bm['gqi']:+.3f}"],
        ],
    )

    # Statistics
    sig_str = f"p={p_value:.4f}" if p_value < 0.05 else f"p={p_value:.4f} (n.s.)"
    cs_ci = grounded_metrics['coverage_ci']
    reporter.summary_panel([
        f"Questions: {len(results)}",
        f"Model: {client.model}",
        f"BLIND:     {blind_total:.1f}/{max_total:.0f}  (perfect: {perfect_blind}, halluc: {blind_halluc})",
        f"GROUNDED:  {grounded_total:.1f}/{max_total:.0f}  (perfect: {perfect_grounded}, halluc: {grounded_halluc})",
        f"GQI lift:  {bm['gqi']:.3f} → {gm['gqi']:.3f}  (Δ={gm['gqi'] - bm['gqi']:+.3f})",
        f"Coverage 95% CI: [{cs_ci[0]:.3f}, {cs_ci[1]:.3f}]",
        f"Wilcoxon: W={W}, {sig_str}",
        f"Cohen's d: {effect_d:.2f}",
    ])

    reporter.save(
        {
            "model": client.model,
            "results": results,
            "aggregate": {
                "blind_total": blind_total,
                "grounded_total": grounded_total,
                "max_total": max_total,
                "blind_f1": blind_metrics,
                "grounded_f1": grounded_metrics,
                "blind_hallucinations": blind_halluc,
                "grounded_hallucinations": grounded_halluc,
                "statistics": {
                    "wilcoxon_W": W,
                    "wilcoxon_p": p_value,
                    "cohens_d": effect_d,
                    "n": len(results),
                    "grounded_coverage_ci_95": list(cs_ci),
                },
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
