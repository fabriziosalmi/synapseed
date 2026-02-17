#!/usr/bin/env python3
"""
Synapseed Regression Detector — compare two bench reports and flag regressions.

Usage:
    python tools/regression.py baseline.json candidate.json [--threshold 0.05]

Compares quality metrics (F1, precision, recall, SID, SCR, hallucination rate)
and latency metrics (mean, p95, min, max) between two bench report JSON files.

Exit codes:
    0 = no regressions
    1 = regressions detected
    2 = input error

Thresholds (configurable via --threshold):
    Quality: F1/precision/recall must not drop by more than threshold (default 5%)
    Latency: mean/p95 must not increase by more than 2x threshold (default 10%)
    Hallucination: rate must not increase by more than threshold (default 5%)
"""

import json
import sys
import argparse
from pathlib import Path
from dataclasses import dataclass


@dataclass
class Comparison:
    metric: str
    baseline: float
    candidate: float
    delta: float
    delta_pct: float
    status: str  # "ok", "improved", "regression"
    category: str  # "quality", "latency", "size"


def load_report(path: str) -> dict:
    with open(path) as f:
        return json.load(f)


def compare_reports(baseline: dict, candidate: dict, threshold: float) -> list[Comparison]:
    ba = baseline["aggregate"]
    ca = candidate["aggregate"]

    comparisons = []

    # Quality metrics (higher is better)
    for metric in ["mean_f1", "mean_precision", "mean_recall", "mean_sid", "mean_scr"]:
        bv = ba.get(metric, 0.0)
        cv = ca.get(metric, 0.0)
        delta = cv - bv
        delta_pct = (delta / bv * 100) if bv != 0 else 0
        if delta < -threshold * bv and bv > 0:
            status = "regression"
        elif delta > threshold * bv and bv > 0:
            status = "improved"
        else:
            status = "ok"
        comparisons.append(Comparison(metric, bv, cv, delta, delta_pct, status, "quality"))

    # Hallucination rate (lower is better)
    bv = ba.get("hallucination_rate", 0.0)
    cv = ca.get("hallucination_rate", 0.0)
    delta = cv - bv
    delta_pct = (delta / bv * 100) if bv != 0 else (100 if cv > 0 else 0)
    if delta > threshold:
        status = "regression"
    elif delta < -threshold:
        status = "improved"
    else:
        status = "ok"
    comparisons.append(Comparison("hallucination_rate", bv, cv, delta, delta_pct, status, "quality"))

    # Latency metrics (lower is better) — use 2x threshold
    latency_threshold = threshold * 2
    for metric in ["mean_latency_ms", "p95_latency_ms"]:
        bv = ba.get(metric, 0.0)
        cv = ca.get(metric, 0.0)
        if bv == 0:
            comparisons.append(Comparison(metric, bv, cv, cv, 0, "ok", "latency"))
            continue
        delta = cv - bv
        delta_pct = (delta / bv * 100)
        if delta > latency_threshold * bv:
            status = "regression"
        elif delta < -latency_threshold * bv:
            status = "improved"
        else:
            status = "ok"
        comparisons.append(Comparison(metric, bv, cv, delta, delta_pct, status, "latency"))

    # Difficulty breakdown
    for metric in ["easy_mean_f1", "medium_mean_f1", "hard_mean_f1"]:
        bv = ba.get(metric, 0.0)
        cv = ca.get(metric, 0.0)
        delta = cv - bv
        delta_pct = (delta / bv * 100) if bv != 0 else 0
        if delta < -threshold * bv and bv > 0:
            status = "regression"
        elif delta > threshold * bv and bv > 0:
            status = "improved"
        else:
            status = "ok"
        comparisons.append(Comparison(metric, bv, cv, delta, delta_pct, status, "quality"))

    return comparisons


def per_question_diff(baseline: dict, candidate: dict) -> list[dict]:
    """Compare individual questions between reports."""
    b_questions = {q["id"]: q for q in baseline.get("questions", [])}
    c_questions = {q["id"]: q for q in candidate.get("questions", [])}

    diffs = []
    for qid in sorted(set(b_questions) | set(c_questions)):
        bq = b_questions.get(qid)
        cq = c_questions.get(qid)
        if bq and cq:
            f1_delta = cq["f1"] - bq["f1"]
            latency_delta = cq.get("latency_ms", 0) - bq.get("latency_ms", 0)
            diffs.append({
                "id": qid,
                "difficulty": cq.get("difficulty", "unknown"),
                "f1_baseline": bq["f1"],
                "f1_candidate": cq["f1"],
                "f1_delta": f1_delta,
                "latency_baseline_ms": bq.get("latency_ms", 0),
                "latency_candidate_ms": cq.get("latency_ms", 0),
                "latency_delta_ms": latency_delta,
                "bottleneck": cq.get("bottleneck", "unknown"),
            })
        elif cq:
            diffs.append({"id": qid, "status": "new_question"})
        else:
            diffs.append({"id": qid, "status": "removed_question"})
    return diffs


def format_report(comparisons: list[Comparison], per_q: list[dict],
                  baseline_meta: dict, candidate_meta: dict) -> str:
    lines = []
    lines.append("# Regression Report")
    lines.append("")
    lines.append(f"**Baseline**: {baseline_meta.get('timestamp', '?')} ({baseline_meta.get('version', '?')})")
    lines.append(f"**Candidate**: {candidate_meta.get('timestamp', '?')} ({candidate_meta.get('version', '?')})")
    lines.append(f"**Suite**: {baseline_meta.get('suite_path', '?')}")
    lines.append("")

    regressions = [c for c in comparisons if c.status == "regression"]
    improvements = [c for c in comparisons if c.status == "improved"]

    if regressions:
        lines.append(f"## ❌ {len(regressions)} REGRESSION(S) DETECTED")
    else:
        lines.append("## ✅ NO REGRESSIONS")
    lines.append("")

    # Summary table
    lines.append("| Metric | Baseline | Candidate | Delta | Status |")
    lines.append("|--------|----------|-----------|-------|--------|")
    for c in comparisons:
        icon = {"ok": "✅", "improved": "🟢", "regression": "❌"}[c.status]
        if c.category == "latency":
            lines.append(f"| {c.metric} | {c.baseline:.1f}ms | {c.candidate:.1f}ms | {c.delta:+.1f}ms ({c.delta_pct:+.1f}%) | {icon} |")
        elif "rate" in c.metric:
            lines.append(f"| {c.metric} | {c.baseline:.1%} | {c.candidate:.1%} | {c.delta:+.3f} | {icon} |")
        else:
            lines.append(f"| {c.metric} | {c.baseline:.3f} | {c.candidate:.3f} | {c.delta:+.3f} ({c.delta_pct:+.1f}%) | {icon} |")
    lines.append("")

    if improvements:
        lines.append(f"### 🟢 Improvements: {', '.join(c.metric for c in improvements)}")
        lines.append("")

    # Per-question regressions (F1 dropped > 0.1)
    q_regressions = [q for q in per_q if isinstance(q.get("f1_delta"), (int, float)) and q["f1_delta"] < -0.1]
    if q_regressions:
        lines.append("## Per-Question Regressions (F1 drop > 0.1)")
        lines.append("")
        lines.append("| ID | Difficulty | F1 Before | F1 After | Delta | Bottleneck |")
        lines.append("|-----|-----------|-----------|----------|-------|------------|")
        for q in sorted(q_regressions, key=lambda x: x["f1_delta"]):
            lines.append(
                f"| {q['id']} | {q['difficulty']} | {q['f1_baseline']:.2f} | "
                f"{q['f1_candidate']:.2f} | {q['f1_delta']:+.2f} | {q.get('bottleneck', '?')} |"
            )
        lines.append("")

    # Slowest questions
    slow_qs = sorted(
        [q for q in per_q if "latency_candidate_ms" in q],
        key=lambda x: x.get("latency_candidate_ms", 0),
        reverse=True
    )[:5]
    if slow_qs:
        lines.append("## Top 5 Slowest Questions")
        lines.append("")
        lines.append("| ID | Latency | Delta | Bottleneck |")
        lines.append("|-----|---------|-------|------------|")
        for q in slow_qs:
            lines.append(
                f"| {q['id']} | {q.get('latency_candidate_ms', 0):.1f}ms | "
                f"{q.get('latency_delta_ms', 0):+.1f}ms | {q.get('bottleneck', '?')} |"
            )

    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description="Compare two Synapseed bench reports for regressions")
    parser.add_argument("baseline", help="Path to baseline bench report JSON")
    parser.add_argument("candidate", help="Path to candidate bench report JSON")
    parser.add_argument("--threshold", type=float, default=0.05,
                        help="Regression threshold (0.05 = 5%% quality, 10%% latency)")
    parser.add_argument("--json", action="store_true", help="Output raw JSON instead of markdown")
    args = parser.parse_args()

    try:
        baseline = load_report(args.baseline)
        candidate = load_report(args.candidate)
    except (FileNotFoundError, json.JSONDecodeError) as e:
        print(f"Error loading reports: {e}", file=sys.stderr)
        sys.exit(2)

    comparisons = compare_reports(baseline, candidate, args.threshold)
    per_q = per_question_diff(baseline, candidate)

    if args.json:
        output = {
            "comparisons": [vars(c) for c in comparisons],
            "per_question": per_q,
            "has_regressions": any(c.status == "regression" for c in comparisons),
        }
        print(json.dumps(output, indent=2))
    else:
        report = format_report(
            comparisons, per_q,
            baseline.get("metadata", {}),
            candidate.get("metadata", {}),
        )
        print(report)

    has_regressions = any(c.status == "regression" for c in comparisons)
    sys.exit(1 if has_regressions else 0)


if __name__ == "__main__":
    main()
