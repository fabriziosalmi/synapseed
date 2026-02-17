"""Stratified question bank for the grounding benchmark.

Questions are categorized by type and difficulty.
Ground truth is defined with precision to enable automated scoring.

Question types:
- factual_exact: Single correct answer (version, count)
- factual_count: Enumeration with known cardinality
- structural:    Type/location of specific code entities
- behavioral:    How specific code behaves at runtime
- cross_crate:   Relationships spanning multiple crates
"""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class GroundingQuestion:
    """A question with ground truth for grounding evaluation."""

    id: str
    question_type: str  # factual_exact | factual_count | structural | behavioral | cross_crate
    difficulty: str  # easy | medium | hard
    question: str
    ground_truth_answer: str  # Human-verified correct answer
    required_keywords: list[str] = field(default_factory=list)
    required_files: list[str] = field(default_factory=list)
    required_symbols: list[str] = field(default_factory=list)
    forbidden_keywords: list[str] = field(default_factory=list)
    max_score: int = 3  # 0=wrong, 1=partial, 2=mostly correct, 3=perfect


# ── SYNAPSEED self-hosted questions ──────────────────────────────────
# These are validated against the actual codebase and should be updated
# when the codebase changes significantly (version bumps, API changes).

QUESTIONS: list[GroundingQuestion] = [
    # ── Factual Exact ──
    GroundingQuestion(
        id="g01_version",
        question_type="factual_exact",
        difficulty="easy",
        question="What is the workspace version in Cargo.toml?",
        ground_truth_answer="4.23.0",
        required_keywords=["4.23"],
        required_files=["Cargo.toml"],
    ),
    GroundingQuestion(
        id="g02_license",
        question_type="factual_exact",
        difficulty="easy",
        question="What license is SYNAPSEED released under?",
        ground_truth_answer="Apache-2.0",
        required_keywords=["Apache-2.0"],
        required_files=["Cargo.toml"],
    ),
    # ── Factual Count ──
    GroundingQuestion(
        id="g03_crate_count",
        question_type="factual_count",
        difficulty="easy",
        question="How many crates are in the workspace?",
        ground_truth_answer="16 (15 lib + 1 bin)",
        required_keywords=["16"],
        required_files=["Cargo.toml"],
    ),
    GroundingQuestion(
        id="g04_tool_count",
        question_type="factual_count",
        difficulty="medium",
        question="How many canonical MCP tools does SYNAPSEED register?",
        ground_truth_answer="24 tools in TOOL_NAMES constant",
        required_keywords=["24"],
        required_symbols=["TOOL_NAMES"],
    ),
    GroundingQuestion(
        id="g05_intent_variants",
        question_type="factual_count",
        difficulty="medium",
        question="What are the 5 intent categories in the whisper router?",
        ground_truth_answer="BugFix, Security, Explain, Refactor, General",
        required_keywords=["BugFix", "Security", "Explain", "Refactor", "General"],
        required_files=["crates/whisper/src/router/mod.rs"],
        required_symbols=["Intent"],
    ),
    # ── Structural ──
    GroundingQuestion(
        id="g06_dlp_finding",
        question_type="structural",
        difficulty="medium",
        question="Where is the DLP Finding struct defined? What visibility does it have?",
        ground_truth_answer="Finding in crates/husk/src/scanner.rs, pub(crate)",
        required_keywords=["Finding"],
        required_files=["crates/husk/src/scanner.rs"],
        required_symbols=["Finding"],
    ),
    GroundingQuestion(
        id="g07_severity_enum",
        question_type="structural",
        difficulty="easy",
        question="What are the variants of the Severity enum in the event system?",
        ground_truth_answer="Low, Medium, High, Critical",
        required_keywords=["Low", "Medium", "High", "Critical"],
        required_files=["crates/core/src/event.rs"],
        required_symbols=["Severity"],
    ),
    GroundingQuestion(
        id="g08_code_patterns",
        question_type="structural",
        difficulty="medium",
        question="What categories does the CodePatternScanner detect?",
        ground_truth_answer="SQL injection, XSS, command injection, path traversal",
        required_keywords=["injection", "XSS"],
        required_symbols=["CodePatternScanner"],
    ),
    # ── Behavioral ──
    GroundingQuestion(
        id="g09_sandbox_isolation",
        question_type="behavioral",
        difficulty="hard",
        question="How does the Gym sandbox prevent network access during code evaluation?",
        ground_truth_answer="Uses [net] offline = true in .cargo/config.toml",
        required_keywords=["offline"],
        required_files=["crates/gym/src/sandbox.rs"],
        forbidden_keywords=["seccomp", "namespace", "cgroup"],
    ),
    GroundingQuestion(
        id="g10_fuzzy_matching",
        question_type="behavioral",
        difficulty="hard",
        question="How does handle_tool_call resolve fuzzy tool name matches?",
        ground_truth_answer="3-tier: Levenshtein <= 3, NL redirect, suggestion list",
        required_keywords=["Levenshtein"],
        required_symbols=["handle_tool_call"],
    ),
    GroundingQuestion(
        id="g11_shadow_debounce",
        question_type="behavioral",
        difficulty="hard",
        question="What are the adaptive debounce parameters in shadow-check?",
        ground_truth_answer="Initial 2s, max 5s, trigger threshold 3",
        required_keywords=["debounce"],
        required_files=["crates/shadow-check/src/runner.rs"],
    ),
    # ── Cross-crate ──
    GroundingQuestion(
        id="g12_event_flow",
        question_type="cross_crate",
        difficulty="hard",
        question=(
            "Trace the flow of a FileChanged event from detection to indexing. "
            "Which crates are involved?"
        ),
        ground_truth_answer="root (watcher) -> core (event) -> cortex (parser) -> search (indexer)",
        required_keywords=["FileChanged", "event"],
        required_files=["crates/core/src/event.rs"],
        required_symbols=["SynapseEvent", "FileChanged"],
    ),
    GroundingQuestion(
        id="g13_coherence_gate",
        question_type="cross_crate",
        difficulty="hard",
        question=(
            "What is the Coherence Gate? What threshold triggers it "
            "and how does model tier affect clustering?"
        ),
        ground_truth_answer="CS < 0.4 triggers clustering. Atomic: 2 clusters, others: 3.",
        required_keywords=["0.4", "cluster", "coherence"],
        required_files=["crates/whisper/src/router/coherence.rs"],
        required_symbols=["coherence_gate", "coherence_score"],
    ),
    GroundingQuestion(
        id="g14_visibility_boost",
        question_type="cross_crate",
        difficulty="medium",
        question="How does the visibility boost affect search ranking? What are the multipliers?",
        ground_truth_answer="public=1.0, crate=0.7, super=0.5, private=0.3, unknown=0.6 (weight W_VISIBILITY=0.05)",
        required_keywords=["visibility", "public", "0.05"],
        required_files=["crates/search/src/indexer.rs"],
        required_symbols=["visibility_to_str", "score_results"],
    ),
    GroundingQuestion(
        id="g15_momentum_tiers",
        question_type="structural",
        difficulty="medium",
        question="What model tiers does the MomentumEngine support? What determines the tier?",
        ground_truth_answer="Atomic (<1B), Molecular (1-4B), Galactic (4-14B), Universal (>14B/Cloud)",
        required_keywords=["Atomic", "Molecular", "Galactic", "Universal"],
        required_symbols=["ModelTier", "MomentumEngine"],
    ),
]


def get_questions(
    difficulty: str | None = None,
    question_type: str | None = None,
) -> list[GroundingQuestion]:
    """Get questions, optionally filtered."""
    qs = QUESTIONS
    if difficulty:
        qs = [q for q in qs if q.difficulty == difficulty]
    if question_type:
        qs = [q for q in qs if q.question_type == question_type]
    return qs
