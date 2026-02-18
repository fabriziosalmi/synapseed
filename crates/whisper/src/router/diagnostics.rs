use synapseed_core::context::SynapseContext;
use synapseed_shadow_check::diagnostic::DiagnosticLevel;
use synapseed_shadow_check::runner::DiagnosticStore;

use super::{DiagnosticsContext, Intent, Target};

pub(super) fn gather_diagnostics(
    intent: &Intent,
    targets: &[Target],
    ctx: &SynapseContext,
) -> Option<DiagnosticsContext> {
    // v4.1.0: Explain and Security also benefit from compiler diagnostics.
    // "explain why this fails" or "audit this code" need error context.
    if !matches!(
        intent,
        Intent::BugFix | Intent::Explain | Intent::Refactor | Intent::General | Intent::Security
    ) {
        return None;
    }

    let store = ctx.get_extension::<DiagnosticStore>()?;

    let file_paths: Vec<&str> = targets
        .iter()
        .filter_map(|t| t.file_path.as_deref())
        .collect();

    // v4.17.1 (W7): Always include global errors alongside target-scoped.
    // A query like "why is X broken?" needs all errors, not just those
    // in the target file — the root cause may be in a dependency.
    let snapshot = store.snapshot();
    let diagnostics = if file_paths.is_empty() {
        snapshot.diagnostics
    } else {
        let mut scoped: Vec<_> = file_paths.iter().flat_map(|f| store.for_file(f)).collect();
        // Merge global errors not already in the scoped set
        for d in &snapshot.diagnostics {
            if d.level == DiagnosticLevel::Error
                && !scoped
                    .iter()
                    .any(|s| s.file_path == d.file_path && s.line_start == d.line_start)
            {
                scoped.push(d.clone());
            }
        }
        scoped
    };

    let error_count = diagnostics
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .count();
    let warning_count = diagnostics
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Warning)
        .count();

    let items: Vec<serde_json::Value> = diagnostics
        .iter()
        .map(|d| serde_json::to_value(d).unwrap_or_default())
        .collect();

    Some(DiagnosticsContext {
        error_count,
        warning_count,
        items,
    })
}
