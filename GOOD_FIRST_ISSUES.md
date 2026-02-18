# Good First Issues for New Contributors

Welcome! This document contains beginner-friendly tasks to help you get started with SYNAPSEED. No PhD in computer science required! 🎓

## Prerequisites

Before starting any task:
1. ✅ Install Rust (via [rustup.rs](https://rustup.rs/))
2. ✅ Clone the repo: `git clone https://github.com/fabriziosalmi/synapseed.git`
3. ✅ Run `cargo test` to ensure everything builds

## 🟢 Level 1: Documentation & Examples (No Code)

Perfect for your first contribution! Help others understand the project better.

### 1. Add Missing Examples to README
**Difficulty:** ⭐ Easy  
**Time:** 15-30 minutes  
**What to do:**
- Pick a feature from the README that lacks an example
- Try using it yourself
- Add a code block showing the command and output
- **Files to edit:** `README.md`

**Example:**
```bash
# Current documentation says: "Use synapseed blame to see git history"
# Add this:

$ synapseed blame src/main.rs --start 1 --end 10
Line 1-3: Alice (2024-01-15) "feat: add authentication"
Line 4-7: Bob (2024-01-20) "fix: handle null user"
```

### 2. Create a Troubleshooting Guide
**Difficulty:** ⭐ Easy  
**Time:** 30-60 minutes  
**What to do:**
- Document common problems you encountered while setting up
- Add solutions or workarounds
- Create a new file: `docs/guide/troubleshooting.md`

**Common issues to cover:**
- Rust version too old
- Missing git in PATH
- Permission errors
- Build failures

### 3. Add Screenshots to Documentation
**Difficulty:** ⭐ Easy  
**Time:** 20-40 minutes  
**What to do:**
- Run SYNAPSEED commands
- Take screenshots of interesting outputs
- Add them to docs with descriptions
- **Files to edit:** Any file in `docs/`

## 🟡 Level 2: Small Code Changes (Beginner-Friendly)

These tasks involve simple, localized code changes with clear instructions.

### 4. Add a New Secret Pattern to DLP Scanner
**Difficulty:** ⭐⭐ Moderate  
**Time:** 30-60 minutes  
**What to do:**
- Add detection for a new type of secret (e.g., Stripe API keys)
- **Files to edit:** `crates/husk/src/patterns.rs`
- **Test:** `crates/husk/tests/dlp_tests.rs`

**Step-by-step:**
```rust
// 1. Add pattern to patterns.rs
pub const STRIPE_KEY: &str = r"sk_live_[0-9a-zA-Z]{24}";

// 2. Add to detection list
patterns.push(("STRIPE_KEY", STRIPE_KEY));

// 3. Add test in dlp_tests.rs
#[test]
fn test_stripe_key_detection() {
    // Example key format: sk_live_ followed by 24 chars
    let input = "API key: sk_live_EXAMPLE_KEY_NOT_REAL";
    let result = scan_for_secrets(input);
    assert!(result.contains("STRIPE_KEY"));
}
```

### 5. Add a CLI Alias for a Tool
**Difficulty:** ⭐⭐ Moderate  
**Time:** 20-40 minutes  
**What to do:**
- Make CLI commands more intuitive by adding aliases
- **Files to edit:** `bin/synapseed/src/main.rs`

**Example:**
```rust
// Current: synapseed lookup MyFunction
// Add alias: synapseed find MyFunction

match cmd_name {
    "lookup" | "find" => { /* existing lookup code */ }
    // ...
}
```

### 6. Improve Error Messages
**Difficulty:** ⭐⭐ Moderate  
**Time:** 30-60 minutes  
**What to do:**
- Find cryptic error messages
- Make them more helpful with context and suggestions

**Before:**
```
Error: IoError
```

**After:**
```
Error: Could not read file 'config.yaml'
Suggestion: Check if the file exists and you have read permissions.
Try: ls -la .synapseed/
```

## 🔴 Level 3: Feature Additions (Intermediate)

These involve adding new functionality with guidance.

### 7. Add Support for a New Programming Language
**Difficulty:** ⭐⭐⭐ Challenging  
**Time:** 2-4 hours  
**What to do:**
- Add tree-sitter parser for a new language (e.g., Go, Ruby, Java)
- **Files to edit:** `crates/cortex/src/parsers.rs`
- **Dependencies:** Add tree-sitter grammar to `Cargo.toml`

**Guided steps:**
1. Add the tree-sitter grammar dependency to `Cargo.toml`
2. Update `parsers.rs` to register the new language
3. Add test cases in the cortex tests
4. Update documentation to include the new language

See the existing language implementations in `crates/cortex/src/parsers.rs` for examples.

### 8. Create a New Benchmark Test
**Difficulty:** ⭐⭐⭐ Challenging  
**Time:** 1-2 hours  
**What to do:**
- Add a new coding task to the benchmark suite
- **Files to edit:** `benchmark/coding/tasks.py`

**Template:**
```python
CodingTask(
    id="easy_10",
    difficulty="easy",
    description="Find all functions that handle user input",
    ground_truth="Functions: parse_request(), validate_input(), sanitize_data()",
    target_repo="your-test-repo"
)
```

## 🚀 How to Submit Your Contribution

1. **Create a branch:**
   ```bash
   git checkout -b docs/add-troubleshooting-guide
   ```

2. **Make your changes** and test them:
   ```bash
   cargo fmt --all          # Format code
   cargo clippy -- -D warnings  # Check for issues
   cargo test               # Run tests
   ```

3. **Commit with a clear message:**
   ```bash
   git commit -m "docs: add troubleshooting guide for common setup issues"
   ```

4. **Push and open a PR:**
   ```bash
   git push origin docs/add-troubleshooting-guide
   ```
   Then open a Pull Request on GitHub with:
   - Clear title describing what you did
   - Description of the changes
   - Mention "Fixes #<issue_number>" if applicable

## Getting Help

- 💬 **Questions?** Open a [GitHub Discussion](https://github.com/fabriziosalmi/synapseed/discussions)
- 🐛 **Found a bug?** Open an [Issue](https://github.com/fabriziosalmi/synapseed/issues)
- 📖 **Read the docs:** [docs/guide/](docs/guide/)
- 🤝 **Contributing guide:** [CONTRIBUTING.md](CONTRIBUTING.md)

## Why These Tasks Are Good for Beginners

1. ✅ **Clear scope** - Each task has defined boundaries
2. ✅ **Guided instructions** - Step-by-step guidance provided
3. ✅ **Low risk** - Changes are isolated and testable
4. ✅ **Quick feedback** - Most tasks take less than 2 hours
5. ✅ **Learn by doing** - Each task teaches something about the codebase

## After Your First Contribution

Congratulations! 🎉 You're now a SYNAPSEED contributor. Next steps:

1. Look for issues labeled `good-first-issue` on GitHub
2. Try a Level 2 or Level 3 task
3. Help review other contributors' PRs
4. Suggest new features or improvements

Thank you for contributing to SYNAPSEED! 🚀
