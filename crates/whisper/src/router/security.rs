use synapseed_core::context::SynapseContext;
use synapseed_core::error::safe_resolve_path;
use synapseed_cortex::graph::CodeGraph;
use synapseed_husk::guard::SecurityGuard;

use super::{Intent, Target};

pub(super) fn gather_security(intent: &Intent, targets: &[Target], ctx: &SynapseContext) -> String {
    // Always scan for Security intent; also scan for BugFix (might reveal root cause)
    if !matches!(intent, Intent::Security | Intent::BugFix) {
        return "NOT_SCANNED".to_string();
    }

    let root = ctx.project_root();
    let guard = SecurityGuard::with_defaults();

    // If we have specific target files, scan them
    let mut findings = Vec::new();
    for target in targets {
        if let Some(file_path) = &target.file_path {
            let abs_path = match safe_resolve_path(&root, file_path) {
                Ok(p) => p,
                Err(e) => {
                    findings.push(format!("{file_path}: {e}"));
                    continue;
                }
            };

            if let Ok(content) = std::fs::read_to_string(&abs_path) {
                if let Err(e) = guard.check(&content) {
                    findings.push(format!("{}: {}", file_path, e));
                }
            }
        }
    }

    // If no targets but Security intent, scan all indexed source files
    if findings.is_empty() && matches!(intent, Intent::Security) && targets.is_empty() {
        let graph = CodeGraph::new();
        if graph.index_directory(&root).is_ok() {
            for file in graph.all_files() {
                // Files from the index are already relative to root; still validate.
                let abs_path = match safe_resolve_path(&root, &file.path) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                if let Ok(content) = std::fs::read_to_string(&abs_path) {
                    if let Err(e) = guard.check(&content) {
                        findings.push(format!("{}: {}", file.path, e));
                    }
                }
            }
        }
    }

    if findings.is_empty() {
        "CLEAN".to_string()
    } else {
        format!("ALERT: {}", findings.join("; "))
    }
}
