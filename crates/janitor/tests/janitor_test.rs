use std::sync::Arc;

use synapseed_janitor::proposal::{Proposal, ProposalCategory, ProposalStatus, ProposalStore};
use synapseed_janitor::Janitor;

// ── ProposalStore tests ─────────────────────────────────────

#[test]
fn test_proposal_store_crud() {
    let store = ProposalStore::new();

    // Add a proposal
    let proposal = Proposal::new(
        ProposalCategory::Clippy,
        "clippy::needless_return",
        "src/lib.rs",
        10,
        10,
        "unneeded `return` statement",
        "return x + 1;",
        "x + 1",
    );
    let id = proposal.id.clone();
    store.add(proposal);

    // Get by ID
    let found = store.get(&id).unwrap();
    assert_eq!(found.lint_code, "clippy::needless_return");
    assert_eq!(found.status, ProposalStatus::Pending);

    // Pending count
    assert_eq!(store.pending_count(), 1);
    assert_eq!(store.total_count(), 1);

    // Mark applied
    assert!(store.mark_applied(&id));
    let updated = store.get(&id).unwrap();
    assert_eq!(updated.status, ProposalStatus::Applied);
    assert_eq!(store.pending_count(), 0);

    // Mark rejected (on non-existent)
    assert!(!store.mark_rejected("nonexistent"));
}

#[test]
fn test_proposal_store_multiple_proposals() {
    let store = ProposalStore::new();

    for i in 0..5 {
        store.add(Proposal::new(
            ProposalCategory::Clippy,
            &format!("clippy::lint_{i}"),
            "src/lib.rs",
            i * 10,
            i * 10 + 5,
            &format!("Issue {i}"),
            "old",
            "new",
        ));
    }

    assert_eq!(store.total_count(), 5);
    assert_eq!(store.pending_count(), 5);
    assert_eq!(store.pending().len(), 5);

    // Clear
    store.clear();
    assert_eq!(store.total_count(), 0);
}

#[test]
fn test_proposal_categories_serialize() {
    let proposal = Proposal::new(
        ProposalCategory::UnusedDependency,
        "unused_dependency",
        "Cargo.toml",
        0,
        0,
        "Dependency `foo` appears unused",
        "foo = \"1.0\"",
        "# foo removed",
    );

    let json = serde_json::to_string(&proposal).unwrap();
    assert!(json.contains("\"unused_dependency\""));
    assert!(json.contains("\"pending\""));

    // Roundtrip
    let deserialized: Proposal = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.category, ProposalCategory::UnusedDependency);
    assert_eq!(deserialized.status, ProposalStatus::Pending);
}

// ── Scanner tests ───────────────────────────────────────────

#[test]
fn test_scan_clippy_on_clean_project() {
    // Scan the synapseed project itself — it should have zero or few clippy issues
    // since we maintain clean code. This test verifies the scanner doesn't crash.
    let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let result = synapseed_janitor::scanner::scan_clippy(project_root);
    // Should not error (clippy is installed)
    assert!(
        result.is_ok(),
        "scan_clippy should succeed: {:?}",
        result.err()
    );
}

#[test]
fn test_scan_clippy_with_known_issues() {
    // Create a temp project with a known clippy warning
    let dir = tempfile::TempDir::new().unwrap();
    let project_path = dir.path().join("clippy_test");

    // cargo init
    let output = std::process::Command::new("cargo")
        .args(["init", "--lib", "--name", "clippy_test"])
        .arg(&project_path)
        .output()
        .unwrap();
    assert!(output.status.success(), "cargo init failed");

    // Write code with a known clippy warning (needless_return)
    std::fs::write(
        project_path.join("src/lib.rs"),
        r#"
pub fn add(a: i32, b: i32) -> i32 {
    return a + b;
}
"#,
    )
    .unwrap();

    let issues = synapseed_janitor::scanner::scan_clippy(&project_path).unwrap();

    // Should find at least one clippy issue (needless_return)
    assert!(
        !issues.is_empty(),
        "Expected at least one clippy issue for `return a + b;`"
    );

    // The first issue should be about needless_return
    let return_issue = issues
        .iter()
        .find(|i| i.lint_code.contains("needless_return"));
    assert!(
        return_issue.is_some(),
        "Expected clippy::needless_return, got: {:?}",
        issues.iter().map(|i| &i.lint_code).collect::<Vec<_>>()
    );

    // Should have a MachineApplicable suggestion
    let issue = return_issue.unwrap();
    assert!(
        issue.has_auto_fix(),
        "needless_return should be auto-fixable"
    );
}

// ── Janitor integration test ────────────────────────────────

#[test]
fn test_janitor_scan_and_proposals() {
    let dir = tempfile::TempDir::new().unwrap();
    let project_path = dir.path().join("janitor_test");

    // cargo init
    let output = std::process::Command::new("cargo")
        .args(["init", "--lib", "--name", "janitor_test"])
        .arg(&project_path)
        .output()
        .unwrap();
    assert!(output.status.success());

    // Write code with clippy issues
    std::fs::write(
        project_path.join("src/lib.rs"),
        r#"
pub fn compute(x: Option<i32>) -> i32 {
    return match x {
        Some(v) => v * 2,
        None => 0,
    };
}
"#,
    )
    .unwrap();

    let store = Arc::new(ProposalStore::new());
    let janitor = Janitor::new(store.clone());

    let result = janitor.scan(&project_path).unwrap();

    // Should find clippy issues
    assert!(result.clippy_issues > 0, "Expected clippy issues");

    // Should have proposals for fixable issues
    if result.fixable_issues > 0 {
        assert!(
            result.proposals_created > 0,
            "Fixable issues should produce proposals"
        );
        assert!(store.pending_count() > 0);
    }
}

// ── Unused deps detection ───────────────────────────────────

#[test]
fn test_scan_unused_deps() {
    let dir = tempfile::TempDir::new().unwrap();
    let project_path = dir.path().join("deps_test");

    // cargo init
    let output = std::process::Command::new("cargo")
        .args(["init", "--lib", "--name", "deps_test"])
        .arg(&project_path)
        .output()
        .unwrap();
    assert!(output.status.success());

    // Add a dependency to Cargo.toml but don't use it
    let cargo_toml = project_path.join("Cargo.toml");
    let mut content = std::fs::read_to_string(&cargo_toml).unwrap();
    content.push_str("\n[dependencies]\nserde = \"1\"\n");
    std::fs::write(&cargo_toml, content).unwrap();

    // Source code doesn't use serde
    std::fs::write(
        project_path.join("src/lib.rs"),
        "pub fn hello() -> &'static str { \"hello\" }\n",
    )
    .unwrap();

    let unused = synapseed_janitor::scanner::scan_unused_deps(&project_path).unwrap();
    assert!(
        unused.contains(&"serde".to_string()),
        "serde should be detected as unused, got: {:?}",
        unused
    );
}
