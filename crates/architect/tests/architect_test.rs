use synapseed_architect::DependencyGraph;
use synapseed_architect::blueprint::{self, ReportStore};
use synapseed_architect::linter::{self, LinterConfig};

// ── Graph building on real project ──────────────────────────

#[test]
fn test_build_graph_on_synapseed() {
    let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let graph = synapseed_cortex::graph::CodeGraph::new();
    graph.index_directory(project_root).unwrap();

    let mut dep_graph = DependencyGraph::build(&graph);
    dep_graph.compute_metrics();

    // Synapseed has many modules.
    assert!(
        dep_graph.module_count() > 5,
        "Expected >5 modules, got {}",
        dep_graph.module_count()
    );

    // Should have some internal dependency edges.
    assert!(
        dep_graph.edge_count() > 0,
        "Expected some dependency edges, got 0"
    );

    // Metrics count matches node_map size (deduplicated modules).
    assert!(!dep_graph.all_metrics().is_empty());
}

// ── Score calculation ────────────────────────────────────────

#[test]
fn test_score_clean_project() {
    let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let graph = synapseed_cortex::graph::CodeGraph::new();
    graph.index_directory(project_root).unwrap();

    let mut dep_graph = DependencyGraph::build(&graph);
    dep_graph.compute_metrics();

    let config = LinterConfig::default();
    let violations = linter::lint(&dep_graph, &config);
    let report = blueprint::generate_report(&dep_graph, violations);

    // Synapseed should have a reasonable architecture score.
    assert!(
        report.score >= 50,
        "Expected score >= 50 for synapseed, got {}",
        report.score
    );
    assert!(!report.grade.is_empty());
    assert!(!report.modules.is_empty());
}

// ── ReportStore thread safety ────────────────────────────────

#[test]
fn test_report_store() {
    let store = ReportStore::new();

    // Initially empty.
    assert!(store.get().is_none());
    assert!(store.health().is_none());

    // Set a report.
    let report = blueprint::ArchitectureReport {
        score: 85,
        grade: "B".into(),
        module_count: 10,
        edge_count: 15,
        avg_instability: 0.45,
        avg_complexity: 5.0,
        max_coupling: 3,
        modules: vec![],
        violations: vec![],
        recommendations: vec![],
    };

    store.set(report);

    let retrieved = store.get().unwrap();
    assert_eq!(retrieved.score, 85);
    assert_eq!(retrieved.grade, "B");

    let (score, violations) = store.health().unwrap();
    assert_eq!(score, 85);
    assert_eq!(violations, 0);
}

// ── Latency benchmark ─────────────────────────────────────────

#[test]
fn test_analysis_latency_under_500ms() {
    let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let graph = synapseed_cortex::graph::CodeGraph::new();
    graph.index_directory(project_root).unwrap();

    let start = std::time::Instant::now();

    let mut dep_graph = DependencyGraph::build(&graph);
    dep_graph.compute_metrics();
    let config = LinterConfig::default();
    let violations = linter::lint(&dep_graph, &config);
    let _report = blueprint::generate_report(&dep_graph, violations);

    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 500,
        "Full analysis took {}ms — must be <500ms",
        elapsed.as_millis()
    );
}

// ── Score determinism ─────────────────────────────────────────

#[test]
fn test_score_determinism() {
    let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let graph = synapseed_cortex::graph::CodeGraph::new();
    graph.index_directory(project_root).unwrap();

    // Run twice — scores must be bit-identical.
    let mut dep1 = DependencyGraph::build(&graph);
    dep1.compute_metrics();
    let config = LinterConfig::default();
    let v1 = linter::lint(&dep1, &config);
    let r1 = blueprint::generate_report(&dep1, v1);

    let mut dep2 = DependencyGraph::build(&graph);
    dep2.compute_metrics();
    let v2 = linter::lint(&dep2, &config);
    let r2 = blueprint::generate_report(&dep2, v2);

    assert_eq!(r1.score, r2.score, "Score must be deterministic");
    assert_eq!(r1.grade, r2.grade, "Grade must be deterministic");
    assert_eq!(
        r1.module_count, r2.module_count,
        "Module count must be deterministic"
    );
    assert_eq!(
        r1.edge_count, r2.edge_count,
        "Edge count must be deterministic"
    );

    // Metrics order must be identical (sorted by module_name).
    let names1: Vec<&str> = r1.modules.iter().map(|m| m.module_name.as_str()).collect();
    let names2: Vec<&str> = r2.modules.iter().map(|m| m.module_name.as_str()).collect();
    assert_eq!(names1, names2, "Module order must be deterministic");
}
