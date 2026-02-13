# Synapseed: Technical Q&A

**Synapseed** is an open-source middleware developed in Rust, designed to act as a logical layer between users and Large Language Models (LLMs) during programming.

---

### What is Synapseed and how does it differ from a standard LLM?

Unlike traditional AI systems that treat code as plain text, Synapseed performs semantic code analysis using AST (Abstract Syntax Tree) parsing and local indexing to prevent AI hallucinations.  It integrates advanced security features, runs entirely locally to guarantee maximum speed (zero outbound network calls for analysis), and uses the MCP protocol to interface easily with environments like VS Code, Cursor, and Claude.

### How does Synapseed's plugin architecture improve development?

The architecture transforms AI interaction from basic text manipulation into a semantic, secure, and structured process. It uses specialized Rust plugins (crates) that act as a "thinking layer":

* **Deep Code Understanding (Cortex and Search):** Generates an AST and symbol graph for deep structural understanding.
* **"Fail-Closed" Security (Husk and Root):** Operates locally to ensure no sensitive data leaves the developer's machine.
* **Quality & Maintenance (Shadow, Janitor, and Architect):** Runs background compilation checks, identifies unused dependencies, and monitors structural health.
* **Historical Context (Chronos and Whisper):** Tracks Git history with semantic tags and acts as an "Intent Router" to orchestrate responses.
* **High Performance (SynapseContext):** All plugins communicate via a local Event Bus compiled in a single Rust binary, keeping latency under 10 ms.

### What are the advantages of using Tantivy for search?

Using Tantivy within the Search plugin offers significant upgrades over standard tools like `grep` or regex:

* **Semantic and Conceptual Search:** Combines Full-Text Search (FTS) with vector embeddings (via `fastembed` and cosine similarity) to find code based on meaning and intent rather than exact character matches.
* **Extreme Performance (Zero Latency):** Operates locally within the Rust binary, enabling "zero-copy" execution with sub-10 ms latency.
* **Memory Flexibility:** The index resides in RAM for instant startup but can be configured for disk persistence to handle massive codebases without re-indexing.

### How does Synapseed protect code from secret leaks?

It adopts a "defense-in-depth" and "fail-closed" approach via the **Husk (DLP Shield)** plugin.

* **Real-Time Scanning:** Uses Aho-Corasick algorithms and Regex to identify API keys, passwords, and PII before anything is sent to the LLM.
* **Protective Actions:** Automatically applies actions like `Redact` (hiding the secret), `Deny` (blocking the request), or `Audit` (logging the event).
* **Custom DNA Configuration:** Allows developers to define custom rules and whitelists in the `.synapseed/dna.yaml` file to fit company-specific patterns.

### How does the predefined whitelist reduce false positives?

Standard security scanners often flag harmless words like "token" or "key". Synapseed's whitelist prevents this by:

* **Recognizing Rust Context:** Automatically ignoring safe, common programming constructs like `CancellationToken` or `shutdown_token`.
* **Regex Filtering:** Using targeted regex to identify the specific syntactic structure of harmless terms, distinguishing them from actual hardcoded credentials.

### How does the Command Sentinel block dangerous commands?

The Command Sentinel (within the Root plugin) evaluates shell instructions proposed by the AI before execution:

* **Deny-First Model:** Security has absolute priority. If a command hits a deny rule, it is blocked immediately.
* **Destructive Pattern Blocking:** Pre-configured to neutralize commands like `rm -rf`, `chmod 777`, `mkfs`, or insecure remote executions.
* **Local Policy Evaluation:** Validates commands locally to prevent accidental execution caused by AI hallucinations or prompt injection.

### Does the Command Sentinel also block insecure network commands?

Yes. It specifically intercepts highly risky remote execution patterns, such as `curl | sh` (downloading and executing scripts without verification). Additionally, Synapseed enforces strict **Network Isolation**, binding servers exclusively to `127.0.0.1` and utilizing a "zero outbound calls" design for its analysis operations.

### How do you define a custom security rule in the DNA configuration?

Custom rules are defined in the `.synapseed/dna.yaml` file under the `dlp_custom_rules` key. Each rule requires three fields:

* **name:** A unique identifier (e.g., `internal_api_key`).
* **pattern:** The Regex identifying the sensitive string.
* **action:** The system response (`redact`, `deny`, `audit`, or `allow`).

### What is the advantage of the "redact" action over "deny"?

**Redact** replaces sensitive data with a neutral placeholder (like `[REDACTED]`), preserving the semantic context. This allows the LLM to understand the code logic and provide helpful suggestions without seeing the actual secret. **Deny** completely halts the request, which is necessary for destructive commands but can disrupt the coding workflow if triggered by simple variables.

### How does the Mentor Mode work based on query complexity?

Driven by the Whisper plugin (Intent Router), Mentor Mode dynamically adjusts the depth of its answers:

* **Intent Analysis:** Analyzes the semantic structure of the user's query rather than just looking at keywords.
* **Adaptive Depth:** Provides concise answers for direct queries (e.g., "Where is the User struct?") and deep, step-by-step guidance for abstract architectural questions.
* **Strategic Orchestration:** Selects the best combination of tools to explain the "why" and "how" behind a solution instead of just returning raw data.

### What are the 20 MCP tools included in Synapseed?

Synapseed exposes 20 Model Context Protocol (MCP) tools, structured across three levels.

| Category | Tools | Primary Function |
| --- | --- | --- |
| **Orchestration** | `ask` (or `whisper`) | The primary intent-based router and decision engine. |
| **Code Analysis & Navigation** | `hoist`, `lookup`, `search`, `similar` | Parses AST structure, finds specific traits, and runs semantic/vector searches. |
| **Security & Protection** | `scan`, `check` | Runs local DLP scans for secrets and evaluates shell commands against security policies. |
| **Git & History** | `blame`, `analyze`, `intent` | Provides semantic Git history, analyzes file churn, and categorizes commit intents. |
| **Quality & Maintenance** | `diagnostics`, `quickfix`, `janitor`, `janitor-fix` | Surfaces real-time compiler warnings, unused dependencies, and applies automated fixes. |
| **Architecture & Docs** | `architect`, `consult`, `oracle` | Analyzes structural health/coupling and repairs outdated documentation. |
| **Diagnostics & Sandbox** | `diagnose`, `reset-telemetry`, `train` | Manages internal system health, clears telemetry data, and evaluates Rust code in an isolated sandbox. |

### What are the advantages of the Gym sandbox for Rust?

The Gym sandbox (`synapseed-gym`) improves AI-generated code quality through empirical verification:

* **Isolated Evaluation:** Tests Rust snippets safely without risking the main development environment.
* **Real Feedback:** Actively attempts to compile code and run tests, ensuring suggestions are functionally valid, not just syntactically correct.
* **Reinforcement Learning (RL):** Acts as a training environment, allowing the AI to learn from compilation errors.
* **Adversarial Testing:** Can stress-test code against hostile scenarios to guarantee robustness before deployment.

### How does the integration between telemetry and hotspot optimization work?

Synapseed uses a "Dogfooding" autonomous feedback loop to monitor and improve its own performance:

* **Self-Telemetry:** Activated via `SYNAPSEED_SELF_TELEMETRY=1`, sending internal tracing spans to an integrated OTLP receiver.
* **Bottleneck Identification:** The data is processed and exposed via the `synapseed://telemetry/hotspots` resource, highlighting the Top-10 most resource-intensive operations.
* **Guided Optimization:** The `optimize_hotspots` prompt instructs the LLM to analyze this internal data and suggest concrete architectural or codebase optimizations to speed up the middleware.
