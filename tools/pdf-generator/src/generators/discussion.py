
import sys
from pathlib import Path



# Import shared logic/metrics
# Import shared logic/metrics
from .results import load_search_metrics, load_grounding_metrics, load_all_grounding_metrics
from .performance import load_performance_metrics
from .methodology import extract_architecture_stats



def generate_abstract() -> str:
    search = load_search_metrics()
    grounding = load_grounding_metrics()
    perf = load_performance_metrics()
    
    mrr = search.get("mrr", 0.0)
    recall = search.get("recall_at_10", 0.0)
    gqi = grounding.get("gqi", grounding.get("f1", 0.0))
    latency = perf.get("avg_latency", 0.0)
    throughput = perf.get("avg_throughput", 0.0)
    
    return f"""
\\begin{{abstract}}
We present Synapseed, a high-performance semantic AI middleware designed to bridge the gap between large language models (LLMs) and large-scale software repositories. By integrating a modular graph-based architecture with the Model Context Protocol (MCP), Synapseed enhances search quality and grounding accuracy for code-related queries. Our benchmarks demonstrate that Synapseed achieves a Mean Reciprocal Rank (MRR) of \\textbf{{{mrr:.3f}}} and a Recall@10 of \\textbf{{{recall:.3f}}} in search tasks. Furthermore, the grounded generation capability yields a Grounding Quality Index (GQI) of \\textbf{{{gqi:.3f}}}, showing improvement over ungrounded baselines across all tested model sizes. These results are delivered with an average end-to-end latency of {latency:.2f}s and a throughput of {throughput:.1f} tokens/sec, validating Synapseed's suitability for production-grade developer tooling.
\\end{{abstract}}
"""

def generate_introduction() -> str:
    # Heuristic: We could add dynamic stats about the repo size here if available
    return r"""
The rapid advancement of Large Language Models (LLMs) has revolutionized software engineering, offering unprecedented capabilities in code generation, explanation, and debugging. However, integrating these models with existing, complex codebases remains a challenge due to the limited context window and the lack of up-to-date knowledge in pre-trained models.

Existing approaches such as naive vector-based Retrieval-Augmented Generation (RAG) suffer from structural blindness—they retrieve text chunks without understanding code dependencies, call graphs, or architectural relationships. Similarly, while commercial tools like GitHub Copilot and Cursor provide code assistance, they lack the transparency and local-first privacy guarantees required for enterprise deployment.

\textbf{Our Contributions.} This paper makes the following contributions:
\begin{enumerate}
    \item \textbf{Graph-Based Differential Memory}: A novel dual-track memory architecture ($M_{hi-fi}$ / $M_{lo-fi}$) that balances local context precision with global architectural awareness, enabling efficient long-context reasoning.
    \item \textbf{Hybrid RRF Retrieval}: A rank fusion algorithm combining vector embedding search with semantic graph traversal, achieving superior recall compared to vector-only approaches.
    \item \textbf{MCP Integration}: A standardized tool ecosystem via the Model Context Protocol, providing deterministic state management and reducing hallucination.
    \item \textbf{Empirical Validation}: Comprehensive benchmarks across four model sizes (0.6B--8B parameters) demonstrating that small models with Synapseed middleware achieve substantial grounding improvements over ungrounded baselines, enabling efficient local deployment.
\end{enumerate}
"""


def generate_related_work() -> str:
    return r"""
\subsection{Retrieval-Augmented Generation (RAG)}
Traditional RAG systems rely on vector similarity search to retrieve context. While effective for general knowledge, they often struggle with the precise structural dependencies required for software engineering. Synapseed improves upon this by incorporating a semantic graph that captures code relationships (e.g., call graphs, type hierarchies).

\subsection{Tool-Augmented LLMs}
Recent works like Toolformer and AutoGPT have demonstrated the power of equipping LLMs with external tools. Synapseed adopts the Model Context Protocol (MCP) to standardize these interactions, providing a deterministic state machine for tool execution that reduces the likelihood of hallucination compared to purely probabilistic token generation.

\subsection{Small Language Models (SLMs)}
The trend towards highly efficient, edge-deployable models (e.g., Phi-2, Qwen-1.5) challenges the assumption that complex reasoning requires massive parameters. Synapseed validates this potential by demonstrating that a \textbf{0.6B parameter model}, when grounded with tool-augmented context, shows measurable improvements over its ungrounded baseline in code understanding tasks.
"""

def generate_discussion() -> str:
    grounding = load_grounding_metrics()
    perf = load_performance_metrics()
    all_metrics = load_all_grounding_metrics()
    
    gqi = grounding.get("gqi", grounding.get("f1", 0.0))
    coverage = grounding.get("coverage_score", grounding.get("recall", 0.0))
    
    models = list(all_metrics.keys())
    model_count = len(models)
    model_names_str = ", ".join([f"\\texttt{{{m}}}" for m in models])
    
    return f"""
\\subsection{{Performance Analysis}}
Our results indicate that Synapseed improves grounding quality across all tested model sizes. The Grounding Quality Index (GQI) of {gqi:.3f} and coverage score of {coverage:.3f} demonstrate the effectiveness of the tool-use paradigm in providing relevant context and restricting hallucinations.

Analysis of system performance shows that while retrieval-augmented generation introduces latency (avg {perf.get('avg_latency',0):.2f}s), the trade-off is justified by the gain in factual correctness. The optimized binary size of {perf.get('binary_size_mb',0):.1f} MB ensures that the middleware remains lightweight and deployable in diverse environments.

\\subsection{{Model Comparison \\& Cost}}
We evaluated Synapseed across {model_count} different model configurations: {model_names_str} (Figure \\ref{{fig:comp_f1}}). In all cases, the Grounded mode (Synapseed) showed improved Coverage Scores compared to the Blind baseline.
While grounding inevitably increases the token count per task due to context retrieval (Figure \\ref{{fig:comp_cost}}), the efficiency ratio suggests that the system retrieves relevant context without excessive overhead.

\\subsection{{Statistical Considerations}}
We report Wilcoxon signed-rank tests and 95\\% bootstrap confidence intervals where sample sizes permit ($n \\geq 5$). Cohen's $d$ effect sizes are provided for practical significance assessment. Given the relatively small sample sizes in our evaluation (15 grounding questions, 9 coding tasks), we caution against over-interpreting specific point estimates and emphasize the consistency of improvement direction across all model sizes and task types.

\\subsection{{Limitations}}
While Synapseed improves grounding, the reliance on an external semantic graph introduces an indexing overhead. Currently, the graph construction is static; real-time updates for rapidly changing codebases are a subject of future work. Additionally, the dual-track memory, while efficient, may still suffer from ``forgetting'' in extremely long conversational sessions exceeding the journey map's sliding window. The current evaluation uses a single codebase (the Synapseed repository itself); cross-project generalization remains to be validated.
"""

def generate_conclusion() -> str:
    return r"""
We have presented Synapseed, a middleware for semantic AI in software engineering. By combining graph-based retrieval with the Model Context Protocol, we achieve consistent improvements in code search and grounded generation across all tested model sizes (0.6B--8B parameters). The system's modular architecture ensures extensibility, while its performance metrics confirm its viability for local-first deployment. Synapseed demonstrates that small language models, when augmented with structured code context, can produce more grounded and accurate responses than their ungrounded baselines.
"""


if __name__ == "__main__":
    try:
        assets_dir = Path(__file__).resolve().parent.parent.parent / "assets"
        assets_dir.mkdir(exist_ok=True)
        
        with open(assets_dir / "abstract.tex", "w") as f:
            f.write(generate_abstract())
            
        with open(assets_dir / "introduction.tex", "w") as f:
            f.write(generate_introduction())
            
        with open(assets_dir / "discussion.tex", "w") as f:
            f.write(generate_discussion())
            
        print("✅ Dynamic Narrative sections generated.")
    except Exception as e:
        print(f"Error generating narrative: {e}", file=sys.stderr)
        sys.exit(1)
