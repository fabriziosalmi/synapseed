# Visual Demos and Screenshots

This document contains visual demonstrations of SYNAPSEED's key features.

> **Note:** This is a placeholder document. Actual screenshots and GIFs will be added as visual demonstrations become available.

## Planned Demonstrations

### 1. Terminal Usage Demo

**What to show:**
- Installation process
- First search command and output
- Secret scanning in action
- Architecture health check

**Format:** Animated GIF or short video (30-60 seconds)

**Why it matters:** New users can see exactly what to expect before installing.

---

### 2. Claude Desktop Integration

**What to show:**
- Adding SYNAPSEED to Claude config
- Claude using SYNAPSEED tools to analyze code
- Before/after comparison (Claude without vs. with SYNAPSEED)

**Format:** Screenshot series or screen recording

**Why it matters:** Demonstrates the AI integration use case.

---

### 3. Security Scanning in Action

**What to show:**
- Various types of secrets being detected
- Redaction in real-time
- Safe vs. unsafe command checking

**Format:** Terminal recording with asciinema

**Why it matters:** Shows the security features protecting sensitive data.

---

### 4. Code Search Comparison

**What to show:**
- Traditional text search (grep) finding only literal matches
- SYNAPSEED semantic search finding conceptual matches

**Side-by-side comparison:**
```
grep "error"              vs    synapseed search "error handling"
├─ error_log.txt         │     ├─ handle_failure()
├─ error_msg             │     ├─ process_exception()
└─ user_error_count      │     ├─ catch_error()
                         │     └─ error_recovery_strategy()
```

**Format:** Side-by-side terminal windows

**Why it matters:** Clearly demonstrates semantic understanding advantage.

---

### 5. Architecture Health Visualization

**What to show:**
- Running `synapseed architect`
- Grade report (A-F)
- Dependency graph visualization
- Before/after refactoring

**Format:** Static screenshots with annotations

**Why it matters:** Shows objective code quality metrics.

---

### 6. Git History Analysis

**What to show:**
- Running `synapseed blame`
- Semantic commit classification
- Churn analysis heatmap
- Co-change pattern detection

**Format:** Terminal output screenshots

**Why it matters:** Demonstrates time-travel and evolution tracking.

---

### 7. VS Code Extension

**What to show:**
- Extension sidebar panels
- Real-time diagnostics
- Dashboard view
- Security alerts

**Format:** VS Code window screenshots

**Why it matters:** Shows the IDE integration experience.

---

### 8. Benchmark Results

**What to show:**
- Running benchmark suite
- Results dashboard
- Accuracy comparison charts (BLIND vs SYNAPSEED)
- Performance metrics

**Format:** Dashboard screenshots with highlighted metrics

**Why it matters:** Provides empirical evidence of effectiveness.

---

## How to Contribute Demos

If you'd like to create visual demonstrations:

1. **Record your terminal:**
   ```bash
   # Use asciinema for terminal recordings
   asciinema rec demo.cast
   synapseed search "your query"
   # Press Ctrl+D when done
   ```

2. **Convert to GIF:**
   ```bash
   # Using agg (asciinema to GIF)
   agg demo.cast demo.gif
   ```

3. **Take screenshots:**
   - Use your OS screenshot tool
   - Highlight important parts with annotations
   - Save as PNG with descriptive names

4. **Submit:**
   - Add to `docs/assets/demos/`
   - Update this file with links
   - Open a pull request

## File Organization

When adding visuals, use this structure:

```
docs/assets/
  demos/
    terminal/
      01-search-demo.gif
      02-security-scan.gif
      03-architecture-health.png
    vscode/
      sidebar-overview.png
      diagnostics-panel.png
      dashboard-view.png
    benchmarks/
      accuracy-comparison.png
      performance-metrics.png
    integration/
      claude-before.png
      claude-after.png
```

## Recording Guidelines

### For Terminal Recordings

- **Resolution:** 1280x720 or higher
- **Font size:** Large enough to read (14pt+)
- **Duration:** Keep under 60 seconds
- **Focus:** One feature per demo
- **Annotations:** Add text overlays explaining what's happening

### For Screenshots

- **Format:** PNG for UI, GIF for animations
- **Quality:** High resolution, but compressed
- **Context:** Show enough context to understand what's happening
- **Cropping:** Remove unnecessary chrome/decorations

### For Screen Recordings

- **Tool recommendations:**
  - macOS: QuickTime, ScreenFlow
  - Linux: SimpleScreenRecorder, Kazam
  - Windows: OBS Studio, ShareX
- **Audio:** Optional but helpful for narration
- **Length:** 1-3 minutes maximum per feature
- **Editing:** Trim dead time, add captions if needed

## Examples to Create

Priority list for visual demonstrations:

1. ⏳ **5-minute quickstart** - Highest priority, shows end-to-end workflow (guide exists, video needed)
2. ⏳ **Secret scanning demo** - Show security in action
3. ⏳ **Search comparison** - grep vs semantic search
4. ⏳ **Claude integration** - Before/after with AI assistant
5. ⏳ **Architecture analysis** - Health scores and metrics
6. ⏳ **VS Code extension tour** - All panels and features
7. ⏳ **Benchmark results** - Data-driven proof
8. ⏳ **Git analysis** - History and churn visualization

## Questions?

Open an issue or discussion on GitHub if you'd like to contribute demos but need guidance on what to record or how to do it.

---

**Status:** This document is a placeholder. Contributions welcome!
