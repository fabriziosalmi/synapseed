
def generate_case_study() -> str:
    """Generates a qualitative case study section.

    Uses a representative example of multi-step resolution to illustrate
    Synapseed's reasoning flow. The trace is schematic (not a verbatim log)
    and is documented as such.
    """

    # Read actual version from Cargo.toml if possible
    import os
    version = "N/A"
    cargo_path = os.path.join(
        os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(__file__))))),
        "Cargo.toml"
    )
    if os.path.exists(cargo_path):
        with open(cargo_path, "r") as f:
            for line in f:
                if line.strip().startswith("version"):
                    version = line.split("=")[1].strip().strip('"')
                    break

    trace_tex = r"""
\begin{figure*}[ht]
\centering
\begin{small}
\begin{tabular}{p{0.95\textwidth}}
\toprule
\textbf{User Query}: ``What is the workspace version in Cargo.toml?'' \\
\midrule
\textbf{Schematic Reasoning Trace}\footnotemark: \\
\texttt{[Step 1]} Parse query intent: user seeks the workspace-level version string. \\
\texttt{[Step 2]} \textit{search(query=``workspace version'', file\_pattern=``Cargo.toml'')} \\
\quad Returns: \texttt{[workspace.package]} section in root \texttt{Cargo.toml}. \\
\texttt{[Step 3]} Extract version field from TOML structure. \\
\texttt{[Step 4]} Cross-reference with \texttt{reporting.py} which parses \texttt{Cargo.toml} dynamically. \\
\textbf{Final Answer}: The workspace version is """ + version + r""", as declared in the root \texttt{Cargo.toml}. \\
\bottomrule
\end{tabular}
\end{small}
\caption{Schematic illustration of multi-step query resolution. This trace represents the \emph{type} of reasoning Synapseed enables, not a verbatim system log.}
\label{fig:case_study}
\end{figure*}
\footnotetext{This trace is a schematic representation of the system's reasoning flow, simplified for clarity. Actual tool invocations involve additional MCP protocol overhead and context management.}
"""
    return r"""
\section{Qualitative Analysis}
To illustrate the system's reasoning capabilities, we present a schematic trace where Synapseed resolves a query that requires cross-referencing multiple files.

%s

Figure \ref{fig:case_study} demonstrates the multi-step resolution pattern.

\textbf{Why Standard RAG Struggles}: Standard RAG systems treat \texttt{Cargo.toml} as a text file. Since the version string may be in a different TOML section than naively expected, vector similarity matches can be weak. Synapseed's semantic graph identifies structural relationships between configuration files and the code that consumes them, enabling more reliable resolution.
""" % trace_tex
