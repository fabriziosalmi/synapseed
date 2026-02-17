use synapseed_core::context::SynapseContext;
use synapseed_cortex::graph::CodeGraph;
use tracing::debug;

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
    let mut ghost_count = 0usize;
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

            // Ghost detection (v5.0.1): verify the source file still exists
            // on disk. The CodeGraph is built at init and may reference files
            // that have been deleted since. Injecting stale symbols causes the
            // LLM to hallucinate about code that no longer exists.
            let abs = root.join(&sym.file_path);
            if !abs.exists() {
                ghost_count += 1;
                debug!(
                    file = %sym.file_path,
                    symbol = %sym.name,
                    "Ghost detected: symbol references deleted file, skipping"
                );
                continue;
            }

            let mut val = serde_json::to_value(&sym).unwrap_or_default();

            // v4.27.0 Body Enrichment: inject a snippet field for container
            // and behavioral symbols so build_human_summary can extract
            // member names (traits/structs) and key body details (functions).
            let enrichable = matches!(
                sym.kind,
                synapseed_core::symbol::SymbolKind::Interface
                    | synapseed_core::symbol::SymbolKind::Struct
                    | synapseed_core::symbol::SymbolKind::Enum
                    | synapseed_core::symbol::SymbolKind::Function
                    | synapseed_core::symbol::SymbolKind::Method
            );
            if enrichable && sym.line_end > sym.line_start {
                if let Ok(content) = std::fs::read_to_string(&abs) {
                    let lines: Vec<&str> = content.lines().collect();
                    let start = sym.line_start.saturating_sub(1);
                    let end = sym.line_end.min(lines.len());
                    let snippet_end = end.min(start + 40);
                    if start < snippet_end && snippet_end <= lines.len() {
                        let snippet: String = lines[start..snippet_end].join("\n");
                        if let serde_json::Value::Object(ref mut map) = val {
                            map.insert("snippet".into(), serde_json::Value::String(snippet));
                        }
                    }
                }
            }

            symbols.push(val);
        }
    }

    if ghost_count > 0 {
        debug!(ghosts = ghost_count, "Ghost detection: filtered stale symbols");
    }

    // v4.28.0 Metadata File Fallback: if the CodeGraph doesn't contain
    // a target (e.g. Cargo.toml is not parsed by tree-sitter), read the
    // file directly and inject a pseudo-symbol with key facts extracted.
    for target in targets {
        let fp = match &target.file_path {
            Some(p) => p,
            None => continue,
        };
        // Only process metadata files not already found in CodeGraph
        let is_metadata = fp.ends_with(".toml") || fp == "LICENSE" || fp.ends_with(".lock");
        if !is_metadata {
            continue;
        }
        // Skip if we already have a symbol from this file
        if symbols.iter().any(|s| {
            s.get("file_path")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p == fp)
        }) {
            continue;
        }
        let abs = root.join(fp);
        if let Ok(content) = std::fs::read_to_string(&abs) {
            // Extract key-value facts from TOML files
            let mut facts = Vec::new();
            for line in content.lines().take(50) {
                let trimmed = line.trim();
                if let Some((key, val)) = trimmed.split_once(" = ") {
                    let key = key.trim();
                    let val = val.trim().trim_matches('"');
                    match key {
                        "version" | "name" | "edition" | "license" | "repository" => {
                            facts.push(format!("{key} = \"{val}\""));
                        }
                        _ => {}
                    }
                }
            }
            let signature = if facts.is_empty() {
                fp.to_string()
            } else {
                facts.join(", ")
            };
            let snippet: String = content.lines().take(30).collect::<Vec<_>>().join("\n");
            let mut map = serde_json::Map::new();
            map.insert("name".into(), serde_json::Value::String(fp.to_string()));
            map.insert("kind".into(), serde_json::Value::String("Constant".into()));
            map.insert("file_path".into(), serde_json::Value::String(fp.to_string()));
            map.insert("signature".into(), serde_json::Value::String(signature));
            map.insert("line_start".into(), serde_json::json!(1));
            map.insert("line_end".into(), serde_json::json!(content.lines().count()));
            map.insert("snippet".into(), serde_json::Value::String(snippet));
            symbols.push(serde_json::Value::Object(map));
            debug!(file = fp, "Metadata fallback: injected pseudo-symbol");
        }
    }

    if symbols.is_empty() {
        return None;
    }

    // Dedup by symbol name
    symbols.dedup_by(|a, b| a["name"] == b["name"]);
    Some(CodeContext { symbols })
}
