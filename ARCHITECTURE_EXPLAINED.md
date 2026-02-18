# Architecture Explained (In Plain English)

SYNAPSEED might look complex, but the core idea is simple. This guide explains how it works without the jargon.

## The Big Picture

```
┌─────────────────────────────────────────────────────────┐
│  You (or an AI Assistant) ask a question about code    │
└───────────────────────┬─────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────┐
│              SYNAPSEED (The Middleman)                  │
│  ┌───────────────────────────────────────────────────┐  │
│  │ 1. Understands your question                      │  │
│  │ 2. Figures out what info is needed                │  │
│  │ 3. Collects relevant data from your codebase     │  │
│  │ 4. Organizes and returns the answer              │  │
│  └───────────────────────────────────────────────────┘  │
└───────────────────────┬─────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────┐
│              Your Codebase (Files on Disk)              │
└─────────────────────────────────────────────────────────┘
```

**Key Point:** SYNAPSEED doesn't modify your code. It just reads, analyzes, and reports.

## The 5 Main Components

Think of SYNAPSEED as having 5 specialized assistants, each with a specific job:

### 1. 🧠 The Parser ("Cortex")
**What it does:** Reads your code and understands structure  
**Real-world analogy:** Like a grammar checker, but for code

**Example:**
- **Raw code:** `function login(user, pass) { ... }`
- **What Cortex sees:** "A function named 'login' that takes 2 parameters and returns..."

**Why it matters:** AI assistants can see *what* your code does, not just *what* it says.

### 2. 🔍 The Search Engine ("Search")
**What it does:** Finds code based on concepts, not just exact words  
**Real-world analogy:** Google for your codebase

**Example:**
- **You search:** "error handling"
- **It finds:** Functions named `handle_error()`, `process_exception()`, `catch_failure()`

**Why it matters:** Find related code even when it uses different terminology.

### 3. 🛡️ The Security Guard ("Husk")
**What it does:** Scans for secrets before they leak  
**Real-world analogy:** Metal detector at an airport

**Example:**
- **Input:** `API_KEY=sk_live_abc123xyz789`
- **Output:** `API_KEY=[REDACTED]` + Alert logged

**Why it matters:** Prevents accidentally sharing sensitive data with AI or committing it to git.

### 4. ⏱️ The Historian ("Chronos")
**What it does:** Analyzes git history to understand code evolution  
**Real-world analogy:** Archaeological dig through your commits

**Example:**
- **Question:** "Why does this function look weird?"
- **Answer:** "It was refactored 3 times. Last changed by Alice to fix bug #42"

**Why it matters:** Understand the *why* behind code decisions.

### 5. 🏗️ The Inspector ("Architect")
**What it does:** Evaluates code quality and organization  
**Real-world analogy:** Building inspector checking structural integrity

**Example:**
- **Analysis:** "Grade: B- (82/100)"
- **Issues:** "High coupling in auth module, 2 circular dependencies"

**Why it matters:** Objective measurements of code health.

## How They Work Together

When you ask a question, here's what happens:

```
YOU: "Where's the authentication logic?"
  │
  ▼
SYNAPSEED (decides): "I need to search + parse + check git"
  │
  ├─► Search Engine: Find files mentioning "auth"
  │   └─► Returns: [auth.rs, middleware.rs, ...]
  │
  ├─► Parser: Get function details from those files
  │   └─► Returns: [verify_user(), check_token(), ...]
  │
  └─► Historian: Who last modified these?
      └─► Returns: [Alice on Jan 15, Bob on Feb 3, ...]
  │
  ▼
SYNAPSEED (combines everything):
  "Authentication logic is in 3 places:
   - auth.rs:45 → verify_user() (last modified by Alice)
   - middleware.rs:12 → AuthMiddleware (last modified by Bob)
   - ..."
```

All of this happens in **milliseconds** on your local machine.

## The Technical Details (Optional)

If you're curious about the implementation:

### Built with Rust
- **Why Rust?** Fast, safe, and compiles to a single binary
- **No runtime dependencies:** Everything is self-contained
- **Performance:** Most operations complete in < 10ms

### Key Technologies
| Component | Technology Used |
|-----------|----------------|
| Code parsing | Tree-sitter (battle-tested parser framework) |
| Search engine | Tantivy (pure Rust, like Lucene) |
| Secret detection | Aho-Corasick algorithm (industry standard) |
| Git analysis | libgit2 (used by GitHub itself) |
| Communication | JSON-RPC over stdio (Model Context Protocol) |

### Plugin Architecture
Each component is a separate "plugin" that can be enabled/disabled:

```yaml
# .synapseed/dna.yaml
plugins:
  - cortex    # Parser
  - search    # Search engine
  - husk      # Security scanner
  - chronos   # Git analyzer
  - architect # Code quality inspector
```

This means:
- ✅ You only load what you need
- ✅ New features can be added without changing the core
- ✅ Each plugin can be tested independently

## Data Flow Example

Let's trace what happens when you run: `synapseed search "error handling"`

```
1. CLI parses command
   └─► Calls Search plugin

2. Search plugin:
   ├─► Builds index of all code files (first time only)
   ├─► Tokenizes query: ["error", "handling"]
   ├─► Searches index using BM25 ranking
   └─► Returns top 10 matches with scores

3. CLI formats results:
   └─► Displays file paths, line numbers, and context

Total time: ~5ms (after initial index)
```

## Security Model

SYNAPSEED is designed to be **secure by default**:

1. **No network calls** - Everything runs locally
2. **Read-only** - Never modifies your files
3. **Process isolation** - Runs in its own sandboxed process
4. **Fail-closed** - Errors block the operation, not allow it
5. **Secret scanning** - All data is checked before being sent anywhere

## Common Questions

### Q: Does SYNAPSEED modify my code?
**A:** No. It only reads and analyzes. The only exception is the Janitor feature, which can *optionally* apply fixes with your explicit approval.

### Q: Where is my data stored?
**A:** SYNAPSEED creates a `.synapseed/` folder in your project with:
- Configuration (`dna.yaml`)
- Optional search index cache (for faster startup)
- Session history (if enabled)

You can delete this folder anytime with no harm to your code.

### Q: Does it send my code to the cloud?
**A:** Absolutely not. Everything runs on your machine. Zero network calls.

### Q: How much memory does it use?
**A:** Typical usage: 50-200MB depending on project size. Configurable limit in `dna.yaml`.

### Q: Can I use it without an AI assistant?
**A:** Yes! The CLI works standalone. AI integration is optional.

## What Makes SYNAPSEED Different?

| Traditional Approach | SYNAPSEED Approach |
|---------------------|-------------------|
| AI reads raw file contents | AI sees structured code analysis |
| Search for exact text matches | Search for concepts and patterns |
| Manual security reviews | Automated real-time secret detection |
| Subjective code quality | Objective metrics and scores |
| AI makes educated guesses | AI gets factual structural data |

## Performance Characteristics

Real measurements on a typical laptop (M1 MacBook):

| Operation | Time | Notes |
|-----------|------|-------|
| Initial code parsing | 1-3s | For ~50,000 lines |
| Symbol lookup | 1-5ms | After initial parse |
| Semantic search | 3-10ms | After index build |
| Secret scan | 0.5-2ms | Per 1KB of text |
| Git history query | 10-50ms | Depends on repo size |
| Architecture analysis | 100-500ms | Full project scan |

## Learn More

- **Want to contribute?** See [GOOD_FIRST_ISSUES.md](GOOD_FIRST_ISSUES.md)
- **Detailed docs:** [docs/guide/](docs/guide/)
- **Integration guides:** [docs/integration/](docs/integration/)
- **API reference:** [docs/reference/](docs/reference/)

## TL;DR

SYNAPSEED is like a Swiss Army knife for code intelligence:
- 🔍 Understands code structure
- 🛡️ Protects secrets
- 📊 Tracks history
- 🏗️ Measures quality
- ⚡ Does it all locally and fast

It sits between you (or an AI) and your codebase, making code understanding easier and safer.
