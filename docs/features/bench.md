# Benchmark Engine

The Benchmark Engine provides a **reproducible evaluation framework** for SYNAPSEED's semantic understanding. It measures how effectively the system can answer questions about code, using standard information retrieval and generation metrics.

## Methodology

Benchmarks run against **Question Suites** (JSONL files). Each suite contains a set of questions with "ground truth" answers (facts requiring retrieval).

The engine calculates:
- **F1 Score**: Harmonic mean of precision and recall on key facts.
- **SCR (Semantic Compression Ratio)**: Efficiency metric — knowledge density per token.
- **SID (Semantic Information Density)**: Information density of the response.
- **Hallucination Rate**: Frequency of citing non-existent files or symbols.

## Running Benchmarks

### MCP Tool: `run_benchmark`
Execute a benchmark suite.
```bash
synapseed run_benchmark --suite_path suites/grounding_v1.jsonl
```

### Creating Suites

A suite file is a JSONL (JSON Lines) file where each line is a test case:

```json
{
  "id": "q1",
  "question": "How does authentication work?",
  "difficulty": "Medium",
  "ground_truth": [
    "Uses JWT tokens",
    "Validated in AuthService middleware",
    "Tokens expire after 1 hour"
  ]
}
```

The system will answer the question using the `ask` tool and compare the generated response against the `ground_truth` facts.
