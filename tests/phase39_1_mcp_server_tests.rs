//! Phase 39.1: Comprehensive MCP server tests.
//!
//! Covers initialize, tools/list, tools/call, resources/list, resources/read,
//! prompts/list, prompts/get, error handling, HTTP transport, and edge cases.

use serde_json::{json, Value};
use std::sync::Arc;
use tet_core::mcp::protocol::{error_codes, make_error, make_response};

fn setup_mcp() -> tet_core::mcp::server::McpServer {
    let hive = tet_core::hive::HivePeers::new();
    let (mesh, call_rx) = tet_core::mesh::TetMesh::new(10, hive);
    let sandbox = Arc::new(
        tet_core::sandbox::WasmtimeSandbox::new(
            mesh,
            Arc::new(tet_core::economy::VoucherManager::new("test".into())),
            false,
            "test-node".into(),
        )
        .expect("sandbox init"),
    );
    tet_core::mesh_worker::spawn_mesh_worker(sandbox.clone(), call_rx);
    tet_core::mcp::server::McpServer::new(sandbox)
}

fn make_request(id: i32, method: &str, params: Value) -> Vec<u8> {
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    serde_json::to_vec(&req).unwrap()
}

async fn call_mcp(mcp: &tet_core::mcp::server::McpServer, body: &[u8]) -> Value {
    let resp_bytes = mcp.handle_http_request(body).await;
    serde_json::from_slice(&resp_bytes).unwrap()
}

// ---- initialize ----

#[tokio::test]
async fn test_mcp_initialize_returns_capabilities() {
    let mcp = setup_mcp();
    let req = make_request(1, "initialize", json!({"protocolVersion": "2024-11-05"}));
    let resp = call_mcp(&mcp, &req).await;

    assert_eq!(resp["id"], json!(1));
    let result = &resp["result"];
    assert_eq!(result["protocolVersion"], "2024-11-05");
    assert!(result["capabilities"]["tools"].is_object());
    assert_eq!(result["serverInfo"]["name"], "Trytet Engine MCP");
}

// ---- tools/list ----

#[tokio::test]
async fn test_mcp_tools_list_returns_tools() {
    let mcp = setup_mcp();
    let req = make_request(2, "tools/list", json!({}));
    let resp = call_mcp(&mcp, &req).await;

    let tools = resp["result"]["tools"].as_array().unwrap();
    assert!(!tools.is_empty(), "Expected at least 1 tool");
    let js_tool = tools.iter().find(|t| t["name"] == "trytet_js_evaluator");
    assert!(js_tool.is_some(), "JS evaluator tool should be present");
    let js = js_tool.unwrap();
    assert!(js["inputSchema"]["required"]
        .as_array()
        .unwrap()
        .contains(&json!("code")));
}

// ---- tools/call ----

#[tokio::test]
async fn test_mcp_tools_call_unknown_tool() {
    let mcp = setup_mcp();
    let req = make_request(
        3,
        "tools/call",
        json!({"name": "nonexistent", "arguments": {}}),
    );
    let resp = call_mcp(&mcp, &req).await;

    assert!(resp["error"].is_object());
    assert_eq!(resp["error"]["code"], error_codes::METHOD_NOT_FOUND);
}

#[tokio::test]
async fn test_mcp_tools_call_missing_name() {
    let mcp = setup_mcp();
    let req = make_request(4, "tools/call", json!({"arguments": {}}));
    let resp = call_mcp(&mcp, &req).await;

    // Should handle missing name gracefully
    assert!(resp["result"]["isError"].as_bool().unwrap_or(false) || resp["error"].is_object());
}

// ---- resources/list ----

#[tokio::test]
async fn test_mcp_resources_list_not_implemented() {
    let mcp = setup_mcp();
    let req = make_request(5, "resources/list", json!({}));
    let resp = call_mcp(&mcp, &req).await;
    assert!(
        resp["error"].is_object(),
        "resources/list not yet implemented"
    );
}

#[tokio::test]
async fn test_mcp_resources_read_not_implemented() {
    let mcp = setup_mcp();
    let req = make_request(
        6,
        "resources/read",
        json!({"uri": "trytet://swarm/metrics"}),
    );
    let resp = call_mcp(&mcp, &req).await;
    assert!(
        resp["error"].is_object(),
        "resources/read not yet implemented"
    );
}

#[tokio::test]
async fn test_mcp_prompts_list_not_implemented() {
    let mcp = setup_mcp();
    let req = make_request(8, "prompts/list", json!({}));
    let resp = call_mcp(&mcp, &req).await;
    assert!(
        resp["error"].is_object(),
        "prompts/list not yet implemented"
    );
}

#[tokio::test]
async fn test_mcp_prompts_get_not_implemented() {
    let mcp = setup_mcp();
    let req = make_request(10, "prompts/get", json!({"name": "js_eval"}));
    let resp = call_mcp(&mcp, &req).await;
    assert!(resp["error"].is_object(), "prompts/get not yet implemented");
}

// ---- error cases ----

#[tokio::test]
async fn test_mcp_method_not_found() {
    let mcp = setup_mcp();
    let req = make_request(11, "nonexistent/method", json!({}));
    let resp = call_mcp(&mcp, &req).await;

    assert_eq!(resp["error"]["code"], error_codes::METHOD_NOT_FOUND);
}

#[tokio::test]
async fn test_mcp_invalid_json() {
    let mcp = setup_mcp();
    let invalid_json = b"not valid json at all";
    let resp_bytes = mcp.handle_http_request(invalid_json).await;
    let resp: Value = serde_json::from_slice(&resp_bytes).unwrap();

    assert!(resp["error"].is_object());
    assert_eq!(resp["error"]["code"], error_codes::PARSE_ERROR);
}

// ---- protocol unit tests ----

#[test]
fn test_protocol_make_response() {
    let resp = make_response(json!(42), json!({"ok": true}));
    assert_eq!(resp.jsonrpc, "2.0");
    assert_eq!(resp.id, json!(42));
    assert_eq!(resp.result["ok"], json!(true));
}

#[test]
fn test_protocol_make_error() {
    let err = make_error(json!(1), -32600, "Invalid Request".into());
    assert_eq!(err.jsonrpc, "2.0");
    assert_eq!(err.error.code, -32600);
    assert_eq!(err.error.message, "Invalid Request");
}

#[test]
fn test_protocol_error_codes() {
    assert_eq!(error_codes::PARSE_ERROR, -32700);
    assert_eq!(error_codes::METHOD_NOT_FOUND, -32601);
    assert_eq!(error_codes::INVALID_PARAMS, -32602);
    assert_eq!(error_codes::INTERNAL_ERROR, -32603);
}
