---
description: How to generate the arXiv paper for Synapseed
---

This workflow describes how to generate the LaTeX tables and compile the full PDF paper for Synapseed using the `arxiv-generator` toolchain.

1. **Navigate to the toolchain directory**
   ```bash
   cd tools/arxiv-generator
   ```

2. **Generate Tables Only**
   If you only want to update the data tables (Search, Grounding, Performance):
   ```bash
   make tables
   ```
   **Artifacts**: Check `tools/arxiv-generator/assets/*.tex`.

3. **Generate Full Paper (PDF)**
   To generate all assets and compile the PDF (requires `pdflatex` installed):
   ```bash
   // turbo
   make paper
   ```
   > [!NOTE]
   > If `pdflatex` is missing, the command will fail after generating the `.tex` files. You can manually compile `tools/arxiv-generator/src/templates/main.tex` using Overleaf or another LaTeX editor.
   
   **Artifacts**: Check `tools/arxiv-generator/assets/main.pdf`.


4. **Verify Data**
   Ensure that the data is fresh ( < 24h old). The tool will automatically fail if benchmark data is stale.
