"""Scoring utilities for benchmark evaluation.

Metric definitions (for paper Methodology section):

    keyword_recall(R, K):  |{k ∈ K : k ⊆ R}| / |K|
        Fraction of required keywords found in response R.
        Uses word-boundary matching for short tokens to prevent
        substring false positives (e.g., 'Low' matching 'following').

    file_recall(R, F):  |{f ∈ F : f ⊆ R}| / |F|
        Fraction of required file paths found (with suffix matching).

    symbol_recall(R, S):  |{s ∈ S : s ⊆ R}| / |S|
        Fraction of required symbol names found (word-boundary).

    coverage_score(R, K, F, S):
        W_k · keyword_recall + W_f · file_recall + W_s · symbol_recall
        Default weights: W_k=0.4, W_f=0.3, W_s=0.3
        Range: [0, 1]. NOT an F1 score — it is a weighted recall metric
        over three evidence types.

    citation_precision(R, repo):
        |{p ∈ cited_paths(R) : exists(repo/p)}| / |cited_paths(R)|
        Returns NaN when no paths cited (excluded from aggregation).

    hallucination_count(R, forbidden, repo):
        Count of (a) forbidden keywords + (b) non-existent cited paths.
        Deduplicated.

All scorers return float in [0.0, 1.0] unless documented otherwise.
"""

from __future__ import annotations

import math
import os
import re
import statistics


# ── Path extraction ──────────────────────────────────────────────────
_PATH_RE = re.compile(
    r'(?:src|crates|lib|bin|docs|benchmark|tests|tools|examples|vscode-extension|experiments)'
    r'/[\w/.-]+\.\w+'
)
_ROOT_FILES_RE = re.compile(
    r'\b(Cargo\.toml|README\.md|CHANGELOG\.md|CONTRIBUTING\.md|'
    r'SECURITY\.md|FAQ\.md|LICENSE|rust-toolchain\.toml)\b'
)


def _extract_file_refs(response: str) -> list[str]:
    """Extract deduplicated file path references from a response."""
    refs = _PATH_RE.findall(response)
    refs += _ROOT_FILES_RE.findall(response)
    seen: set[str] = set()
    unique: list[str] = []
    for ref in refs:
        if ref not in seen:
            seen.add(ref)
            unique.append(ref)
    return unique


# ── Core scoring functions ───────────────────────────────────────────

def keyword_recall(response: str, keywords: list[str]) -> float:
    """Fraction of required keywords found in response.

    Short keywords (≤ 6 chars) use word-boundary matching to avoid
    substring false positives. Longer keywords use substring matching.
    """
    if not keywords:
        return 1.0
    response_lower = response.lower()
    hits = 0
    for kw in keywords:
        if len(kw) <= 6:
            if re.search(r'\b' + re.escape(kw) + r'\b', response, re.IGNORECASE):
                hits += 1
        else:
            if kw.lower() in response_lower:
                hits += 1
    return hits / len(keywords)


keyword_score = keyword_recall  # backward compat


def file_recall(response: str, files: list[str]) -> float:
    """Fraction of required file paths found in response.

    Uses suffix matching: 'crates/husk/src/scanner.rs' also matches
    if the response mentions 'husk/src/scanner.rs'.
    """
    if not files:
        return 1.0
    hits = 0
    for f in files:
        if f in response:
            hits += 1
            continue
        # Suffix match
        parts = f.split('/')
        for i in range(1, len(parts)):
            suffix = '/'.join(parts[i:])
            if suffix in response:
                hits += 1
                break
    return hits / len(files)


file_score = file_recall


def symbol_recall(response: str, symbols: list[str]) -> float:
    """Fraction of required symbol names found in response (word-boundary)."""
    if not symbols:
        return 1.0
    hits = sum(
        1 for s in symbols
        if re.search(r'\b' + re.escape(s) + r'\b', response)
    )
    return hits / len(symbols)


symbol_score = symbol_recall


def hallucination_count(
    response: str,
    forbidden_keywords: list[str] | None = None,
    repo_path: str | None = None,
) -> int:
    """Count hallucination signals in a response.

    1. Forbidden keywords that should NOT appear (word-boundary matched).
    2. File paths cited in the response that don't exist on disk.
    """
    count = 0

    if forbidden_keywords:
        for kw in forbidden_keywords:
            if re.search(r'\b' + re.escape(kw) + r'\b', response, re.IGNORECASE):
                count += 1

    if repo_path:
        for ref in _extract_file_refs(response):
            if not os.path.exists(os.path.join(repo_path, ref)):
                count += 1

    return count


def code_quality_score(response: str, code_pattern: str | None = None) -> float:
    """Score code quality: has code blocks? Matches expected pattern?"""
    has_code = "```" in response or "fn " in response or "def " in response
    if not has_code:
        return 0.0
    if code_pattern and not re.search(code_pattern, response):
        return 0.5
    return 1.0


def coverage_score(
    response: str,
    keywords: list[str],
    files: list[str],
    symbols: list[str],
    *,
    keyword_weight: float = 0.4,
    file_weight: float = 0.3,
    symbol_weight: float = 0.3,
) -> float:
    """Weighted coverage of ground-truth elements.

    CS = W_k · keyword_recall + W_f · file_recall + W_s · symbol_recall

    NOT an F1 score. This is a weighted recall over three evidence types.
    Weights: keywords (0.4) carry the most signal since models can answer
    correctly without citing specific files; file (0.3) and symbol (0.3)
    mentions confirm source grounding.
    """
    ks = keyword_recall(response, keywords)
    fs = file_recall(response, files)
    ss = symbol_recall(response, symbols)
    return ks * keyword_weight + fs * file_weight + ss * symbol_weight


composite_score = coverage_score  # backward compat


def citation_precision(response: str, repo_path: str) -> float:
    """Fraction of cited file paths that actually exist on disk.

    Returns NaN when no file paths are cited — callers MUST handle
    this by excluding NaN values from aggregation. This is standard
    IR practice (undefined precision at recall=0).

    This is NOT IR precision (relevant / retrieved). It measures
    citation accuracy: are the paths the model mentions real files?
    """
    file_refs = _extract_file_refs(response)
    if not file_refs:
        return float('nan')
    valid = sum(1 for ref in file_refs if os.path.exists(os.path.join(repo_path, ref)))
    return valid / len(file_refs)


grounding_rate = citation_precision  # backward compat


# ── Statistical utilities ────────────────────────────────────────────

def bootstrap_ci(
    values: list[float],
    n_boot: int = 10000,
    alpha: float = 0.05,
    statistic: str = "mean",
) -> tuple[float, float, float]:
    """Bootstrap confidence interval (percentile method).

    Returns (point_estimate, ci_lower, ci_upper).
    Filters out NaN values before computation.
    """
    import random
    clean = [v for v in values if not math.isnan(v)]
    if not clean:
        return (0.0, 0.0, 0.0)

    stat_fn = statistics.mean if statistic == "mean" else statistics.median
    point = stat_fn(clean)

    if len(clean) < 2:
        return (point, point, point)

    rng = random.Random(42)  # reproducible
    boot_stats = []
    for _ in range(n_boot):
        sample = rng.choices(clean, k=len(clean))
        boot_stats.append(stat_fn(sample))

    boot_stats.sort()
    lo = boot_stats[int(n_boot * alpha / 2)]
    hi = boot_stats[int(n_boot * (1 - alpha / 2))]
    return (round(point, 4), round(lo, 4), round(hi, 4))


def cohens_d(group1: list[float], group2: list[float]) -> float:
    """Cohen's d effect size for paired samples.

    d = mean(diff) / SD(diff)  where diff = group2 - group1
    """
    if len(group1) != len(group2) or len(group1) < 2:
        return 0.0
    diffs = [g2 - g1 for g1, g2 in zip(group1, group2)]
    mean_diff = statistics.mean(diffs)
    sd_diff = statistics.stdev(diffs)
    if sd_diff == 0:
        return float('inf') if mean_diff > 0 else 0.0
    return round(mean_diff / sd_diff, 3)


def wilcoxon_signed_rank(
    group1: list[float],
    group2: list[float],
) -> tuple[float, float]:
    """Wilcoxon signed-rank test for paired samples.

    Returns (W_statistic, approximate_p_value).
    Uses normal approximation (valid for n ≥ 10; approximate for n < 10).
    """
    if len(group1) != len(group2):
        raise ValueError("Groups must have equal length")

    diffs = [(i, g2 - g1) for i, (g1, g2) in enumerate(zip(group1, group2))]
    diffs = [(i, d) for i, d in diffs if d != 0]
    if not diffs:
        return (0.0, 1.0)

    n = len(diffs)
    ranked = sorted(diffs, key=lambda x: abs(x[1]))
    ranks: dict[int, float] = {}
    i = 0
    while i < n:
        j = i
        while j < n and abs(ranked[j][1]) == abs(ranked[i][1]):
            j += 1
        avg_rank = (i + 1 + j) / 2
        for k in range(i, j):
            ranks[ranked[k][0]] = avg_rank
        i = j

    w_plus = sum(ranks[idx] for idx, d in diffs if d > 0)
    w_minus = sum(ranks[idx] for idx, d in diffs if d < 0)
    W = min(w_plus, w_minus)

    mean_w = n * (n + 1) / 4
    var_w = n * (n + 1) * (2 * n + 1) / 24
    if var_w == 0:
        return (W, 1.0)

    z = (W - mean_w) / math.sqrt(var_w)
    p = 2 * _norm_cdf(-abs(z))
    return (round(W, 2), round(p, 4))


def _norm_cdf(z: float) -> float:
    """Standard normal CDF (Abramowitz & Stegun 26.2.17)."""
    if z < -8:
        return 0.0
    if z > 8:
        return 1.0
    a1, a2, a3, a4, a5 = 0.254829592, -0.284496736, 1.421413741, -1.453152027, 1.061405429
    p_coeff = 0.3275911
    t = 1.0 / (1.0 + p_coeff * abs(z))
    y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * math.exp(-z * z / 2)
    return y if z >= 0 else 1.0 - y
