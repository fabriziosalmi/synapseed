//! MCP Resource definitions and handlers.
//!
//! Resources expose read-only data that the LLM can inspect.

use serde_json::json;

use synapseed_architect::ReportStore;
use synapseed_core::context::SynapseContext;
use synapseed_core::state::ProjectState;
use synapseed_janitor::ProposalStore;
use synapseed_root::sentinel::Sentinel;
use synapseed_shadow_check::runner::DiagnosticStore;
use synapseed_telemetry_sink::store::SpanStore;

use crate::protocol::{ResourceContent, ResourceDefinition};

/// Return all available resource definitions.
pub fn list_resources() -> Vec<ResourceDefinition> {
    vec![
        ResourceDefinition {
            uri: "synapseed://status".into(),
            name: "Project Status".into(),
            description: Some(
                "Current project state, metrics, and plugin health. Use this to understand the runtime situation."
                    .into(),
            ),
            mime_type: Some("application/json".into()),
        },
        ResourceDefinition {
            uri: "synapseed://dna".into(),
            name: "Project DNA".into(),
            description: Some(
                "Active configuration (workspace strategy, preferred libs, DLP level, plugins). The genetic code of this project."
                    .into(),
            ),
            mime_type: Some("application/json".into()),
        },
        ResourceDefinition {
            uri: "synapseed://security/policy".into(),
            name: "Security Policy".into(),
            description: Some(
                "Active DLP rules and command execution policy. Shows what gets blocked and why."
                    .into(),
            ),
            mime_type: Some("application/json".into()),
        },
        ResourceDefinition {
            uri: "synapseed://diagnostics/active".into(),
            name: "Active Diagnostics".into(),
            description: Some(
                "Live compiler diagnostics from the background shadow compiler. Shows errors, warnings, and available quick-fixes."
                    .into(),
            ),
            mime_type: Some("application/json".into()),
        },
        ResourceDefinition {
            uri: "synapseed://visualizer/url".into(),
            name: "Visualizer Dashboard URL".into(),
            description: Some(
                "URL of the live architecture visualizer dashboard. Open in a browser to see an interactive graph of the project structure with real-time file change highlighting."
                    .into(),
            ),
            mime_type: Some("application/json".into()),
        },
        ResourceDefinition {
            uri: "synapseed://telemetry/hotspots".into(),
            name: "Telemetry Hotspots".into(),
            description: Some(
                "Runtime performance hotspots from OTLP traces. Shows top-10 slowest code locations with call counts, average/max/p95 latencies. Use this to identify performance bottlenecks."
                    .into(),
            ),
            mime_type: Some("application/json".into()),
        },
        ResourceDefinition {
            uri: "synapseed://janitor/proposals".into(),
            name: "Janitor Proposals".into(),
            description: Some(
                "Pending maintenance proposals from the Janitor. Lists clippy fixes, unused dependencies, and other validated improvements ready to apply. Ask the Janitor to scan first with `janitor_run_now`."
                    .into(),
            ),
            mime_type: Some("application/json".into()),
        },
        ResourceDefinition {
            uri: "synapseed://architect/health".into(),
            name: "Architecture Health".into(),
            description: Some(
                "Structural health of the codebase: architecture score (A-F), module coupling metrics, detected violations (cycles, god objects, layer breaches), and top recommendations."
                    .into(),
            ),
            mime_type: Some("application/json".into()),
        },
    ]
}

/// Read a resource by URI and return its content.
pub fn read_resource(uri: &str, ctx: &SynapseContext) -> Option<ResourceContent> {
    match uri {
        "synapseed://status" => Some(resource_status(ctx)),
        "synapseed://dna" => Some(resource_dna(ctx)),
        "synapseed://security/policy" => Some(resource_security_policy()),
        "synapseed://diagnostics/active" => Some(resource_diagnostics(ctx)),
        "synapseed://visualizer/url" => Some(resource_visualizer_url()),
        "synapseed://telemetry/hotspots" => Some(resource_telemetry_hotspots(ctx)),
        "synapseed://janitor/proposals" => Some(resource_janitor_proposals(ctx)),
        "synapseed://architect/health" => Some(resource_architect_health(ctx)),
        _ => None,
    }
}

fn resource_status(ctx: &SynapseContext) -> ResourceContent {
    let state = ctx.project_state();
    let metrics = ctx.metrics();

    let state_label = match &state {
        ProjectState::VirginRepo => "virgin_repo",
        ProjectState::PartialSetup { .. } => "partial_setup",
        ProjectState::HealthyWorkspace { .. } => "healthy_workspace",
        ProjectState::Unknown => "unknown",
    };

    let text = serde_json::to_string_pretty(&json!({
        "project_root": ctx.project_root().display().to_string(),
        "state": state_label,
        "state_detail": state,
        "metrics": {
            "files_indexed": metrics.files_indexed,
            "symbols_found": metrics.symbols_found,
            "dlp_scans": metrics.dlp_scans,
            "dlp_blocks": metrics.dlp_blocks,
            "commands_allowed": metrics.commands_allowed,
            "commands_denied": metrics.commands_denied,
            "errors_prevented": metrics.errors_prevented,
            "events_broadcast": metrics.events_broadcast,
        }
    }))
    .unwrap_or_default();

    ResourceContent {
        uri: "synapseed://status".into(),
        mime_type: Some("application/json".into()),
        text: Some(text),
    }
}

fn resource_dna(ctx: &SynapseContext) -> ResourceContent {
    let dna = ctx.dna();

    let text = serde_json::to_string_pretty(&json!({
        "workspace_strategy": dna.workspace_strategy,
        "preferred_libs": dna.preferred_libs,
        "naming": {
            "core_crate": dna.naming.core_crate,
            "bin_name": dna.naming.bin_name,
        },
        "plugins": dna.plugins,
        "dlp_level": format!("{:?}", dna.dlp_level),
        "templates": dna.templates,
    }))
    .unwrap_or_default();

    ResourceContent {
        uri: "synapseed://dna".into(),
        mime_type: Some("application/json".into()),
        text: Some(text),
    }
}

fn resource_security_policy() -> ResourceContent {
    // Build policy summary from the default sentinel rules
    let sentinel_info = match Sentinel::with_defaults() {
        Ok(sentinel) => {
            let test_commands = [
                "ls -la",
                "cat /etc/passwd",
                "rm -rf /",
                "git status",
                "cargo test",
                "chmod 777 /etc/shadow",
                "dd if=/dev/zero of=/dev/sda",
                "mkfs.ext4 /dev/sda",
                "curl https://example.com",
            ];

            let mut evaluations = Vec::new();
            for cmd in &test_commands {
                let status = match sentinel.evaluate(cmd) {
                    Ok(action) => format!("ALLOWED ({action:?})"),
                    Err(e) => format!("DENIED: {e}"),
                };
                evaluations.push(json!({
                    "command": cmd,
                    "result": status,
                }));
            }

            json!({
                "mode": "fail-closed",
                "description": "Commands not matching any allow rule are DENIED by default.",
                "sample_evaluations": evaluations,
            })
        }
        Err(e) => {
            json!({
                "error": format!("Failed to load sentinel: {e}"),
            })
        }
    };

    let text = serde_json::to_string_pretty(&json!({
        "dlp": {
            "engine": "aho-corasick + regex",
            "default_rules": [
                "AWS Access Key (AKIA...)",
                "GitHub Token (ghp_/gho_/ghs_/ghr_/github_pat_)",
                "Generic Secret patterns (password=, api_key=, etc.)",
                "Private Key markers (BEGIN RSA/EC/OPENSSH PRIVATE KEY)",
            ],
            "mode": "fail-closed (block on any finding)",
        },
        "command_sentinel": sentinel_info,
    }))
    .unwrap_or_default();

    ResourceContent {
        uri: "synapseed://security/policy".into(),
        mime_type: Some("application/json".into()),
        text: Some(text),
    }
}

fn resource_diagnostics(ctx: &SynapseContext) -> ResourceContent {
    let text = match ctx.get_extension::<DiagnosticStore>() {
        Some(store) => {
            let snap = store.snapshot();
            serde_json::to_string_pretty(&json!({
                "error_count": snap.error_count,
                "warning_count": snap.warning_count,
                "last_check_ms": snap.last_check_ms,
                "diagnostics": snap.diagnostics,
            }))
            .unwrap_or_default()
        }
        None => serde_json::to_string_pretty(&json!({
            "status": "inactive",
            "message": "Shadow compiler not active. Requires a Cargo.toml in the project root.",
        }))
        .unwrap_or_default(),
    };

    ResourceContent {
        uri: "synapseed://diagnostics/active".into(),
        mime_type: Some("application/json".into()),
        text: Some(text),
    }
}

fn resource_visualizer_url() -> ResourceContent {
    let text = serde_json::to_string_pretty(&json!({
        "url": "http://localhost:3000",
        "description": "Live architecture dashboard with interactive Cytoscape.js graph",
        "features": [
            "Interactive code graph with file and symbol nodes",
            "Real-time WebSocket updates on file changes",
            "Pulse animation on modified files",
            "Automatic graph refresh on file creation/deletion",
            "Activity log with file change history",
        ],
    }))
    .unwrap_or_default();

    ResourceContent {
        uri: "synapseed://visualizer/url".into(),
        mime_type: Some("application/json".into()),
        text: Some(text),
    }
}

fn resource_telemetry_hotspots(ctx: &SynapseContext) -> ResourceContent {
    let text = match ctx.get_extension::<SpanStore>() {
        Some(store) => {
            let hotspots = store.hotspots();
            let stats = store.stats();
            let top10: Vec<_> = hotspots.into_iter().take(10).collect();
            serde_json::to_string_pretty(&json!({
                "total_spans": stats.total_spans,
                "unique_locations": stats.unique_locations,
                "buffer_usage": format!("{:.0}%", stats.buffer_usage * 100.0),
                "hotspots": top10,
            }))
            .unwrap_or_default()
        }
        None => serde_json::to_string_pretty(&json!({
            "status": "inactive",
            "message": "Telemetry sink not active. Configure an OTLP exporter to send traces to 127.0.0.1:4317.",
        }))
        .unwrap_or_default(),
    };

    ResourceContent {
        uri: "synapseed://telemetry/hotspots".into(),
        mime_type: Some("application/json".into()),
        text: Some(text),
    }
}

fn resource_janitor_proposals(ctx: &SynapseContext) -> ResourceContent {
    let text = match ctx.get_extension::<ProposalStore>() {
        Some(store) => {
            let pending = store.pending();
            let applied: Vec<_> = store
                .all()
                .into_iter()
                .filter(|p| p.status == synapseed_janitor::ProposalStatus::Applied)
                .collect();

            let mut data = json!({
                "is_scanning": store.is_scanning(),
                "pending_count": pending.len(),
                "applied_count": applied.len(),
                "total_count": store.total_count(),
                "proposals": pending,
            });

            if let Some(last) = store.last_scan() {
                data["last_scan"] = serde_json::to_value(last).unwrap_or_default();
            }

            serde_json::to_string_pretty(&data).unwrap_or_default()
        }
        None => serde_json::to_string_pretty(&json!({
            "status": "inactive",
            "is_scanning": false,
            "message": "Janitor plugin not active. Run `janitor_run_now` to scan for issues.",
        }))
        .unwrap_or_default(),
    };

    ResourceContent {
        uri: "synapseed://janitor/proposals".into(),
        mime_type: Some("application/json".into()),
        text: Some(text),
    }
}

fn resource_architect_health(ctx: &SynapseContext) -> ResourceContent {
    let text = match ctx.get_extension::<ReportStore>() {
        Some(store) => match store.get() {
            Some(report) => {
                let violations_summary: Vec<_> = report
                    .violations
                    .iter()
                    .map(|v| {
                        json!({
                            "rule": v.rule,
                            "severity": format!("{:?}", v.severity),
                            "description": v.description,
                        })
                    })
                    .collect();

                let top_recs: Vec<_> = report
                    .recommendations
                    .iter()
                    .take(3)
                    .map(|r| {
                        json!({
                            "priority": r.priority,
                            "category": r.category,
                            "action": r.action,
                        })
                    })
                    .collect();

                serde_json::to_string_pretty(&json!({
                    "score": report.score,
                    "grade": report.grade,
                    "module_count": report.module_count,
                    "edge_count": report.edge_count,
                    "avg_instability": report.avg_instability,
                    "avg_complexity": report.avg_complexity,
                    "max_coupling": report.max_coupling,
                    "violation_count": report.violations.len(),
                    "violations": violations_summary,
                    "top_recommendations": top_recs,
                }))
                .unwrap_or_default()
            }
            None => serde_json::to_string_pretty(&json!({
                "status": "pending",
                "message": "Architecture analysis in progress. Call `architect_analyze` or wait for background analysis to complete.",
            }))
            .unwrap_or_default(),
        },
        None => serde_json::to_string_pretty(&json!({
            "status": "inactive",
            "message": "Architect plugin not active.",
        }))
        .unwrap_or_default(),
    };

    ResourceContent {
        uri: "synapseed://architect/health".into(),
        mime_type: Some("application/json".into()),
        text: Some(text),
    }
}
