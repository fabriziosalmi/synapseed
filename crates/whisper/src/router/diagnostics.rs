use synapseed_core::context::SynapseContext;
use synapseed_shadow_check::diagnostic::DiagnosticLevel;
use synapseed_shadow_check::runner::DiagnosticStore;

use super::{DiagnosticsContext, Intent, Target};

pub(super) fn gather_diagnostics(
    intent: &Intent,
    targets: &[Target],
    ctx: &SynapseContext,
) -> Option<DiagnosticsContext> {
    if !matches!(intent, Intent::BugFix | Intent::Refactor | Intent::General) {
        return None;
    }

    let store = ctx.get_extension::<DiagnosticStore>()?;

    let file_paths: Vec<&str> = targets
        .iter()
        .filter_map(|t| t.file_path.as_deref())
        .collect();

    let diagnostics = if file_paths.is_empty() {
        store.snapshot().diagnostics
    } else {
        file_paths.iter().flat_map(|f| store.for_file(f)).collect()
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
