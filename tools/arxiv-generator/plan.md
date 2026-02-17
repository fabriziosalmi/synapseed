# Future Metrics Plan

The following metrics are required for a complete arXiv paper but are currently missing from the benchmark data or implementation.

## 1. Tool-Specific Metrics
**Goal**: Analyze the usage and impact of individual tools (e.g., `check`, `search`, `run_benchmark`) within the grounding process.
- [ ] **Data Requirement**: The `grounding_*.json` log must include tool call traces or statistics per query.
- [ ] **Metric**: Frequency of use, Success rate per tool, Latency contribution per tool.
- [ ] **Implementation**: Update `performance.py` or new `tools_analysis.py` to parse tool usage if available.

## 2. Cross-Version Improvements
**Goal**: Show progress of Synapseed over time (e.g., v4.10 vs v4.14).
- [ ] **Data Requirement**: Maintain historical benchmark files in `data/` or a dedicated history folder.
- [ ] **Metric**: Delta in MRR, F1, and Latency between versions.
- [ ] **Implementation**: `results.py` should accept multiple files or scan for previous versions to generate a trend graph/table.

## 3. Multi-Model Comparison
**Goal**: Compare performance across different models (e.g., Qwen 1.7B vs 4B vs 8B, or Llama-3).
- [ ] **Data Requirement**: Run `grounding` benchmark with different `--model` arguments.
- [ ] **Metric**: F1 vs Model Size, Latency vs Model Size matrix.
- [ ] **Implementation**: New generator `models_comparison.py` to aggregate multiple `grounding_*.json` files keyed by model name.

## 4. Hallucination Analysis by Category
**Goal**: Break down hallucinations (e.g., factual error vs fabrication).
- [ ] **Data Requirement**: Richer annotation in `grounding_*.json`.
