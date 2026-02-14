//! Integration tests for the MCP server.
//!
//! Spawns the actual `synapseed serve` binary and communicates via
//! JSON-RPC 2.0 over stdin/stdout. Validates protocol correctness,
//! tool execution, and error handling.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

/// Spawn the synapseed serve binary and send a sequence of JSON-RPC messages.
/// Returns all JSON response lines parsed as serde_json::Value.
fn run_mcp_session(messages: &[&str]) -> Vec<serde_json::Value> {
    // Get the workspace root from CARGO_MANIFEST_DIR (bin/synapseed/) -> ../..
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_synapseed"))
        .args(["serve", "--project", workspace_root.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null()) // suppress tracing logs
        .spawn()
        .expect("Failed to start synapseed serve");

    // Write all messages to stdin
    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        for msg in messages {
            writeln!(stdin, "{msg}").expect("Failed to write to stdin");
        }
        stdin.flush().expect("Failed to flush stdin");
    }
    // Close stdin to signal EOF — server will process all messages then exit
    drop(child.stdin.take());

    // Read all responses from stdout
    let stdout = child.stdout.take().expect("Failed to open stdout");
    let reader = BufReader::new(stdout);
    let responses: Vec<serde_json::Value> = reader
        .lines()
        .filter_map(|l| l.ok())
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

/// Helper: build the standard initialize + initialized handshake.
fn handshake() -> [&'static str; 2] {
    [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"integration-test","version":"1.0"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    ]
}

// ── Test: Full MCP Lifecycle ─────────────────────────────────────────

#[test]
fn test_mcp_full_lifecycle() {
    let hs = handshake();
    let responses = run_mcp_session(&[
        hs[0],
        hs[1],
        // tools/list
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
        // tools/call: check_command "ls" → ALLOWED
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"check_command","arguments":{"command":"ls"}}}"#,
        // tools/call: scan_security clean text → CLEAN
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"scan_security","arguments":{"content":"perfectly safe text"}}}"#,
        // tools/call: check_command dangerous → DENIED
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"check_command","arguments":{"command":"rm -rf /"}}}"#,
        // tools/call: consult_architect
        r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"consult_architect","arguments":{"query":"which async runtime?"}}}"#,
        // resources/list
        r#"{"jsonrpc":"2.0","id":7,"method":"resources/list","params":{}}"#,
        // prompts/list
        r#"{"jsonrpc":"2.0","id":8,"method":"prompts/list","params":{}}"#,
        // ping
        r#"{"jsonrpc":"2.0","id":9,"method":"ping","params":{}}"#,
    ]);

    // Notifications produce no response, so:
    // Requests: initialize(1), tools/list(2), tools/call(3,4,5,6), resources/list(7), prompts/list(8), ping(9)
    // = 9 requests → 9 responses
    assert!(
        responses.len() >= 9,
        "Expected at least 9 responses, got {}",
        responses.len()
    );

    // ── 1. Initialize ──
    let init = &responses[0];
    assert_eq!(init["id"], 1);
    assert!(init["result"]["protocolVersion"].is_string());
    assert_eq!(init["result"]["serverInfo"]["name"], "synapseed");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    assert!(init["result"]["capabilities"]["resources"].is_object());
    assert!(init["result"]["capabilities"]["prompts"].is_object());
    // Dynamic context injection present
    assert!(init["result"]["instructions"].is_string());

    // ── 2. tools/list — 10 tools ──
    let tools_list = &responses[1];
    assert_eq!(tools_list["id"], 2);
    let tools = tools_list["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 24, "Expected 24 tools, got {}", tools.len());

    // Verify all tool names are present (short canonical names since v3.1)
    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(tool_names.contains(&"hoist"));
    assert!(tool_names.contains(&"lookup"));
    assert!(tool_names.contains(&"scan"));
    assert!(tool_names.contains(&"check"));
    assert!(tool_names.contains(&"blame"));
    assert!(tool_names.contains(&"diagnose"));
    assert!(tool_names.contains(&"consult"));
    assert!(tool_names.contains(&"search"));
    assert!(tool_names.contains(&"diagnostics"));
    assert!(tool_names.contains(&"analyze"));
    assert!(tool_names.contains(&"quickfix"));
    assert!(tool_names.contains(&"ask"));
    assert!(tool_names.contains(&"intent"));
    assert!(tool_names.contains(&"train"));
    assert!(tool_names.contains(&"reset-telemetry"));
    assert!(tool_names.contains(&"janitor"));
    assert!(tool_names.contains(&"janitor-fix"));
    assert!(tool_names.contains(&"architect"));
    assert!(tool_names.contains(&"oracle"));
    assert!(tool_names.contains(&"similar"));
    assert!(tool_names.contains(&"verify_path"));

    // ── 3. check_command "ls" → ALLOWED ──
    let check_ls = &responses[2];
    assert_eq!(check_ls["id"], 3);
    let text = check_ls["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("ALLOWED"),
        "Expected ALLOWED for 'ls', got: {text}"
    );

    // ── 4. scan_security clean → CLEAN ──
    let scan_clean = &responses[3];
    assert_eq!(scan_clean["id"], 4);
    let text = scan_clean["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("CLEAN"),
        "Expected CLEAN for safe text, got: {text}"
    );

    // ── 5. check_command "rm -rf /" → DENIED ──
    let check_rm = &responses[4];
    assert_eq!(check_rm["id"], 5);
    let text = check_rm["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("DENIED"),
        "Expected DENIED for 'rm -rf /', got: {text}"
    );

    // ── 6. consult_architect → policy response ──
    let architect = &responses[5];
    assert_eq!(architect["id"], 6);
    let text = architect["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("ARCHITECTURE POLICY"),
        "Expected architecture policy, got: {text}"
    );
    assert!(text.contains("tokio"), "Expected tokio in preferred libs");
    assert!(
        text.contains("monorepo"),
        "Expected monorepo workspace strategy"
    );

    // ── 7. resources/list — 3 resources ──
    let res_list = &responses[6];
    assert_eq!(res_list["id"], 7);
    let resources = res_list["result"]["resources"].as_array().unwrap();
    assert_eq!(
        resources.len(),
        11,
        "Expected 11 resources, got {}",
        resources.len()
    );

    // ── 8. prompts/list — 4 prompts ──
    let prompts_list = &responses[7];
    assert_eq!(prompts_list["id"], 8);
    let prompts = prompts_list["result"]["prompts"].as_array().unwrap();
    assert_eq!(
        prompts.len(),
        6,
        "Expected 6 prompts, got {}",
        prompts.len()
    );

    // ── 9. ping ──
    let ping = &responses[8];
    assert_eq!(ping["id"], 9);
    assert!(ping["result"].is_object());
    assert!(ping["error"].is_null());
}

// ── Test: DLP Detection ──────────────────────────────────────────────

#[test]
fn test_mcp_dlp_detection() {
    let hs = handshake();
    let responses = run_mcp_session(&[
        hs[0],
        hs[1],
        // Scan text with AWS key
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"scan_security","arguments":{"content":"aws_key=AKIAIOSFODNN7EXAMPLE"}}}"#,
    ]);

    assert!(responses.len() >= 2);
    let scan = &responses[1];
    assert_eq!(scan["id"], 2);
    let text = scan["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("ALERT"),
        "Expected ALERT for AWS key, got: {text}"
    );
    assert!(
        text.contains("REDACTED"),
        "Expected REDACTED in sanitized output, got: {text}"
    );
}

// ── Test: Error Handling — Unknown Tool ──────────────────────────────

#[test]
fn test_mcp_unknown_tool() {
    let hs = handshake();
    let responses = run_mcp_session(&[
        hs[0],
        hs[1],
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"nonexistent_tool","arguments":{}}}"#,
    ]);

    assert!(responses.len() >= 2);
    let err = &responses[1];
    assert_eq!(err["id"], 2);
    assert_eq!(err["result"]["isError"], true);
    let text = err["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("Unknown tool"),
        "Expected 'Unknown tool' error, got: {text}"
    );
}

// ── Test: Error Handling — Unknown Method ────────────────────────────

#[test]
fn test_mcp_unknown_method() {
    let hs = handshake();
    let responses = run_mcp_session(&[
        hs[0],
        hs[1],
        r#"{"jsonrpc":"2.0","id":2,"method":"nonexistent/method","params":{}}"#,
    ]);

    assert!(responses.len() >= 2);
    let err = &responses[1];
    assert_eq!(err["id"], 2);
    assert!(err["error"].is_object(), "Expected JSON-RPC error object");
    assert_eq!(err["error"]["code"], -32601); // METHOD_NOT_FOUND
}

// ── Test: Pre-initialization Rejection ───────────────────────────────

#[test]
fn test_mcp_reject_before_init() {
    // Send tools/list WITHOUT initialize first
    let responses =
        run_mcp_session(&[r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#]);

    assert_eq!(responses.len(), 1);
    let err = &responses[0];
    assert_eq!(err["id"], 1);
    assert!(
        err["error"].is_object(),
        "Expected error for pre-init request"
    );
    assert_eq!(err["error"]["code"], -32600); // INVALID_REQUEST
    assert!(err["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not initialized"),);
}

// ── Test: Resource Read ──────────────────────────────────────────────

#[test]
fn test_mcp_resource_read_dna() {
    let hs = handshake();
    let responses = run_mcp_session(&[
        hs[0],
        hs[1],
        r#"{"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"synapseed://dna"}}"#,
    ]);

    assert!(responses.len() >= 2);
    let res = &responses[1];
    assert_eq!(res["id"], 2);

    let contents = res["result"]["contents"].as_array().unwrap();
    assert_eq!(contents.len(), 1);
    assert_eq!(contents[0]["uri"], "synapseed://dna");

    let text = contents[0]["text"].as_str().unwrap();
    let dna: serde_json::Value = serde_json::from_str(text).expect("DNA should be valid JSON");
    assert_eq!(dna["workspace_strategy"], "monorepo");
    assert!(dna["plugins"].as_array().unwrap().len() >= 4);
}

// ── Test: Prompt Expansion ───────────────────────────────────────────

#[test]
fn test_mcp_prompt_get() {
    let hs = handshake();
    let responses = run_mcp_session(&[
        hs[0],
        hs[1],
        r#"{"jsonrpc":"2.0","id":2,"method":"prompts/get","params":{"name":"security_audit","arguments":{"scope":"quick"}}}"#,
    ]);

    assert!(responses.len() >= 2);
    let res = &responses[1];
    assert_eq!(res["id"], 2);

    let messages = res["result"]["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");

    let text = messages[0]["content"]["text"].as_str().unwrap();
    assert!(
        text.contains("scan"),
        "Prompt should reference scan tool"
    );
    assert!(text.contains("CLEAN"), "Prompt should mention risk levels");
}

// ── Test: Janitor Preview Mode (Dry-Run Default) ─────────────────────

#[test]
fn test_mcp_janitor_preview_mode() {
    let hs = handshake();
    let responses = run_mcp_session(&[
        hs[0],
        hs[1],
        // Call janitor_apply_fix WITHOUT confirm:true — should preview / error, never apply
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"janitor_apply_fix","arguments":{"proposal_id":"nonexistent-id-123"}}}"#,
    ]);

    assert!(responses.len() >= 2);
    let res = &responses[1];
    assert_eq!(res["id"], 2);

    // Should return an error about missing proposal — crucially, it should NOT
    // have attempted to apply anything (no file modification)
    let text = res["result"]["content"][0]["text"].as_str().unwrap();
    let is_error = res["result"]["isError"].as_bool().unwrap_or(false);

    // Either "No proposal found" (preview path) or "Janitor plugin not active"
    assert!(
        text.contains("No proposal found") || text.contains("not active"),
        "Expected preview error, got: {text}"
    );
    assert!(is_error, "Expected isError=true for missing proposal");
}

// ── Test: get_diagnostics with severity filter ───────────────────────

#[test]
fn test_mcp_diagnostics_severity_filter() {
    let hs = handshake();
    let responses = run_mcp_session(&[
        hs[0],
        hs[1],
        // Call get_diagnostics with min_severity=error
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_diagnostics","arguments":{"min_severity":"error"}}}"#,
    ]);

    assert!(responses.len() >= 2);
    let res = &responses[1];
    assert_eq!(res["id"], 2);
    // Should return valid diagnostics result (may have 0 errors, that's fine)
    let text = res["result"]["content"][0]["text"].as_str().unwrap();
    // Valid outputs: "CLEAN: No diagnostics..." or "X errors, Y warnings" or "not active"
    assert!(
        text.contains("errors") || text.contains("diagnostics") || text.contains("not active") || text.contains("CLEAN"),
        "Expected diagnostics output, got: {text}"
    );
}
