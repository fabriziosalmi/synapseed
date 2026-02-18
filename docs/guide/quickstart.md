# Quick Start

Get SYNAPSEED running in 5 minutes with this hands-on guide.

## Installation (2 minutes)

**Prerequisites:**
- Rust 1.75+ ([install from rustup.rs](https://rustup.rs/))
- Git 2.x+

```bash
# Clone the repository
git clone https://github.com/fabriziosalmi/synapseed.git
cd synapseed

# Build and install
cargo install --path bin/synapseed --force

# Verify installation
synapseed --version
```

**Expected output:**
```
synapseed 0.4.0
```

✅ If you see a version number, you're ready to go!

---

## Your First Commands (3 minutes)

Let's try SYNAPSEED on its own codebase:

### 1. Search for Code

```bash
synapseed search "error handling" --project .
```

**What you'll see:**
```
🔍 Searching for: "error handling"
✓ Found 12 matches in 0.004s

Results:
src/husk/scanner.rs:89 - DlpError enum
src/cortex/parser.rs:45 - handle_parse_error()
src/chronos/git.rs:156 - GitError::NotFound
...
```

This found all places where error handling is implemented, even when the code uses different words.

### 2. Find a Specific Symbol

```bash
synapseed lookup SynapseContext --project .
```

**What you'll see:**
```
Found: SynapseContext
File: crates/core/src/context.rs
Line: 45-120
Kind: Struct
Members: 8 fields, 15 methods
```

### 3. Scan for Secrets

```bash
echo "password=secret123" | synapseed scan
```

**What you'll see:**
```
⚠️  ALERT: Generic Secret Pattern detected
Position: 9-18
Severity: MEDIUM

Sanitized: password=[REDACTED]
```

### 4. Check Architecture Health

```bash
synapseed architect --project .
```

**What you'll see:**
```
Architecture Health Report
Grade: A (95/100)
Modules: 16
Violations: 0 critical, 2 minor
Coupling Score: 0.23 (low is good)
Cyclomatic Complexity: Average 4.2
```

### 5. Ask a Natural Language Question

```bash
synapseed ask "what does this project do?" --project .
```

**What you'll see:**
```
🤔 Analyzing project structure...

📊 SYNAPSEED Analysis:
- Type: Multi-crate Rust workspace
- Purpose: Code intelligence and security analysis tool
- Main components:
  * cortex: Code parsing (AST analysis)
  * husk: Secret scanning (DLP)
  * search: Semantic code search
  * chronos: Git history analysis
  
The project provides AI assistants with structured
code understanding through the Model Context Protocol (MCP).
```

---

## Integration with Claude Desktop

To use SYNAPSEED with Claude, add this to your Claude Desktop configuration:

**macOS:** `~/Library/Application Support/Claude/claude_desktop_config.json`  
**Windows:** `%APPDATA%\Claude\claude_desktop_config.json`  
**Linux:** `~/.config/Claude/claude_desktop_config.json`

```json
{
  "mcpServers": {
    "synapseed": {
      "command": "synapseed",
      "args": ["serve", "--project", "/path/to/your/project"],
      "env": { "RUST_LOG": "warn" }
    }
  }
}
```

**Test it:**
1. Restart Claude Desktop
2. Look for the 🔌 icon indicating SYNAPSEED is connected
3. Ask Claude: "What functions are in my codebase?"
4. Claude can now use SYNAPSEED tools to analyze your code!

---

## Using with Your Own Project

Point SYNAPSEED at any codebase:

```bash
# Navigate to your project
cd /path/to/your/project

# Initialize SYNAPSEED (optional but recommended)
synapseed init --project .

# Try any command
synapseed search "authentication" --project .
synapseed hoist --project .  # Parse entire codebase
synapseed history --limit 20 --project .
```

**Supported Languages:**
Rust, Python, JavaScript, TypeScript, Go, Java, C, C++, and 20+ more via tree-sitter.

---

## Common Use Cases

### Finding Code
```bash
# Search by concept
synapseed search "database connection" --project .

# Find a specific function
synapseed lookup connect_to_database --project .

# Get overview of a directory
synapseed hoist src/database/ --project .
```

### Security Scanning
```bash
# Scan a file
synapseed scan --file config.yaml

# Scan clipboard content
pbpaste | synapseed scan  # macOS
xclip -o | synapseed scan  # Linux

# Check if a command is safe
synapseed check "cargo test" --project .
```

### Git Analysis
```bash
# See recent changes
synapseed history --limit 10 --project .

# Blame a file
synapseed blame src/main.rs --start 1 --end 50 --project .

# Analyze code churn
synapseed analyze src/auth/login.rs --project .
```

### Code Quality
```bash
# Get architecture health score
synapseed architect --refresh --project .

# Find maintenance issues
synapseed janitor --project .

# Run diagnostics
synapseed diagnostics --project .
```

---

## Next Steps

- 📖 Read the [Introduction](introduction.md) to understand the full architecture
- 🔧 Configure SYNAPSEED with [Configuration Guide](configuration.md)
- 🛠️ Learn the [Complete Workflow](workflow.md)
- 🤝 Start contributing with [Good First Issues](https://github.com/fabriziosalmi/synapseed/blob/main/GOOD_FIRST_ISSUES.md)

---

## Troubleshooting

### "Command not found: synapseed"

Add Cargo's bin directory to your PATH:

```bash
# Add to ~/.bashrc or ~/.zshrc
export PATH="$HOME/.cargo/bin:$PATH"

# Apply immediately
source ~/.bashrc  # or source ~/.zshrc
```

### "Rust version too old"

Update Rust:
```bash
rustup update stable
```

### "Failed to parse directory"

Make sure you're in a directory with source code files. SYNAPSEED looks for:
- `.rs` (Rust)
- `.py` (Python)  
- `.js`, `.ts` (JavaScript/TypeScript)
- `.go` (Go)
- And 20+ other file types

### "Permission denied"

On Linux/macOS, ensure the binary is executable:
```bash
chmod +x ~/.cargo/bin/synapseed
```

---

## Performance Tips

For large codebases (100k+ lines), enable disk-based indexing:

```bash
# Create .synapseed/dna.yaml in your project
mkdir -p .synapseed
cat > .synapseed/dna.yaml << EOF
search:
  persistence: true  # Saves index to disk
  
hci:
  memory_ceiling_files: 50000  # Adjust based on your RAM
  background_indexing: true     # Non-blocking startup
EOF
```

This makes subsequent runs much faster by caching the parsed code structure.
