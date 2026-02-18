//! MCP Resource definitions and handlers.
//!
//! Resources expose read-only data that the LLM can inspect.

use serde_json::json;

use parking_lot::Mutex;

use synapseed_architect::ReportStore;
use synapseed_core::context::SynapseContext;
use synapseed_core::ledger::MomentClassifier;
use synapseed_core::pulse::PulseStore;
use synapseed_core::recorder::FlightRecorder;
use synapseed_core::state::ProjectState;
use synapseed_janitor::ProposalStore;
use synapseed_root::sentinel::Sentinel;
use synapseed_shadow_check::runner::DiagnosticStore;
use synapseed_telemetry_sink::store::SpanStore;
use synapseed_whisper::router::metrics::PipelineAggregator;

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
        ResourceDefinition {
            uri: "synapseed://consistency".into(),
            name: "Consistency Oracle".into(),
            description: Some(
                "Cross-references project artifacts (README, Cargo.toml, docs, crate directories) to detect drift and inconsistencies. Returns a consistency score and actionable fixes."
                    .into(),
            ),
            mime_type: Some("application/json".into()),
        },
        ResourceDefinition {
            uri: "synapseed://session/recorder".into(),
            name: "Session Flight Recorder".into(),
            description: Some(
                "Dual-track session memory: Working Set (last 20 events) + Journey Map (compressed context shifts). \
                 Shows current activity, module focus, loop detection, and causal chains. Use to understand what the developer \
                 has been doing and where they are headed."
                    .into(),
            ),
            mime_type: Some("application/json".into()),
        },
        ResourceDefinition {
            uri: "synapseed://session/context".into(),
            name: "Cognitive Ledger — Session Pulse".into(),
            description: Some(
                "Deterministic Operational Moment classification: infers the developer's current cognitive state \
                 (e.g., Rapid Scaffolding, Deep Backend Logic, Iterative Distress) from Flight Recorder metrics. \
                 Returns a SessionPulse with needle range, mode suggestion, and auditable evidence. \
                 Use to understand the session's cognitive rhythm and adapt behavior accordingly."
                    .into(),
            ),
            mime_type: Some("application/json".into()),
        },
        ResourceDefinition {
            uri: "synapseed://context/active".into(),
            name: "Active Context Briefing".into(),
            description: Some(
                "PRIORITY RESOURCE — Dynamic project briefing. Preload this for immediate situational awareness: \
                 project state, active diagnostics summary, recent commit intent, architecture health, and \
                 tool routing recommendations. Reading this resource eliminates the need for multiple initial tool calls."
                    .into(),
            ),
            mime_type: Some("application/json".into()),
        },
        ResourceDefinition {
            uri: "synapseed://pulse".into(),
            name: "Activity Pulse".into(),
            description: Some(
                "Exponential-decay activity counters. Shows which files and symbols have been \
                 most frequently accessed in the current session, weighted by recency (8-min half-life). \
                 Use for understanding the user's current working set."
                    .into(),
            ),
            mime_type: Some("application/json".into()),
        },
        ResourceDefinition {
            uri: "synapseed://pipeline/metrics".into(),
            name: "Pipeline Performance Metrics".into(),
            description: Some(
                "Per-stage timing breakdown for the ask pipeline. Shows microsecond-precision wall-clock \
                 times for each of the 10 pipeline stages (momentum, classify, extract, prune, coherence, \
                 gather, raw_inject, session, context, finalize). Includes rolling averages, P95, bottleneck \
                 identification, and target/token counts. Use to identify performance regressions and optimize \
                 the slowest stages."
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
        "synapseed://telemetry/hotspots" => Some(resource_telemetry_hotspots(ctx)),
        "synapseed://janitor/proposals" => Some(resource_janitor_proposals(ctx)),
        "synapseed://architect/health" => Some(resource_architect_health(ctx)),
        "synapseed://consistency" => Some(resource_consistency(ctx)),
        "synapseed://session/recorder" => Some(resource_flight_recorder(ctx)),
        "synapseed://session/context" => Some(resource_session_pulse(ctx)),
        "synapseed://context/active" => Some(resource_context_active(ctx)),
        "synapseed://pulse" => Some(resource_pulse(ctx)),
        "synapseed://pipeline/metrics" => Some(resource_pipeline_metrics(ctx)),
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
                    "topological_density": report.topological_density,
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

fn resource_consistency(ctx: &SynapseContext) -> ResourceContent {
    let root = ctx.project_root();
    let report = synapseed_core::oracle::check_consistency(&root);

    let text = serde_json::to_string_pretty(&json!({
        "score": report.score,
        "total_checks": report.total_checks,
        "inconsistency_count": report.inconsistencies.len(),
        "inconsistencies": report.inconsistencies,
    }))
    .unwrap_or_default();

    ResourceContent {
        uri: "synapseed://consistency".into(),
        mime_type: Some("application/json".into()),
        text: Some(text),
    }
}

fn resource_flight_recorder(ctx: &SynapseContext) -> ResourceContent {
    let text = ctx
        .get_extension::<Mutex<FlightRecorder>>()
        .map(|rec| {
            let recorder = rec.lock();
            serde_json::to_string_pretty(&recorder.to_json()).unwrap_or_default()
        })
        .unwrap_or_else(|| {
            serde_json::to_string_pretty(&serde_json::json!({
                "status": "inactive",
                "message": "Flight Recorder not initialized. Available in MCP serve mode."
            }))
            .unwrap_or_default()
        });

    ResourceContent {
        uri: "synapseed://session/recorder".into(),
        mime_type: Some("application/json".into()),
        text: Some(text),
    }
}

/// Cognitive Ledger: deterministic SessionPulse from FlightRecorder metrics.
fn resource_session_pulse(ctx: &SynapseContext) -> ResourceContent {
    let text = ctx
        .get_extension::<Mutex<FlightRecorder>>()
        .map(|rec| {
            let recorder = rec.lock();
            // Get diagnostic counts (D25: include previous counts for trend)
            let (errs, warns, prev_errs, prev_warns) = ctx
                .get_extension::<DiagnosticStore>()
                .map(|store| {
                    let snap = store.snapshot();
                    (
                        snap.error_count as u32,
                        snap.warning_count as u32,
                        snap.prev_error_count as u32,
                        snap.prev_warning_count as u32,
                    )
                })
                .unwrap_or((0, 0, 0, 0));
            let snap = recorder.build_metrics_snapshot(errs, warns, prev_errs, prev_warns);
            let pulse = MomentClassifier::classify(&snap);
            serde_json::to_string_pretty(&serde_json::json!({
                "moment": pulse.moment,
                "label": pulse.moment.label(),
                "needle_range": pulse.needle_range,
                "mode": pulse.mode,
                "layer": pulse.layer,
                "session_weight_pct": pulse.session_weight_pct,
                "focus_module": pulse.focus_module,
                "session_hint": pulse.session_hint,
                "evidence": pulse.evidence,
            }))
            .unwrap_or_default()
        })
        .unwrap_or_else(|| {
            serde_json::to_string_pretty(&serde_json::json!({
                "status": "inactive",
                "message": "Cognitive Ledger requires the Flight Recorder. Available in MCP serve mode."
            }))
            .unwrap_or_default()
        });

    ResourceContent {
        uri: "synapseed://session/context".into(),
        mime_type: Some("application/json".into()),
        text: Some(text),
    }
}

/// Dynamic project briefing — aggregates key signals into a single resource.
///
/// This is the "Passive Interception" strategy: clients that preload this
/// resource give the LLM situational awareness before any tool call,
/// reducing roundtrips and improving routing accuracy.
fn resource_context_active(ctx: &SynapseContext) -> ResourceContent {
    let state = ctx.project_state();
    let metrics = ctx.metrics();
    let dna = ctx.dna();

    // Diagnostics summary (D25: include previous counts for trend)
    let (error_count, warning_count, prev_error_count, prev_warning_count) = ctx
        .get_extension::<DiagnosticStore>()
        .map(|store| {
            let snap = store.snapshot();
            (
                snap.error_count,
                snap.warning_count,
                snap.prev_error_count,
                snap.prev_warning_count,
            )
        })
        .unwrap_or((0, 0, 0, 0));

    // Architecture health (cached)
    let arch_score = ctx
        .get_extension::<ReportStore>()
        .and_then(|store| store.get())
        .map(|r| r.grade.clone())
        .unwrap_or_else(|| "unknown".into());

    // Session continuity
    let root = ctx.project_root();
    let session_info = synapseed_core::session::SessionState::load(&root)
        .filter(|s| s.is_recent())
        .map(|s| {
            json!({
                "time_ago": s.time_ago(),
                "files_indexed": s.files_indexed,
                "tools_invoked": s.tools_invoked,
            })
        });

    let state_label = match &state {
        ProjectState::VirginRepo => "virgin_repo",
        ProjectState::PartialSetup { .. } => "partial_setup",
        ProjectState::HealthyWorkspace { .. } => "healthy_workspace",
        ProjectState::Unknown => "unknown",
    };

    let text = serde_json::to_string_pretty(&json!({
        "project_state": state_label,
        "state_detail": state,
        "dna": {
            "strategy": dna.workspace_strategy,
            "dlp_level": format!("{:?}", dna.dlp_level),
            "plugins": dna.plugins,
        },
        "diagnostics": {
            "errors": error_count,
            "warnings": warning_count,
        },
        "architecture_grade": arch_score,
        "metrics": {
            "files_indexed": metrics.files_indexed,
            "symbols_found": metrics.symbols_found,
        },
        "session": session_info,
        "routing_hint": "For ANY code question, call the `ask` tool FIRST. It orchestrates all subsystems automatically.",
        "flight_recorder": ctx.get_extension::<Mutex<FlightRecorder>>().map(|rec| {
            let r = rec.lock();
            let snap = r.build_metrics_snapshot(error_count as u32, warning_count as u32, prev_error_count as u32, prev_warning_count as u32);
            let pulse = MomentClassifier::classify(&snap);
            serde_json::json!({
                "total_events": r.total_events(),
                "phase_count": r.journey().len(),
                "current_phase": r.current_phase(),
                "loop_alert": r.detect_loop(),
                "cognitive_ledger": {
                    "moment": pulse.moment.label(),
                    "needle_range": pulse.needle_range,
                    "mode": format!("{}", pulse.mode),
                    "layer": pulse.layer,
                    "session_hint": pulse.session_hint,
                },
            })
        }),
    }))
    .unwrap_or_default();

    ResourceContent {
        uri: "synapseed://context/active".into(),
        mime_type: Some("application/json".into()),
        text: Some(text),
    }
}

fn resource_pulse(ctx: &SynapseContext) -> ResourceContent {
    let text = ctx
        .get_extension::<PulseStore>()
        .map(|store| {
            let snap = store.snapshot();
            let counters: Vec<serde_json::Value> = snap
                .counters
                .iter()
                .map(|c| {
                    let entries: Vec<serde_json::Value> = c
                        .top
                        .iter()
                        .map(|e| {
                            json!({
                                "value": e.value,
                                "score": (e.score * 1000.0).round() / 1000.0,
                                "raw_count": e.raw_count,
                            })
                        })
                        .collect();
                    json!({
                        "counter": c.name,
                        "total_events": c.total_events,
                        "top": entries,
                    })
                })
                .collect();

            serde_json::to_string_pretty(&json!({
                "half_life_secs": snap.half_life_secs,
                "total_events": snap.total_events,
                "counters": counters,
            }))
            .unwrap_or_default()
        })
        .unwrap_or_else(|| r#"{"error": "PulseStore not available"}"#.to_string());

    ResourceContent {
        uri: "synapseed://pulse".into(),
        mime_type: Some("application/json".into()),
        text: Some(text),
    }
}

fn resource_pipeline_metrics(ctx: &SynapseContext) -> ResourceContent {
    let text = ctx
        .get_extension::<PipelineAggregator>()
        .map(|agg| {
            let stats = agg.aggregate();
            let last = agg.last();

            let last_json = last.as_ref().map(|m| {
                json!({
                    "total_ms": (m.total_us as f64 / 1000.0 * 10.0).round() / 10.0,
                    "bottleneck": m.bottleneck(),
                    "targets_before_prune": m.targets_before_prune,
                    "targets_after_prune": m.targets_after_prune,
                    "context_tokens": m.context_tokens,
                    "context_bytes": m.context_bytes,
                    "stages": {
                        "momentum_ms": (m.momentum_us as f64 / 1000.0 * 10.0).round() / 10.0,
                        "classify_ms": (m.classify_us as f64 / 1000.0 * 10.0).round() / 10.0,
                        "extract_ms": (m.extract_us as f64 / 1000.0 * 10.0).round() / 10.0,
                        "prune_ms": (m.prune_us as f64 / 1000.0 * 10.0).round() / 10.0,
                        "coherence_ms": (m.coherence_us as f64 / 1000.0 * 10.0).round() / 10.0,
                        "gather_ms": (m.gather_us as f64 / 1000.0 * 10.0).round() / 10.0,
                        "raw_inject_ms": (m.raw_inject_us as f64 / 1000.0 * 10.0).round() / 10.0,
                        "session_ms": (m.session_us as f64 / 1000.0 * 10.0).round() / 10.0,
                        "context_ms": (m.context_us as f64 / 1000.0 * 10.0).round() / 10.0,
                        "finalize_ms": (m.finalize_us as f64 / 1000.0 * 10.0).round() / 10.0,
                    },
                    "breakdown": m.summary(),
                })
            });

            serde_json::to_string_pretty(&json!({
                "total_queries": stats.total_queries,
                "rolling_window": stats.window_size,
                "aggregate": {
                    "avg_total_ms": (stats.avg_total_us / 1000.0 * 10.0).round() / 10.0,
                    "min_total_ms": (stats.min_total_us as f64 / 1000.0 * 10.0).round() / 10.0,
                    "max_total_ms": (stats.max_total_us as f64 / 1000.0 * 10.0).round() / 10.0,
                    "p95_total_ms": (stats.p95_total_us as f64 / 1000.0 * 10.0).round() / 10.0,
                    "avg_stages_ms": {
                        "momentum": (stats.avg_momentum_us / 1000.0 * 10.0).round() / 10.0,
                        "classify": (stats.avg_classify_us / 1000.0 * 10.0).round() / 10.0,
                        "extract": (stats.avg_extract_us / 1000.0 * 10.0).round() / 10.0,
                        "prune": (stats.avg_prune_us / 1000.0 * 10.0).round() / 10.0,
                        "coherence": (stats.avg_coherence_us / 1000.0 * 10.0).round() / 10.0,
                        "gather": (stats.avg_gather_us / 1000.0 * 10.0).round() / 10.0,
                        "raw_inject": (stats.avg_raw_inject_us / 1000.0 * 10.0).round() / 10.0,
                        "session": (stats.avg_session_us / 1000.0 * 10.0).round() / 10.0,
                        "context": (stats.avg_context_us / 1000.0 * 10.0).round() / 10.0,
                        "finalize": (stats.avg_finalize_us / 1000.0 * 10.0).round() / 10.0,
                    },
                    "avg_context_tokens": stats.avg_context_tokens.round(),
                },
                "last_query": last_json,
            }))
            .unwrap_or_default()
        })
        .unwrap_or_else(|| r#"{"error": "PipelineAggregator not available"}"#.to_string());

    ResourceContent {
        uri: "synapseed://pipeline/metrics".into(),
        mime_type: Some("application/json".into()),
        text: Some(text),
    }
}
