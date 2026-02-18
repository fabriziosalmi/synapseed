//! MCP tool: analyze_binary — The Neural Decompiler.

use serde_json::json;
use tracing::info;

use synapseed_core::context::SynapseContext;
use synapseed_core::error::safe_resolve_path;
use synapseed_decompiler::{analyze_binary, BehaviorTag};

use super::{error_result, text_result};
use crate::protocol::ToolCallResult;

/// Analyze a compiled binary or shared library.
///
/// Extracts symbols, strings, call graph, and infers behavioral categories.
pub(super) fn tool_analyze_binary(
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    let path_str = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return error_result("Missing required parameter: path".into()),
    };

    let root = ctx.project_root();
    let abs_path = match safe_resolve_path(&root, path_str) {
        Ok(p) => p,
        Err(_) => {
            return error_result(format!(
                "Path traversal blocked: '{}' is outside the project root",
                path_str
            ))
        }
    };

    if !abs_path.exists() {
        return error_result(format!("File not found: '{}'", path_str));
    }

    info!(path = %abs_path.display(), "Neural Decompiler: analyzing binary");

    match analyze_binary(&abs_path) {
        Ok(analysis) => {
            let max_symbols = args
                .get("max_symbols")
                .and_then(|v| v.as_u64())
                .unwrap_or(50) as usize;
            let max_strings = args
                .get("max_strings")
                .and_then(|v| v.as_u64())
                .unwrap_or(30) as usize;

            // Build human-readable output
            let mut out = String::new();

            // Header
            out.push_str(&format!("# Binary Analysis: {}\n\n", analysis.info.name));

            // Info
            out.push_str("## Metadata\n");
            out.push_str(&format!("- **Format**: {}\n", analysis.info.format));
            out.push_str(&format!("- **Architecture**: {}\n", analysis.info.arch));
            out.push_str(&format!(
                "- **Size**: {} bytes ({:.1} KB)\n",
                analysis.info.size,
                analysis.info.size as f64 / 1024.0
            ));
            out.push_str(&format!("- **Library**: {}\n", analysis.info.is_lib));
            out.push_str(&format!("- **Stripped**: {}\n", analysis.info.is_stripped));
            if analysis.info.entry_point != 0 {
                out.push_str(&format!(
                    "- **Entry point**: 0x{:x}\n",
                    analysis.info.entry_point
                ));
            }
            out.push_str(&format!("- **Sections**: {}\n\n", analysis.info.sections));

            // Behaviors
            if !analysis.behaviors.is_empty() {
                out.push_str("## Inferred Behaviors\n");
                for b in &analysis.behaviors {
                    let (icon, desc) = behavior_display(b);
                    out.push_str(&format!("- {} {}\n", icon, desc));
                }
                out.push('\n');
            }

            // Summary
            out.push_str(&format!("## Summary\n{}\n\n", analysis.summary));

            // Symbols
            let total_syms = analysis.symbols.len();
            let exports: Vec<_> = analysis.symbols.iter().filter(|s| !s.is_import).collect();
            let imports: Vec<_> = analysis.symbols.iter().filter(|s| s.is_import).collect();
            out.push_str(&format!(
                "## Symbols ({} total: {} exports, {} imports)\n",
                total_syms,
                exports.len(),
                imports.len()
            ));

            if !exports.is_empty() {
                out.push_str("\n### Exports\n");
                for sym in exports.iter().take(max_symbols) {
                    let display = sym.demangled.as_deref().unwrap_or(&sym.name);
                    let kind = format!("{:?}", sym.kind).to_lowercase();
                    out.push_str(&format!(
                        "- `{}` ({}, 0x{:x})\n",
                        display, kind, sym.address
                    ));
                }
                if exports.len() > max_symbols {
                    out.push_str(&format!("  ... and {} more\n", exports.len() - max_symbols));
                }
            }

            if !imports.is_empty() {
                out.push_str("\n### Imports\n");
                for sym in imports.iter().take(max_symbols) {
                    let display = sym.demangled.as_deref().unwrap_or(&sym.name);
                    out.push_str(&format!("- `{}`\n", display));
                }
                if imports.len() > max_symbols {
                    out.push_str(&format!("  ... and {} more\n", imports.len() - max_symbols));
                }
            }
            out.push('\n');

            // Strings
            let interesting: Vec<_> = analysis
                .strings
                .iter()
                .filter(|s| {
                    !matches!(
                        s.class,
                        synapseed_decompiler::StringClass::General
                            | synapseed_decompiler::StringClass::PackageName
                    )
                })
                .collect();
            if !interesting.is_empty() {
                out.push_str(&format!(
                    "## Interesting Strings ({} classified, {} total extracted)\n",
                    interesting.len(),
                    analysis.strings.len()
                ));
                for cs in interesting.iter().take(max_strings) {
                    let class_str = format!("{:?}", cs.class);
                    let truncated: String = cs.value.chars().take(100).collect();
                    out.push_str(&format!("- **{}**: `{}`\n", class_str, truncated));
                }
                if interesting.len() > max_strings {
                    out.push_str(&format!(
                        "  ... and {} more\n",
                        interesting.len() - max_strings
                    ));
                }
                out.push('\n');
            }

            // Call graph
            let cg = &analysis.call_graph;
            if !cg.nodes.is_empty() {
                out.push_str(&format!(
                    "## Call Graph\n- **Nodes**: {} functions\n- **Edges**: {} calls\n- **Components**: {}\n- **Root functions**: {}\n- **Leaf functions**: {}\n",
                    cg.nodes.len(), cg.edges.len(), cg.components, cg.root_count, cg.leaf_count
                ));
            }

            text_result(out)
        }
        Err(e) => error_result(format!("Failed to analyze binary: {e}")),
    }
}

/// Analyze a dependency's compiled artifact by crate name.
pub(super) fn tool_explain_dependency(
    args: &serde_json::Value,
    ctx: &SynapseContext,
) -> ToolCallResult {
    let crate_name = match args.get("crate_name").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return error_result("Missing required parameter: crate_name".into()),
    };

    let root = ctx.project_root();

    // Look for the compiled artifact in target/debug/deps or target/release/deps
    let candidates = [
        root.join(format!(
            "target/debug/deps/lib{}.rlib",
            crate_name.replace('-', "_")
        )),
        root.join(format!(
            "target/debug/deps/lib{}.dylib",
            crate_name.replace('-', "_")
        )),
        root.join(format!(
            "target/debug/deps/lib{}.so",
            crate_name.replace('-', "_")
        )),
        root.join(format!(
            "target/release/deps/lib{}.rlib",
            crate_name.replace('-', "_")
        )),
        root.join(format!(
            "target/release/deps/lib{}.dylib",
            crate_name.replace('-', "_")
        )),
        root.join(format!(
            "target/release/deps/lib{}.so",
            crate_name.replace('-', "_")
        )),
    ];

    // Also check for exact prefix match in deps directory
    let mut found = None;
    for candidate in &candidates {
        if candidate.exists() {
            found = Some(candidate.clone());
            break;
        }
    }

    // Fallback: glob search in deps for prefix match
    if found.is_none() {
        let norm = crate_name.replace('-', "_");
        for dir in &["target/debug/deps", "target/release/deps"] {
            let deps_dir = root.join(dir);
            if deps_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&deps_dir) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if name.starts_with(&format!("lib{norm}-"))
                            && (name.ends_with(".rlib")
                                || name.ends_with(".dylib")
                                || name.ends_with(".so"))
                        {
                            found = Some(entry.path());
                            break;
                        }
                    }
                }
            }
            if found.is_some() {
                break;
            }
        }
    }

    match found {
        Some(path) => {
            info!(crate_name, path = %path.display(), "Explaining dependency via binary analysis");
            // Re-use analyze_binary with adapted args
            let new_args = json!({
                "path": path.strip_prefix(&root).unwrap_or(&path).to_string_lossy(),
                "max_symbols": 30,
                "max_strings": 20,
            });
            tool_analyze_binary(&new_args, ctx)
        }
        None => error_result(format!(
            "Compiled artifact for crate '{}' not found.\nTry running `cargo build` first, then retry.",
            crate_name
        )),
    }
}

fn behavior_display(b: &BehaviorTag) -> (&str, &str) {
    match b {
        BehaviorTag::NetworkIO => ("🌐", "Network I/O (TCP, HTTP, DNS, TLS)"),
        BehaviorTag::FileIO => ("📁", "File system operations"),
        BehaviorTag::Crypto => ("🔒", "Cryptographic operations"),
        BehaviorTag::Serialization => ("📦", "Serialization/deserialization"),
        BehaviorTag::MemoryManagement => ("🧠", "Memory management"),
        BehaviorTag::Concurrency => ("⚡", "Threading/concurrency"),
        BehaviorTag::ProcessManagement => ("🔧", "Process management"),
        BehaviorTag::Logging => ("📝", "Logging/tracing"),
        BehaviorTag::Database => ("💾", "Database operations"),
        BehaviorTag::Compression => ("🗜️", "Compression"),
    }
}
