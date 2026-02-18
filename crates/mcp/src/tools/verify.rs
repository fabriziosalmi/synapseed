use synapseed_core::context::SynapseContext;
use synapseed_core::error::safe_resolve_path;

use super::{error_result, text_result};
use crate::protocol::ToolCallResult;

pub(super) fn tool_verify_path(args: &serde_json::Value, ctx: &SynapseContext) -> ToolCallResult {
    let path = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return error_result("Missing required parameter: path".into()),
    };

    let root = ctx.project_root();

    // Security: validate the path stays within project root
    let abs_path = match safe_resolve_path(&root, path) {
        Ok(p) => p,
        Err(_) => {
            return text_result(
                serde_json::to_string_pretty(&serde_json::json!({
                    "exists": false,
                    "error": "Path traversal blocked: path must be within project root"
                }))
                .unwrap_or_default(),
            );
        }
    };

    match std::fs::metadata(&abs_path) {
        Ok(meta) => {
            let ext = abs_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let language = match ext {
                "rs" => "rust",
                "py" | "pyi" => "python",
                "js" | "mjs" | "cjs" => "javascript",
                "ts" | "tsx" | "mts" => "typescript",
                "go" => "go",
                "java" => "java",
                "toml" => "toml",
                "yaml" | "yml" => "yaml",
                "json" => "json",
                "md" => "markdown",
                other => other,
            };

            text_result(
                serde_json::to_string_pretty(&serde_json::json!({
                    "exists": true,
                    "size_bytes": meta.len(),
                    "language": language,
                    "is_file": meta.is_file(),
                }))
                .unwrap_or_default(),
            )
        }
        Err(_) => text_result(
            serde_json::to_string_pretty(&serde_json::json!({
                "exists": false
            }))
            .unwrap_or_default(),
        ),
    }
}
