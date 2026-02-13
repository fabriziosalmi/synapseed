use synapseed_core::context::SynapseContext;
use synapseed_cortex::graph::CodeGraph;

use super::{CodeContext, Intent, Target};

pub(super) fn gather_code_context(
    intent: &Intent,
    targets: &[Target],
    ctx: &SynapseContext,
) -> Option<CodeContext> {
    // v4.1.0: Security intent now gathers code context — "how does the
    // security scanner work?" needs symbols to produce SID > 0.
    if !matches!(
        intent,
        Intent::BugFix | Intent::Explain | Intent::Refactor | Intent::General | Intent::Security
    ) {
        return None;
    }

    // Retrieve the code graph from the context (Cortex plugin must be active)
    let graph = ctx.get_extension::<CodeGraph>()?;

    let root = ctx.project_root();
    let mut symbols = Vec::new();
    for target in targets {
        // Find symbols matching target name. Filters by file path if target has it.
        let candidates = graph.lookup(&target.name);
        for sym in candidates {
            if let Some(target_file) = &target.file_path {
                if !sym.file_path.ends_with(target_file) {
                    continue;
                }
            }
            // Relativize file_path before serialization to avoid leaking
            // absolute local paths into the LLM context.
            let mut sym = sym;
            if let Ok(rel) = std::path::Path::new(&sym.file_path).strip_prefix(&root) {
                sym.file_path = rel.display().to_string();
            }
            symbols.push(serde_json::to_value(&sym).unwrap_or_default());
        }
    }

    if symbols.is_empty() {
        return None;
    }

    // Dedup by symbol name
    symbols.dedup_by(|a, b| a["name"] == b["name"]);
    Some(CodeContext { symbols })
}
