"""Scoring utilities for benchmark evaluation.

All scorers take a response string and ground-truth data,
returning a float in [0.0, 1.0].
"""

from __future__ import annotations

import os
import re


def keyword_score(response: str, keywords: list[str]) -> float:
    """Fraction of required keywords found in response (case-insensitive)."""
    if not keywords:
        return 1.0
    response_lower = response.lower()
    hits = sum(1 for kw in keywords if kw.lower() in response_lower)
    return hits / len(keywords)


def file_score(response: str, files: list[str]) -> float:
    """Fraction of required file paths found in response."""
    if not files:
        return 1.0
    hits = sum(1 for f in files if f in response)
    return hits / len(files)


def symbol_score(response: str, symbols: list[str]) -> float:
    """Fraction of required symbol names found in response."""
    if not symbols:
        return 1.0
    hits = sum(1 for s in symbols if s in response)
    return hits / len(symbols)


def hallucination_count(
    response: str,
    forbidden_keywords: list[str] | None = None,
    repo_path: str | None = None,
) -> int:
    """Count hallucination signals in a response.

    1. Forbidden keywords that should NOT appear (wrong facts).
    2. File paths cited in the response that don't exist on disk.
    """
    count = 0

    # Forbidden keywords
    if forbidden_keywords:
        resp_lower = response.lower()
        count += sum(1 for kw in forbidden_keywords if kw.lower() in resp_lower)

    # Physical file validation
    if repo_path:
        # Match patterns like `src/foo/bar.rs`, `crates/x/src/lib.rs`
        file_refs = re.findall(r'(?:src|crates|lib|bin)/[\w/.-]+\.\w+', response)
        for ref in file_refs:
            full = os.path.join(repo_path, ref)
            if not os.path.exists(full):
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


def composite_score(
    response: str,
    keywords: list[str],
    files: list[str],
    symbols: list[str],
    *,
    keyword_weight: float = 0.4,
    file_weight: float = 0.3,
    symbol_weight: float = 0.3,
) -> float:
    """Weighted composite of keyword, file, and symbol scores."""
    ks = keyword_score(response, keywords)
    fs = file_score(response, files)
    ss = symbol_score(response, symbols)
    return ks * keyword_weight + fs * file_weight + ss * symbol_weight


def grounding_rate(response: str, repo_path: str) -> float:
    """Fraction of cited file paths that actually exist on disk."""
    file_refs = re.findall(r'(?:src|crates|lib|bin)/[\w/.-]+\.\w+', response)
    if not file_refs:
        return 1.0  # No citations = no false citations
    valid = sum(1 for ref in file_refs if os.path.exists(os.path.join(repo_path, ref)))
    return valid / len(file_refs)
