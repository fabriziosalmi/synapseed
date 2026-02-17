//! Benchmarks for the cortex code graph: directory indexing and symbol lookup.
//!
//! Runs against the synapseed project root itself, so results reflect
//! real-world AST parsing workload (Rust, Python, JS sources).

use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, Criterion};
use synapseed_cortex::graph::CodeGraph;

/// Resolve the project root (two levels up from this bench file's crate root).
fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // crates/
        .and_then(|p| p.parent()) // synapseed/
        .expect("Could not resolve project root")
        .to_path_buf()
}

/// Benchmark: build a fresh CodeGraph and index the entire synapseed tree.
fn bench_index_directory(c: &mut Criterion) {
    let root = project_root();

    c.bench_function("CodeGraph::index_directory (full project)", |b| {
        b.iter(|| {
            let graph = CodeGraph::new();
            graph
                .index_directory(&root)
                .expect("index_directory should succeed");
            // Return file count to prevent optimisation from eliding the work.
            graph.file_count()
        });
    });
}

/// Benchmark: lookup a known symbol name against an already-indexed graph.
fn bench_lookup(c: &mut Criterion) {
    let root = project_root();

    // Pre-build the graph once (outside the measurement loop).
    let graph = CodeGraph::new();
    graph
        .index_directory(&root)
        .expect("index_directory should succeed");

    // Sanity: make sure there is something to find.
    assert!(
        graph.symbol_count() > 0,
        "Graph should contain symbols after indexing"
    );

    c.bench_function("CodeGraph::lookup('CodeGraph')", |b| {
        b.iter(|| {
            let results = graph.lookup("CodeGraph");
            criterion::black_box(results)
        });
    });
}

/// Benchmark: CodeGraph::new() construction cost (baseline).
fn bench_new(c: &mut Criterion) {
    c.bench_function("CodeGraph::new()", |b| {
        b.iter(|| criterion::black_box(CodeGraph::new()));
    });
}

criterion_group!(benches, bench_new, bench_index_directory, bench_lookup);
criterion_main!(benches);
