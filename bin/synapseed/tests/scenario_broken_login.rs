//! Scenario: "The Broken Login"
//!
//! End-to-end integration test that creates a Rust project with:
//! - A syntax error in `src/login.rs`
//! - A hardcoded password (`"secret123"`)
//!
//! Then validates all four subsystems via MCP:
//! 1. Shadow Compiler → detects the syntax error
//! 2. Husk DLP → blocks the hardcoded password
//! 3. Chronos → sees the commit in history analysis
//! 4. Visualizer (Cortex) → includes `login` symbol in the code graph

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Create a temporary Cargo project with the broken login file + git repo.
/// Returns the path to the temp directory.
fn setup_broken_login_project() -> PathBuf {
    let temp_dir =
        std::env::temp_dir().join(format!("synapseed-broken-login-{}", std::process::id()));

    // Clean up any previous run
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    // cargo init
    let output = Command::new("cargo")
        .args(["init", "--name", "broken-login-test"])
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to run cargo init");
    assert!(
        output.status.success(),
        "cargo init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Write src/login.rs — syntax error + hardcoded password
    let login_rs = r#"pub fn login() {
    let password = "secret123";
    let x = )
}
"#;
    std::fs::write(temp_dir.join("src/login.rs"), login_rs).expect("Failed to write login.rs");

    // Update src/main.rs to include the login module
    let main_rs = r#"mod login;

fn main() {
    println!("Hello, world!");
}
"#;
    std::fs::write(temp_dir.join("src/main.rs"), main_rs).expect("Failed to write main.rs");

    // Git init + configure + commit
    run_git(&temp_dir, &["init"]);
    run_git(&temp_dir, &["config", "user.email", "test@synapseed.dev"]);
    run_git(&temp_dir, &["config", "user.name", "Scenario Test"]);
    run_git(&temp_dir, &["add", "."]);
    run_git(
        &temp_dir,
        &["commit", "-m", "add broken login with hardcoded password"],
    );

    temp_dir
}

fn run_git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run git {}: {e}", args.join(" ")));
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Spawn `synapseed serve` against a project directory.
/// Sends the handshake, waits for the shadow compiler to finish,
/// then sends tool calls and collects all JSON-RPC responses.
fn run_scenario(project_dir: &Path, tool_messages: &[String]) -> Vec<serde_json::Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_synapseed"))
        .args(["serve", "--project", project_dir.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start synapseed serve");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");

        // Handshake
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2024-11-05","capabilities":{{}},"clientInfo":{{"name":"scenario-test","version":"1.0"}}}}}}"#
        )
        .unwrap();
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#
        )
        .unwrap();
        stdin.flush().unwrap();

        // Wait for shadow compiler to finish its initial `cargo check`
        // (on a tiny project with a parse error, this is sub-second,
        //  but we give it a generous margin)
        std::thread::sleep(Duration::from_secs(10));

        // Send all tool call messages
        for msg in tool_messages {
            writeln!(stdin, "{msg}").unwrap();
        }
        stdin.flush().unwrap();
    }
    // Close stdin → server processes remaining messages then exits
    drop(child.stdin.take());

    // Read all responses
    let stdout = child.stdout.take().expect("Failed to open stdout");
    let reader = BufReader::new(stdout);
    let responses: Vec<serde_json::Value> = reader
        .lines()
        .map_while(Result::ok)
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str(&l).ok())
        .collect();

    let status = child.wait().expect("Failed to wait for child");
    assert!(
        status.success(),
        "synapseed serve exited with error: {status}"
    );

    responses
}

// ── The Scenario ──────────────────────────────────────────────────────

#[test]
fn test_scenario_broken_login() {
    // ── Setup ──
    let project_dir = setup_broken_login_project();

    // Build the tool call messages using serde_json for proper escaping
    let msg_diagnostics = serde_json::to_string(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "get_diagnostics",
            "arguments": {}
        }
    }))
    .unwrap();

    // Action 2: Husk — scan the file content for secrets
    let msg_scan = serde_json::to_string(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "scan_security",
            "arguments": {
                "content": "pub fn login() {\n    let password = \"secret123\";\n    let x = )\n}"
            }
        }
    }))
    .unwrap();

    // Action 3: Chronos — analyze history of login.rs
    let msg_history = serde_json::to_string(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "analyze_history",
            "arguments": {
                "file": "src/login.rs"
            }
        }
    }))
    .unwrap();

    // Action 4: Cortex/Visualizer — lookup the `login` symbol to verify
    // that the file is parsed and present in the code graph
    let msg_lookup = serde_json::to_string(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "lookup_symbol",
            "arguments": {
                "name": "login"
            }
        }
    }))
    .unwrap();

    // ── Run ──
    let responses = run_scenario(
        &project_dir,
        &[msg_diagnostics, msg_scan, msg_history, msg_lookup],
    );

    // ── Cleanup ──
    let _ = std::fs::remove_dir_all(&project_dir);

    // ── Validate ──
    // Responses: initialize(1), get_diagnostics(2), scan_security(3),
    //            analyze_history(4), lookup_symbol(5) = 5 responses
    assert!(
        responses.len() >= 5,
        "Expected at least 5 responses, got {}: {:#?}",
        responses.len(),
        responses
    );

    // ── Action 1: Shadow Compiler — syntax error detected ──
    let diag_resp = responses
        .iter()
        .find(|r| r["id"] == 2)
        .expect("Missing get_diagnostics response (id=2)");
    let diag_text = diag_resp["result"]["content"][0]["text"]
        .as_str()
        .expect("get_diagnostics should return text content");

    // The shadow compiler should have found the syntax error
    // Either we see error count > 0 or specific error text
    let has_errors = diag_text.contains("error")
        || diag_text.contains("Error")
        || diag_text.contains("\"level\":\"error\"");
    assert!(
        has_errors,
        "ACTION 1 FAIL: Shadow compiler should detect syntax error in login.rs.\n\
         Got: {diag_text}"
    );
    println!("ACTION 1 PASS: Shadow detected syntax error");

    // ── Action 2: Husk DLP — hardcoded password blocked ──
    let scan_resp = responses
        .iter()
        .find(|r| r["id"] == 3)
        .expect("Missing scan_security response (id=3)");
    let scan_text = scan_resp["result"]["content"][0]["text"]
        .as_str()
        .expect("scan_security should return text content");

    assert!(
        scan_text.contains("ALERT"),
        "ACTION 2 FAIL: Husk should ALERT on hardcoded password.\nGot: {scan_text}"
    );
    assert!(
        scan_text.contains("REDACTED"),
        "ACTION 2 FAIL: Husk should redact the password.\nGot: {scan_text}"
    );
    // Verify it matched the generic_secret rule
    assert!(
        scan_text.contains("generic_secret") || scan_text.contains("password"),
        "ACTION 2 FAIL: Should match generic_secret or password pattern.\nGot: {scan_text}"
    );
    println!("ACTION 2 PASS: Husk blocked hardcoded password");

    // ── Action 3: Chronos — sees the commit ──
    let history_resp = responses
        .iter()
        .find(|r| r["id"] == 4)
        .expect("Missing analyze_history response (id=4)");
    let history_text = history_resp["result"]["content"][0]["text"]
        .as_str()
        .expect("analyze_history should return text content");

    // Should see at least 1 commit (the one we created)
    assert!(
        history_text.contains("Commits: 1") || history_text.contains("\"total_commits\":1"),
        "ACTION 3 FAIL: Chronos should see exactly 1 commit.\nGot: {history_text}"
    );
    // Should see our commit message
    assert!(
        history_text.contains("broken login") || history_text.contains("hardcoded password"),
        "ACTION 3 FAIL: Chronos should see our commit message.\nGot: {history_text}"
    );
    // Should see the author
    assert!(
        history_text.contains("Scenario Test"),
        "ACTION 3 FAIL: Chronos should identify the author.\nGot: {history_text}"
    );
    println!("ACTION 3 PASS: Chronos sees the commit");

    // ── Action 4: Visualizer/Cortex — login.rs in the graph ──
    let lookup_resp = responses
        .iter()
        .find(|r| r["id"] == 5)
        .expect("Missing lookup_symbol response (id=5)");
    let lookup_text = lookup_resp["result"]["content"][0]["text"]
        .as_str()
        .expect("lookup_symbol should return text content");

    // Tree-sitter is error-tolerant: even with the syntax error,
    // it should parse the `login` function definition
    assert!(
        lookup_text.contains("login"),
        "ACTION 4 FAIL: Cortex should find the `login` symbol.\nGot: {lookup_text}"
    );
    assert!(
        lookup_text.contains("login.rs") || lookup_text.contains("src/login.rs"),
        "ACTION 4 FAIL: Symbol should be located in login.rs.\nGot: {lookup_text}"
    );
    println!("ACTION 4 PASS: login.rs appears in the code graph");

    println!("\n=== SCENARIO \"The Broken Login\" — ALL 4 ACTIONS PASS ===");
}
