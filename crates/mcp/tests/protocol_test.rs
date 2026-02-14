//! MCP Protocol tests — JSON-RPC types, tool routing, resource/prompt listing,
//! and error code generation.

use serde_json::json;

use synapseed_core::context::SynapseContext;
use synapseed_core::liquid::ProjectDna;
use synapseed_core::state::ProjectState;

use synapseed_mcp::protocol::*;
use synapseed_mcp::prompts;
use synapseed_mcp::resources;
use synapseed_mcp::tools;

/// Helper: build a minimal SynapseContext rooted at a temp directory.
fn test_ctx(dir: &std::path::Path) -> SynapseContext {
    SynapseContext::new(
        dir.to_path_buf(),
        ProjectState::Unknown,
        ProjectDna::default(),
    )
}

// ══════════════════════════════════════════════════════════════
// 1. JSON-RPC Request / Response construction
// ══════════════════════════════════════════════════════════════

#[test]
fn test_jsonrpc_request_serialize_roundtrip() {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: json!(1),
        method: "tools/list".into(),
        params: json!({}),
    };
    let serialized = serde_json::to_string(&req).unwrap();
    let deserialized: JsonRpcRequest = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.jsonrpc, "2.0");
    assert_eq!(deserialized.method, "tools/list");
    assert_eq!(deserialized.id, json!(1));
}

#[test]
fn test_jsonrpc_request_with_string_id() {
    let req_json = r#"{"jsonrpc":"2.0","id":"abc-123","method":"ping","params":{}}"#;
    let req: JsonRpcRequest = serde_json::from_str(req_json).unwrap();
    assert_eq!(req.id, json!("abc-123"));
    assert_eq!(req.method, "ping");
}

#[test]
fn test_jsonrpc_request_default_params() {
    // params is optional and defaults to null
    let req_json = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
    let req: JsonRpcRequest = serde_json::from_str(req_json).unwrap();
    assert!(req.params.is_null());
}

#[test]
fn test_jsonrpc_response_success() {
    let resp = JsonRpcResponse::success(json!(42), json!({"ok": true}));
    assert_eq!(resp.jsonrpc, "2.0");
    assert_eq!(resp.id, json!(42));
    assert!(resp.result.is_some());
    assert!(resp.error.is_none());
    assert_eq!(resp.result.unwrap(), json!({"ok": true}));
}

#[test]
fn test_jsonrpc_response_error() {
    let resp = JsonRpcResponse::error(json!(7), -32601, "Method not found".into());
    assert_eq!(resp.jsonrpc, "2.0");
    assert_eq!(resp.id, json!(7));
    assert!(resp.result.is_none());
    let err = resp.error.unwrap();
    assert_eq!(err.code, -32601);
    assert_eq!(err.message, "Method not found");
    assert!(err.data.is_none());
}

#[test]
fn test_jsonrpc_response_success_serialization_skips_error() {
    let resp = JsonRpcResponse::success(json!(1), json!("ok"));
    let serialized = serde_json::to_string(&resp).unwrap();
    // error field should be absent (skip_serializing_if)
    assert!(!serialized.contains("\"error\""));
}

#[test]
fn test_jsonrpc_response_error_serialization_skips_result() {
    let resp = JsonRpcResponse::error(json!(1), -32600, "bad".into());
    let serialized = serde_json::to_string(&resp).unwrap();
    // result field should be absent (skip_serializing_if)
    assert!(!serialized.contains("\"result\""));
}

#[test]
fn test_jsonrpc_notification_roundtrip() {
    let notif = JsonRpcNotification {
        jsonrpc: "2.0".into(),
        method: "notifications/initialized".into(),
        params: json!({}),
    };
    let serialized = serde_json::to_string(&notif).unwrap();
    let deserialized: JsonRpcNotification = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.method, "notifications/initialized");
    // Notifications have no "id" field
    assert!(!serialized.contains("\"id\""));
}

// ══════════════════════════════════════════════════════════════
// 2. Error code constants
// ══════════════════════════════════════════════════════════════

#[test]
fn test_error_codes_match_jsonrpc_spec() {
    assert_eq!(PARSE_ERROR, -32700);
    assert_eq!(INVALID_REQUEST, -32600);
    assert_eq!(METHOD_NOT_FOUND, -32601);
    assert_eq!(INVALID_PARAMS, -32602);
    assert_eq!(INTERNAL_ERROR, -32603);
}

// ══════════════════════════════════════════════════════════════
// 3. Tool routing — known, legacy, unknown, fuzzy
// ══════════════════════════════════════════════════════════════

#[test]
fn test_tool_call_known_canonical_name() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = test_ctx(dir.path());
    // "scan" is a canonical tool name; it should dispatch without error
    let result = tools::handle_tool_call("scan", &json!({"content": "hello world"}), &ctx);
    assert!(
        result.is_error.is_none() || result.is_error == Some(false),
        "scan tool should succeed for clean input"
    );
}

#[test]
fn test_tool_call_legacy_alias() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = test_ctx(dir.path());
    // "scan_security" is a legacy alias for "scan"
    let result =
        tools::handle_tool_call("scan_security", &json!({"content": "hello world"}), &ctx);
    assert!(
        result.is_error.is_none() || result.is_error == Some(false),
        "Legacy alias scan_security should resolve to scan"
    );
}

#[test]
fn test_tool_call_unknown_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = test_ctx(dir.path());
    // Use a short gibberish name (< 20 chars, no spaces) that won't fuzzy-match
    // any real tool name (edit distance > 3) and won't trigger the natural-language
    // redirect (requires len > 20 or spaces/question mark).
    let result = tools::handle_tool_call(
        "zzqxwk",
        &json!({}),
        &ctx,
    );
    assert_eq!(result.is_error, Some(true));
    if let Some(ContentBlock::Text { text }) = result.content.first() {
        assert!(text.contains("Unknown tool"));
    }
}

#[test]
fn test_tool_call_fuzzy_match() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = test_ctx(dir.path());
    // "scna" is 1 edit distance from "scan" — should fuzzy-match
    let result = tools::handle_tool_call("scna", &json!({"content": "test"}), &ctx);
    // Fuzzy match auto-dispatches with a "Did you mean" prefix
    if let Some(ContentBlock::Text { text }) = result.content.first() {
        assert!(
            text.contains("Did you mean"),
            "Fuzzy match should include 'Did you mean' prefix, got: {}",
            &text[..text.len().min(200)]
        );
    }
}

#[test]
fn test_tool_call_diagnose() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = test_ctx(dir.path());
    let result = tools::handle_tool_call("diagnose", &json!({}), &ctx);
    assert!(
        result.is_error.is_none() || result.is_error == Some(false),
        "diagnose should succeed"
    );
    if let Some(ContentBlock::Text { text }) = result.content.first() {
        assert!(!text.is_empty(), "diagnose should return non-empty output");
    }
}

// ══════════════════════════════════════════════════════════════
// 4. Tool listing
// ══════════════════════════════════════════════════════════════

#[test]
fn test_list_tools_returns_all_24_tools() {
    let tools = tools::list_tools();
    assert_eq!(
        tools.len(),
        24,
        "Expected 24 tools, got {}",
        tools.len()
    );
}

#[test]
fn test_list_tools_contains_expected_names() {
    let tools = tools::list_tools();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    let expected = [
        "hoist", "lookup", "scan", "check", "blame", "diagnose", "consult",
        "search", "diagnostics", "analyze", "quickfix", "ask", "intent",
        "train", "reset-telemetry", "janitor", "janitor-fix", "architect",
        "oracle", "similar", "analyze_binary", "explain_dependency",
        "run_benchmark",
    ];
    for name in &expected {
        assert!(
            names.contains(name),
            "Tool '{}' missing from list_tools()",
            name
        );
    }
}

#[test]
fn test_tool_definitions_have_valid_schemas() {
    let tools = tools::list_tools();
    for tool in &tools {
        assert!(!tool.name.is_empty(), "Tool name must not be empty");
        assert!(!tool.description.is_empty(), "Tool '{}' has empty description", tool.name);
        // input_schema must have "type": "object"
        assert_eq!(
            tool.input_schema.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "Tool '{}' schema must have type=object",
            tool.name
        );
    }
}

// ══════════════════════════════════════════════════════════════
// 5. Resource listing and reading
// ══════════════════════════════════════════════════════════════

#[test]
fn test_list_resources_returns_10() {
    let resources = resources::list_resources();
    assert_eq!(
        resources.len(),
        10,
        "Expected 10 resources, got {}",
        resources.len()
    );
}

#[test]
fn test_resource_definitions_have_uris() {
    let resources = resources::list_resources();
    for r in &resources {
        assert!(
            r.uri.starts_with("synapseed://"),
            "Resource URI '{}' must start with synapseed://",
            r.uri
        );
        assert!(!r.name.is_empty(), "Resource '{}' has empty name", r.uri);
    }
}

#[test]
fn test_read_known_resource() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = test_ctx(dir.path());
    let content = resources::read_resource("synapseed://status", &ctx);
    assert!(content.is_some(), "synapseed://status should be readable");
    let content = content.unwrap();
    assert_eq!(content.uri, "synapseed://status");
    assert!(content.text.is_some());
    let text = content.text.unwrap();
    assert!(text.contains("project_root"), "Status should contain project_root");
}

#[test]
fn test_read_unknown_resource_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = test_ctx(dir.path());
    let content = resources::read_resource("synapseed://nonexistent", &ctx);
    assert!(content.is_none(), "Unknown resource should return None");
}

#[test]
fn test_read_dna_resource() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = test_ctx(dir.path());
    let content = resources::read_resource("synapseed://dna", &ctx).unwrap();
    let text = content.text.unwrap();
    assert!(text.contains("workspace_strategy"), "DNA should contain workspace_strategy");
    assert!(text.contains("monorepo"), "Default strategy should be monorepo");
}

// ══════════════════════════════════════════════════════════════
// 6. Prompt listing and expansion
// ══════════════════════════════════════════════════════════════

#[test]
fn test_list_prompts_returns_6() {
    let prompts = prompts::list_prompts();
    assert_eq!(
        prompts.len(),
        6,
        "Expected 6 prompts, got {}",
        prompts.len()
    );
}

#[test]
fn test_prompt_definitions_have_names_and_descriptions() {
    let prompts_list = prompts::list_prompts();
    for p in &prompts_list {
        assert!(!p.name.is_empty(), "Prompt has empty name");
        assert!(!p.description.is_empty(), "Prompt '{}' has empty description", p.name);
    }
}

#[test]
fn test_get_known_prompt() {
    let messages = prompts::get_prompt("describe_architecture", &json!({}));
    assert!(messages.is_some(), "describe_architecture prompt should exist");
    let messages = messages.unwrap();
    assert!(!messages.is_empty(), "Prompt should produce at least one message");
    assert_eq!(messages[0].role, "user");
    let ContentBlock::Text { ref text } = messages[0].content;
    assert!(
        text.contains("hoist"),
        "describe_architecture should reference the hoist tool"
    );
}

#[test]
fn test_get_unknown_prompt_returns_none() {
    let messages = prompts::get_prompt("nonexistent_prompt", &json!({}));
    assert!(messages.is_none());
}

#[test]
fn test_prompt_with_arguments() {
    let messages =
        prompts::get_prompt("explain_evolution", &json!({"file": "src/main.rs"}));
    assert!(messages.is_some());
    let messages = messages.unwrap();
    let ContentBlock::Text { ref text } = messages[0].content;
    assert!(
        text.contains("src/main.rs"),
        "Prompt should interpolate the file argument"
    );
}

// ══════════════════════════════════════════════════════════════
// 7. MCP type serialization details
// ══════════════════════════════════════════════════════════════

#[test]
fn test_content_block_text_serialization() {
    let block = ContentBlock::Text {
        text: "hello".into(),
    };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "text");
    assert_eq!(json["text"], "hello");
}

#[test]
fn test_tool_call_result_serialization() {
    let result = ToolCallResult {
        content: vec![ContentBlock::Text {
            text: "done".into(),
        }],
        is_error: Some(false),
    };
    let json = serde_json::to_value(&result).unwrap();
    assert!(json["content"].is_array());
    assert_eq!(json["isError"], false);
}

#[test]
fn test_tool_call_result_no_error_skips_field() {
    let result = ToolCallResult {
        content: vec![ContentBlock::Text {
            text: "ok".into(),
        }],
        is_error: None,
    };
    let serialized = serde_json::to_string(&result).unwrap();
    assert!(
        !serialized.contains("isError"),
        "is_error=None should be omitted from JSON"
    );
}

#[test]
fn test_initialize_result_camel_case() {
    let result = InitializeResult {
        protocol_version: "2024-11-05".into(),
        capabilities: ServerCapabilities {
            tools: Some(ToolsCapability {
                list_changed: Some(false),
            }),
            resources: None,
            prompts: None,
            logging: None,
        },
        server_info: ServerInfo {
            name: "test".into(),
            version: "0.1.0".into(),
        },
        instructions: None,
    };
    let json = serde_json::to_value(&result).unwrap();
    // camelCase: protocolVersion, serverInfo, listChanged
    assert!(json.get("protocolVersion").is_some());
    assert!(json.get("serverInfo").is_some());
    assert!(json["capabilities"]["tools"]["listChanged"] == json!(false));
}
