
def generate_formalization_tex() -> str:
    """Generates LaTeX for the Formalization section."""
    return r"""
\section{Formalization}
\label{sec:formalization}

To rigorously define the operation of Synapseed, we model the system as a directed semantic graph and a stateful execution environment.

\subsection{Semantic Graph Model}
Let $\mathcal{G} = (V, E)$ be the semantic dependency graph where $V$ is the set of code modules and $E \subseteq V \times V$ represents the dependency relation. Each node $v \in V$ is characterized by a tuple of properties:
\begin{equation}
    v = (\text{id}, \mathcal{L}, \sigma, \phi)
\end{equation}
where $\text{id}$ is the unique module identifier, $\mathcal{L}$ is the programming language, $\sigma$ is the set of public symbols, and $\phi$ represents the code complexity (e.g., LOC or AST depth).

An edge $e_{ij} \in E$ exists if module $v_i$ imports symbols from $v_j$. The weight of the edge $w_{ij}$ corresponds to the coupling strength:
\begin{equation}
    w_{ij} = \sum_{s \in \sigma_j} \mathbb{I}(v_i \text{ imports } s)
\end{equation}

\subsection{Architectural Metrics}
We define the \textit{Instability} $I(v)$ of a module $v$ as the ratio of efferent coupling ($C_e$) to total coupling:
\begin{equation}
    I(v) = \frac{C_e(v)}{C_e(v) + C_a(v)}
\end{equation}
where $C_e(v) = |\delta_{\text{out}}(v)|$ and $C_a(v) = |\delta_{\text{in}}(v)|$. A value of $I(v) \approx 0$ indicates a stable, foundational component, while $I(v) \approx 1$ indicates a volatile, high-level coordinator.

\subsection{Tool Execution Dynamics}
The Model Context Protocol (MCP) server acts as a state machine $M = (S, T, \delta)$.
Let $s_t \in S$ be the context state at time $t$, including the active file graph and conversation history.
Let $\mathcal{T} = \{t_1, t_2, \ldots, t_n\}$ be the set of available tools (e.g., \texttt{search}, \texttt{analyze}).

A tool execution is a function $f: \mathcal{T} \times S \to S'$ that transitions the system to a new state:
\begin{equation}
    s_{t+1} = f(\text{tool}, \text{args} \mid s_t)
\end{equation}
The effectiveness of a tool $t$ is measured by the Grounding Quality Index (GQI) described in Section \ref{sec:results}.

\subsection{Strategic Calibration}
We define the \textbf{Operational Pulse} $P$ as a 4-tuple that modulates the transition function $\delta$:
\begin{equation}
    P = (\alpha, \beta, \gamma, \tau)
\end{equation}
where:
\begin{itemize}
    \item $\alpha \in [0,1]$ represents the \textbf{Efficiency Gradient}, biasing retrieval towards low-latency lexical matches (BM25) when $\alpha \to 1$ or expensive vector search when $\alpha \to 0$.
    \item $\beta \in [0,1]$ represents \textbf{Entropy Tolerance}, governing the strictness of JSON output validation. Lower $\beta$ implies stricter schema enforcement.
    \item $\gamma \in [0,1]$ represents the \textbf{Graph Depth Factor}, controlling dependency expansion depth. At $\gamma = 1.0$, the transitive closure of dependencies is fully traversed.
    \item $\tau > 0$ defines the \textbf{Latency Ceiling} in seconds, establishing a hard timeout for sub-tool orchestration.
\end{itemize}
This formalism allows Synapseed to dynamically adapt its search strategy based on real-time constraints.
"""
