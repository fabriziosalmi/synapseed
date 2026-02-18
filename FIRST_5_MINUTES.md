# Your First 5 Minutes with SYNAPSEED

This guide will get you from zero to seeing real output in 5 minutes or less.

## What You'll Do

1. ✅ Install SYNAPSEED (2 minutes)
2. ✅ Run your first search (1 minute)
3. ✅ Try security scanning (1 minute)
4. ✅ See what SYNAPSEED found (1 minute)

Let's go! ⚡

---

## Step 1: Install SYNAPSEED (2 minutes)

**Prerequisites:** Rust installed? Check with: `rustc --version`
If not, install from [rustup.rs](https://rustup.rs/) (takes 3 minutes).

```bash
# Clone and install
git clone https://github.com/fabriziosalmi/synapseed.git
cd synapseed
cargo install --path bin/synapseed --force

# Verify it worked
synapseed --version
```

**Expected output:**
```
synapseed 0.4.0
```

✅ **Checkpoint:** If you see a version number, you're good to go!

---

## Step 2: Run Your First Search (1 minute)

Let's search the SYNAPSEED codebase itself:

```bash
cd /path/to/synapseed
synapseed search "error handling" --project .
```

**Expected output:**
```
🔍 Searching for: "error handling"
✓ Found 8 matches in 0.003s

src/cortex/parser.rs:45
  → handle_parse_error() - Processes parsing failures

src/husk/scanner.rs:89
  → DlpError enum - Security scanning error types

src/chronos/git.rs:156
  → GitError::NotFound - Repository not found error

[5 more results...]
```

**What just happened?**
SYNAPSEED parsed the entire codebase, built an index, and found all places where error handling logic exists - even if the code doesn't use those exact words.

✅ **Checkpoint:** You should see a list of files and functions.

---

## Step 3: Try Security Scanning (1 minute)

Test the secret detector:

```bash
# Scan some test data
echo "AWS_KEY=AKIAIOSFODNN7EXAMPLE" | synapseed scan
```

**Expected output:**
```
⚠️  SECURITY ALERT: Secret detected!
Type: AWS Access Key
Position: 8-32
Severity: HIGH

Sanitized output:
AWS_KEY=[REDACTED]
```

Now try with safe data:

```bash
echo "API_ENDPOINT=https://api.example.com" | synapseed scan
```

**Expected output:**
```
✅ No secrets detected.
Scanned 1 line (35 bytes) in 0.001s
```

✅ **Checkpoint:** You should see different outputs for secrets vs. safe data.

---

## Step 4: Ask SYNAPSEED a Question (1 minute)

This is the most powerful feature - natural language queries:

```bash
synapseed ask "what does this project do?" --project .
```

**Expected output:**
```
🤔 Analyzing project...

📊 Project Overview:
- Language: Rust (16 crates)
- Primary purpose: Code intelligence and security scanning
- Main components:
  * Cortex: Code parsing and AST analysis
  * Husk: Data Loss Prevention (DLP) scanner
  * Search: Full-text semantic search engine
  * Chronos: Git history analyzer

🎯 In short: SYNAPSEED helps AI assistants understand codebases by
providing structured analysis of code, secrets, and history.
```

✅ **Checkpoint:** You should see a structured analysis of the project.

---

## Next Steps (Optional)

Want to go deeper? Try these:

### Find a Specific Function
```bash
synapseed lookup SynapseContext --project .
```

### Check Architecture Health
```bash
synapseed architect --project .
```

### View Git History
```bash
synapseed history --limit 10 --project .
```

### See All Available Commands
```bash
synapseed help
```

---

## Integration with Claude Desktop

To make SYNAPSEED available to Claude, add this to your config:

**macOS:** `~/Library/Application Support/Claude/claude_desktop_config.json`
**Windows:** `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "synapseed": {
      "command": "synapseed",
      "args": ["serve", "--project", "/your/project/path"]
    }
  }
}
```

Restart Claude Desktop, and now Claude can:
- Find functions in your code
- Scan for security issues
- Understand git history
- Analyze architecture

---

## Troubleshooting

### "Command not found: synapseed"

The cargo install path isn't in your PATH. Add this to your `~/.bashrc` or `~/.zshrc`:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

Then run: `source ~/.bashrc` (or restart your terminal)

### "Failed to parse directory"

Make sure you're in a directory with source code. SYNAPSEED works with:
- Rust (`.rs` files)
- Python (`.py` files)
- JavaScript/TypeScript (`.js`, `.ts` files)
- And 27 other languages

### "Rust version too old"

Update Rust:
```bash
rustup update stable
```

SYNAPSEED requires Rust 1.75 or newer.

---

## What You Just Learned

In 5 minutes, you:
1. ✅ Installed a code intelligence tool
2. ✅ Searched a codebase semantically
3. ✅ Detected security issues in real-time
4. ✅ Asked natural language questions about code

**Ready for more?** Check out:
- [Full Documentation](docs/guide/introduction.md)
- [Integration Guides](docs/integration/)
- [Contributing Guide](GOOD_FIRST_ISSUES.md)

---

## Performance Note

Everything you just ran happened **locally on your machine**:
- ⚡ No API calls
- 🔒 Your code never leaves your computer
- 🚀 Sub-10ms response times for most operations

---

**Questions?** Open a [GitHub Issue](https://github.com/fabriziosalmi/synapseed/issues) or [Discussion](https://github.com/fabriziosalmi/synapseed/discussions).

**Want to contribute?** See [GOOD_FIRST_ISSUES.md](GOOD_FIRST_ISSUES.md) for beginner-friendly tasks!
