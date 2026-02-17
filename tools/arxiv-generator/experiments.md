# Experiments Plan

To reach SOTA-level rigor, we need to run specific experiments to isolate the contribution of each component (Ablation) and measure efficiency (Token Economics).

## 1. Ablation Study
**Goal**: Quantify the impact of the *Semantic Graph* and *Tool Use*.

### Experiment A: No-Graph Baseline
Run the `grounding` benchmark with the Graph retrieval disabled (relying only on vector search or naive text search).
- **Command**: `cargo run --release --bin benchmark -- --mode grounding --disable-graph` (Hypothetical flag, check `benchmark` CLI)
- **Metric**: Compare F1 Score with vs without Graph.

### Experiment B: No-Tools Baseline
Run with tools disabled (pure LLM generation).
- **Command**: `cargo run --release --bin benchmark -- --mode grounding --disable-tools`
- **Metric**: Measure Hallucination Rate.

## 2. Token Economics
**Goal**: Calculate the "Cost per Solved Task".

### Metrics to Harvest
- **Total Tokens / Task**: explicit in `grounding_*.json`.
- **Context Efficiency**: Ratio of *Useful* tokens to *Total* tokens.

## 3. Efficiency Hypothesis: "Tools replace Thought"
**Hypothesis**: Synapseed grounded retrieval can achieve equal/higher accuracy than "Thinking" models (e.g., DeepSeek-R1) with significantly lower token cost.

### Experiment C: Thinking vs Tooling
- **Baseline**: Run benchmark with a "Reasoning" model (e.g., `deepseek-r1` or `qwen-2.5-32b` with CoT prompt).
- **Test**: Run benchmark with a smaller "No-Think" model (e.g., `qwen-2.5-7b`) + Synapseed.
- **Metrics**:
    - Accuracy / F1
    - Total Tokens (Thinking tokens vs Tool tokens)
    - Latency

## 4. Scale Testing (Optional)
**Goal**: Prove scalability.
- **Action**: Run `architect` on a very large repo (e.g., `linux` kernel or `rust-lang/rust`) and measure graph build time.

