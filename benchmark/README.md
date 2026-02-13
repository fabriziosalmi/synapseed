# SYNAPSEED Benchmark Suite

Structured benchmarks for measuring and iterating on SYNAPSEED's effectiveness.

## Quick Start

```bash
cd benchmark
cp .env.example .env        # Configure your local LLM
pip install -r requirements.txt

# From project root:
python -m benchmark.coding.run --quick
python -m benchmark.grounding.run --quick
python -m benchmark.search.run
python -m benchmark.niah.run --quick
```

## Benchmark Types

| Benchmark | What it measures | Self-hosted | LLM required |
|-----------|-----------------|-------------|--------------|
| **coding** | BLIND vs SYNAPSEED on code understanding tasks | Yes (synapseed target) | Yes |
| **grounding** | MCP tool effectiveness: precision, recall, F1 | Yes | Yes |
| **search** | Tantivy ranking quality: MRR, P@K, R@K | Yes | No |
| **niah** | Needle-in-a-Haystack: context sensitivity | Yes | Yes |

## Dashboard (local wandb)

```bash
cd benchmark
source venv/bin/activate
streamlit run dashboard/app.py
```

Opens a local web UI at `http://localhost:8501` with:
- Historical trends per benchmark type
- BLIND vs SYNAPSEED comparison charts
- Per-difficulty bar charts
- Search quality MRR/P@K per query
- NIAH depth x context heatmaps
- Auto-refreshes every 10 seconds

## Directory Structure

```
benchmark/
  shared/           Shared utilities (LLM client, scoring, reporting)
  coding/           BLIND vs SYNAPSEED code understanding
  grounding/        MCP grounding F1 evaluation (15 stratified questions)
  search/           Tantivy search ranking quality (12 queries)
  niah/             Needle-in-a-Haystack context sensitivity
  dashboard/        Streamlit web UI for historical metrics
  results/          JSON output (git-tracked for iteration)
  targets/          External target repos (git-ignored, cloned on demand)
  venv/             Python virtual environment (git-ignored)
  .env.example      LLM configuration template
  requirements.txt  Python dependencies
```

## Workflow

1. **Run** a benchmark: `python -m benchmark.coding.run`
2. **Review** results in `benchmark/results/` (Rich console + JSON)
3. **Iterate** on SYNAPSEED code (e.g., improve search ranking)
4. **Re-run** the same benchmark
5. **Compare** JSON results across runs via git diff
6. **Track** improvement trends over versions

Results JSON files are timestamped and include the SYNAPSEED version,
making it easy to correlate improvements with code changes.

## Adding Custom Tasks

### Coding benchmark
Edit `coding/tasks.py` — add a `CodingTask` with ground truth.

### Grounding benchmark
Edit `grounding/questions.py` — add a `GroundingQuestion` with expected answer.

### Search benchmark
Edit `search/run.py` — add a `SearchQuery` with known-relevant symbols.

## Configuration

All LLM settings are in `.env` (copied from `.env.example`).
Supports Ollama, LM Studio, OpenRouter, and any OpenAI-compatible endpoint.
