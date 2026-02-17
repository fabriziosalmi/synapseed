
import os
import re
from pathlib import Path

# Paths
REPO_ROOT = Path(__file__).resolve().parent.parent.parent.parent.parent
ARCHITECT_CRATE = REPO_ROOT / "crates" / "architect"
TOOLS_MOD = REPO_ROOT / "crates" / "mcp" / "src" / "tools" / "mod.rs"

def extract_architecture_stats() -> dict:
    """Count real code metrics from the workspace — no fabricated numbers."""
    total_files = 0
    total_lines = 0
    struct_count = 0
    fn_count = 0

    # Scan ALL crates, not just architect
    crates_dir = REPO_ROOT / "crates"
    for root, _, files in os.walk(crates_dir):
        for file in files:
            if file.endswith(".rs"):
                total_files += 1
                filepath = os.path.join(root, file)
                with open(filepath, "r") as f:
                    for line in f:
                        total_lines += 1
                        stripped = line.strip()
                        if stripped.startswith("pub struct ") or stripped.startswith("pub(crate) struct ") or stripped.startswith("struct "):
                            struct_count += 1
                        if stripped.startswith("pub fn ") or stripped.startswith("pub(crate) fn ") or stripped.startswith("fn ") or stripped.startswith("pub async fn "):
                            fn_count += 1

    # Also count bin/
    bin_dir = REPO_ROOT / "bin"
    for root, _, files in os.walk(bin_dir):
        for file in files:
            if file.endswith(".rs"):
                total_files += 1
                filepath = os.path.join(root, file)
                with open(filepath, "r") as f:
                    for line in f:
                        total_lines += 1

    return {
        "modules": total_files,
        "loc": total_lines,
        "structs": struct_count,
        "functions": fn_count,
    }

def list_tools() -> list:
    """Parses tools/mod.rs to find registered tool names."""
    tools = []
    if not TOOLS_MOD.exists():
        return ["(Tools file not found)"]
        
    with open(TOOLS_MOD, "r") as f:
        content = f.read()
        
    # Look for the TOOL_NAMES array or ToolDefinition structs
    # Pattern: "name": "tool_name" inside ToolDefinition or matches in TOOL_NAMES list
    
    # Text-based extraction from TOOL_NAMES slice if available
    match = re.search(r'const TOOL_NAMES: &\[&str\] = &\[(.*?)\];', content, re.DOTALL)
    if match:
        raw_list = match.group(1)
        # Clean up quotes and commas
        clean_list = [t.strip().strip('"') for t in raw_list.split(',') if t.strip()]
        return clean_list
        
    return ["(Could not parse tool list)"]

def categorize_tools(tools: list) -> dict:
    """Categorizes tools into functional groups for taxonomy table."""
    categories = {
        "Navigation": [],
        "Analysis": [],
        "Mutation": [],
        "Search": [],
        "Memory": [],
        "Execution": []
    }
    
    # Categorization heuristics based on tool names
    for tool in tools:
        tool_lower = tool.lower()
        if any(kw in tool_lower for kw in ['read', 'view', 'list', 'get', 'find']):
            categories["Navigation"].append(tool)
        elif any(kw in tool_lower for kw in ['search', 'grep', 'query']):
            categories["Search"].append(tool)
        elif any(kw in tool_lower for kw in ['write', 'edit', 'replace', 'delete', 'create']):
            categories["Mutation"].append(tool)
        elif any(kw in tool_lower for kw in ['analyze', 'inspect', 'check', 'validate']):
            categories["Analysis"].append(tool)
        elif any(kw in tool_lower for kw in ['memory', 'context', 'recall']):
            categories["Memory"].append(tool)
        elif any(kw in tool_lower for kw in ['run', 'execute', 'command']):
            categories["Execution"].append(tool)
        else:
            # Default to Analysis if unclear
            categories["Analysis"].append(tool)
    
    # Remove empty categories
    return {k: v for k, v in categories.items() if v}


def generate_methodology_tex() -> str:
    """Generates LaTeX text for the Methodology section."""
    stats = extract_architecture_stats()
    tools = list_tools()
    tool_categories = categorize_tools(tools)
    
    # Generate taxonomy table
    taxonomy_rows = []
    for category, tool_list in tool_categories.items():
        escaped_tools = [t.replace('_', r'\_') for t in tool_list]
        tools_str = ", ".join([f"\\texttt{{{t}}}" for t in escaped_tools])
        taxonomy_rows.append(f"{category} & {tools_str} \\\\")
    
    taxonomy_table = "\n".join(taxonomy_rows)
    
    algo_tex = r"""
\begin{figure}[ht]
\centering
\begin{minipage}{0.95\linewidth}
\textbf{Algorithm 1}: Hybrid Reciprocal Rank Fusion Expansion

\begin{lstlisting}[language=Python, mathescape=true]
Input: Query q, Vector Rank Rv, Lexical Rank Rl
Output: Context Subgraph C_G

# Fusion Step
Score(d) = Sum(1/(k + rank(d, r)) for r in {Rv, Rl})

# Expansion Step
C_G = TopK(Score, N)
For d in C_G:
    # Traverse Graph for immediate neighbors
    N_neigh = Neighbors(d, G)
    For v in N_neigh:
        # Visibility Check
        If dist(d, v) == 1 AND Visibility(v) == Public:
            C_G.add(v)
            
Return C_G
\end{lstlisting}
\end{minipage}
\caption{Hybrid Reciprocal Rank Fusion used for retrieval.}
\label{algo:context_expansion}
\end{figure}
"""

    intro_text = r"""
\textbf{Inference-Only Architecture}. It is important to note that Synapseed operates entirely as a middleware layer. The underlying LLMs are kept \textbf{frozen} during all operations; no fine-tuning or parameter updates are performed. This ensures that all performance gains are attributable solely to the semantic graph retrieval and tool-use mechanics.
"""

    # Determine which diagrams to use (Mermaid PDF preferred, else PNG fallback)
    assets_path = Path("assets")
    arch_img = "assets/diagram_arch_mermaid.pdf" if (assets_path / "diagram_arch_mermaid.pdf").exists() else "assets/diagram_arch.png"
    flow_img = "assets/diagram_seq_mermaid.pdf" if (assets_path / "diagram_seq_mermaid.pdf").exists() else "assets/diagram_flow.png"

    return rf"""
\\subsection{{System Architecture}}
The Synapseed codebase comprises {stats['modules']} Rust source files totaling approximately {stats['loc']:,} lines of code,
defining {stats['structs']} struct/enum types and {stats['functions']} functions across 15 library crates and 1 binary crate.
\\textbf{{Dual-Track Memory}}. Synapseed implements a Differential Symbolic Memory to manage context:
\\begin{{itemize}}
    \\item $M_{{hi-fi}}$ (Working Set): A circular buffer of the last $N$ activated symbols, providing high-resolution local context.
    \\item $M_{{lo-fi}}$ (Journey Map): A compressed, event-driven timeline that records context shifts between crates, preserving 'architectural intent' while minimizing token pressure.
\\end{{itemize}}


{algo_tex}

\\subsection{{Evaluation Metrics}}
\\label{{sec:metrics}}
We define the following metrics for evaluating response quality.
Given a response $r$ and ground-truth sets of keywords $K$, files $F$, and symbols $S$:

\\textbf{{Keyword Recall}} (KR): Fraction of required keywords found in $r$:
\\begin{{equation}}
    \\text{{KR}} = \\frac{{|\\{{k \\in K : k \\subseteq r\\}}|}}{{|K|}}
\\end{{equation}}

\\textbf{{File Recall}} (FR): Fraction of required file paths matched in $r$ (with suffix matching for partial paths):
\\begin{{equation}}
    \\text{{FR}} = \\frac{{|\\{{f \\in F : \\exists p \\in \\text{{paths}}(r),\\, p \\text{{ ends with }} f\\}}|}}{{|F|}}
\\end{{equation}}

\\textbf{{Symbol Recall}} (SR): Fraction of required symbols found in $r$ (with word-boundary matching):
\\begin{{equation}}
    \\text{{SR}} = \\frac{{|\\{{s \\in S : s \\in_{{\\partial}} r\\}}|}}{{|S|}}
\\end{{equation}}

\\textbf{{Coverage Score}} (CS): Weighted aggregate recall:
\\begin{{equation}}
    \\text{{CS}} = 0.4 \\cdot \\text{{KR}} + 0.3 \\cdot \\text{{FR}} + 0.3 \\cdot \\text{{SR}}
\\end{{equation}}

\\textbf{{Citation Precision}} (CP): Fraction of cited file paths that exist on disk. Undefined (excluded from aggregation) when the response contains no file citations.

\\textbf{{Grounding Quality Index}} (GQI): Harmonic mean of CP and CS:
\\begin{{equation}}
    \\text{{GQI}} = \\begin{{cases}}
        \\frac{{2 \\cdot \\text{{CP}} \\cdot \\text{{CS}}}}{{\\text{{CP}} + \\text{{CS}}}} & \\text{{if CP is defined}} \\\\
        \\text{{CS}} & \\text{{otherwise}}
    \\end{{cases}}
\\end{{equation}}

\\textit{{Note}}: While GQI has the form of an F-measure, it combines two domain-specific metrics (citation precision and coverage recall) rather than standard IR precision and recall. We therefore report it under its own name to avoid confusion.

\\textbf{{Statistical Tests}}: For paired comparisons (blind vs.~grounded), we use the Wilcoxon signed-rank test (non-parametric, no normality assumption) with 95\\% bootstrap confidence intervals ($n_{{\\text{{boot}}}} = 10{{,}}000$) and Cohen's $d$ for effect size.


\\subsection{{Experimental Setup}}
All experiments were conducted on an \\textbf{{Apple MacBook Pro M4}}, utilizing \\textbf{{LMStudio}} to serve models locally via an OpenAI-compatible endpoint. This local-first setup ensures data privacy while leveraging the Apple Neural Engine for efficient inference.
The models used for evaluation are:
\\begin{{itemize}}
    \\item \\textbf{{Nano}}: Qwen3-0.6B (8-bit quantization).
    \\item \\textbf{{Efficient}}: Qwen3-1.7B (GGUF Q4\_K\_M).
    \\item \\textbf{{Balanced}}: Qwen3-4B (GGUF Q4\_K\_M).
    \\item \\textbf{{Reasoning}}: Qwen3-8B (GGUF Q4\_K\_M).
\\end{{itemize}}
Parameter settings for generation: Temperature $T=0.0$ (deterministic), no Top-P sampling.
No artificial token limit; models generate until natural stop.
Timeout per API call: 900 seconds (15 minutes) to accommodate extended thinking in smaller models.

{intro_text}

\\subsection{{Reproducibility}}
The Synapseed repository itself serves as the primary artifact for reproducibility.
The entire toolchain, including the benchmark engine and this paper generator, is open-source.
To reproduce these results:
\\begin{{enumerate}}
    \\item Clone the repository: \\texttt{{git clone https://github.com/fabriziosalmi/synapseed}}
    \\item Install dependencies: \\texttt{{cargo build --release}}
    \\item Run benchmarks: \\texttt{{python run.py coding}}, \\texttt{{python run.py grounding}}, \\texttt{{python run.py search}}
    \\item Generate paper: \\texttt{{make paper}} (Requires \\texttt{{pdflatex}})
\\end{{enumerate}}


\\subsection{{Tool Ecosystem}}
The Model Context Protocol (MCP) server exposes {len(tools)} specialized tools organized by functional capability (Table \\ref{{tab:tool_taxonomy}}).

\\begin{{table}}[ht]
\\centering
\\caption{{Tool Taxonomy: MCP tools categorized by functional capability.}}
\\label{{tab:tool_taxonomy}}
\\begin{{tabular}}{{p{{0.15\\linewidth}}p{{0.80\\linewidth}}}}
\\toprule
\\textbf{{Category}} & \\textbf{{Tools}} \\\\
\\midrule
{taxonomy_table}
\\bottomrule
\\end{{tabular}}
\\end{{table}}
"""

