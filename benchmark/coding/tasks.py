"""Task definitions for the coding benchmark.

Each task has:
- A question to ask the LLM
- Difficulty level (easy/medium/hard)
- Ground truth for automated scoring
- An optional multi-turn follow-up

Ground truth is validated against the target repo on disk,
so tasks are tied to specific target repositories.
"""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class GroundTruth:
    """What a correct answer must contain."""

    keywords: list[str] = field(default_factory=list)
    files: list[str] = field(default_factory=list)
    symbols: list[str] = field(default_factory=list)
    forbidden: list[str] = field(default_factory=list)
    code_pattern: str | None = None
    concepts: list[str] = field(default_factory=list)


@dataclass
class CodingTask:
    """A single coding benchmark task."""

    id: str
    difficulty: str  # easy | medium | hard
    target: str  # target repo name (e.g., "actix-web", "synapseed")
    question: str
    follow_up: str | None  # multi-turn follow-up question
    ground_truth: GroundTruth


# ── Self-hosted tasks: SYNAPSEED codebase ────────────────────────────
# These test against our own codebase — always available, no external deps.

SYNAPSEED_TASKS: list[CodingTask] = [
    # Easy: factual retrieval
    CodingTask(
        id="easy_version",
        difficulty="easy",
        target="synapseed",
        question="What is the current workspace version in Cargo.toml?",
        follow_up="Which file defines the workspace members?",
        ground_truth=GroundTruth(
            keywords=["version"],
            files=["Cargo.toml"],
            symbols=[],
            concepts=["workspace version"],
        ),
    ),
    CodingTask(
        id="easy_tool_count",
        difficulty="easy",
        target="synapseed",
        question="How many MCP tools does SYNAPSEED expose? List some of them.",
        follow_up="Which tool is the primary entry point for natural-language queries?",
        ground_truth=GroundTruth(
            keywords=["ask", "lookup", "hoist", "scan"],
            files=[],
            symbols=["TOOL_NAMES"],
            concepts=["MCP tools", "tool list"],
        ),
    ),
    CodingTask(
        id="easy_build_system",
        difficulty="easy",
        target="synapseed",
        question="What build system does this project use? How many crates are in the workspace?",
        follow_up=None,
        ground_truth=GroundTruth(
            keywords=["cargo", "workspace", "crates"],
            files=["Cargo.toml"],
            symbols=[],
            concepts=["Rust workspace", "monorepo"],
        ),
    ),
    # Medium: structural understanding
    CodingTask(
        id="med_dlp_finding",
        difficulty="medium",
        target="synapseed",
        question="Where is the DLP Finding struct defined? What fields does it have?",
        follow_up="How does the DLP scanner use this struct?",
        ground_truth=GroundTruth(
            keywords=["Finding", "DLP", "scanner"],
            files=["crates/husk/src/scanner.rs"],
            symbols=["Finding"],
            concepts=["data loss prevention", "security scanning"],
        ),
    ),
    CodingTask(
        id="med_intent_router",
        difficulty="medium",
        target="synapseed",
        question="What are the intent categories in the whisper router? How is intent classified?",
        follow_up="When does the intent get hardened from General to Explain?",
        ground_truth=GroundTruth(
            keywords=["BugFix", "Security", "Explain", "Refactor", "General"],
            files=["crates/whisper/src/router/mod.rs"],
            symbols=["Intent", "classify_intent"],
            concepts=["intent classification", "keyword heuristics"],
        ),
    ),
    CodingTask(
        id="med_search_ranking",
        difficulty="medium",
        target="synapseed",
        question=(
            "How does the search ranking work? "
            "What boost factors are applied to BM25 scores?"
        ),
        follow_up="What is the visibility boost and why was it added?",
        ground_truth=GroundTruth(
            keywords=["BM25", "temporal", "source", "path", "visibility", "pagerank"],
            files=["crates/search/src/indexer.rs"],
            symbols=["search", "SearchResult"],
            concepts=["ranking", "boost stack", "multiplicative factors"],
        ),
    ),
    # Hard: cross-crate / implementation detail
    CodingTask(
        id="hard_coherence_gate",
        difficulty="hard",
        target="synapseed",
        question=(
            "Explain the Coherence Gate in the whisper router. "
            "What is the coherence score formula? When does the gate trigger?"
        ),
        follow_up="How does model tier affect the number of clusters kept?",
        ground_truth=GroundTruth(
            keywords=["coherence", "threshold", "0.4", "cluster", "module_prefix"],
            files=["crates/whisper/src/router/coherence.rs"],
            symbols=["coherence_gate", "coherence_score"],
            concepts=["scattered targets", "module clustering", "model tier"],
        ),
    ),
    CodingTask(
        id="hard_plugin_system",
        difficulty="hard",
        target="synapseed",
        question=(
            "How does the plugin system work? "
            "What trait must plugins implement and what lifecycle events exist?"
        ),
        follow_up="How are events dispatched to plugins during SystemInit?",
        ground_truth=GroundTruth(
            keywords=["SynapsePlugin", "SynapseEvent", "SystemInit", "SystemShutdown"],
            files=["crates/core/src/plugin.rs", "crates/core/src/event.rs"],
            symbols=["SynapsePlugin", "SynapseEvent"],
            concepts=["plugin trait", "event-driven", "lifecycle"],
        ),
    ),
    CodingTask(
        id="hard_gym_sandbox",
        difficulty="hard",
        target="synapseed",
        question=(
            "How does the Gym sandbox isolate code evaluation? "
            "What prevents network access? How does mutation testing work?"
        ),
        follow_up="What happens if a mutated build fails to compile?",
        ground_truth=GroundTruth(
            keywords=["sandbox", "offline", "mutation", "Cargo.toml", "proptest"],
            files=["crates/gym/src/sandbox.rs"],
            symbols=["evaluate", "run_mutations"],
            concepts=["network isolation", "mutation testing", "adversarial"],
        ),
    ),
]


# ── External target tasks ────────────────────────────────────────────
# These require cloned repos in benchmark/targets/

ACTIX_WEB_TASKS: list[CodingTask] = [
    CodingTask(
        id="actix_easy_router",
        difficulty="easy",
        target="actix-web",
        question="Where is the main Router struct defined in actix-web?",
        follow_up=None,
        ground_truth=GroundTruth(
            keywords=["Router", "route", "recognize"],
            files=["actix-router/src/router.rs"],
            symbols=["Router", "recognize"],
        ),
    ),
    CodingTask(
        id="actix_med_middleware",
        difficulty="medium",
        target="actix-web",
        question="What trait defines middleware in actix-web? Where is it defined?",
        follow_up="How does a middleware wrap around the next service?",
        ground_truth=GroundTruth(
            keywords=["Transform", "Service", "middleware"],
            files=["actix-web/src/middleware/"],
            symbols=["Transform"],
            concepts=["middleware pipeline", "service wrapping"],
        ),
    ),
    CodingTask(
        id="actix_hard_extractor",
        difficulty="hard",
        target="actix-web",
        question=(
            "How does the extractor system work in actix-web? "
            "How does FromRequest enable type-safe parameter extraction?"
        ),
        follow_up="What happens when extraction fails?",
        ground_truth=GroundTruth(
            keywords=["FromRequest", "extract", "ServiceRequest", "Error"],
            files=["actix-web/src/extract.rs"],
            symbols=["FromRequest", "from_request"],
            concepts=["type-safe extraction", "async trait", "error handling"],
        ),
    ),
]


def get_tasks(target: str | None = None) -> list[CodingTask]:
    """Get tasks, optionally filtered by target."""
    all_tasks = SYNAPSEED_TASKS + ACTIX_WEB_TASKS
    if target:
        return [t for t in all_tasks if t.target == target]
    return all_tasks


def get_self_tasks() -> list[CodingTask]:
    """Get only self-hosted tasks (no external deps needed)."""
    return SYNAPSEED_TASKS
