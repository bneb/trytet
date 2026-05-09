//! Phase 37.1: Model Context Protocol (MCP) Server — TDD Test Suite
//!
//! Validates that Trytet can expose its uncrashable Wasm Engine as an MCP
//! server over stdio, allowing IDEs like Cursor and AI agents like Claude
//! to execute code deterministically.

use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use tet_core::engine::TetSandbox;
use tet_core::sandbox::WasmtimeSandbox;
use tet_core::mcp::server::McpServer;

async fn setup_mock_mcp_server() -> (tokio::io::DuplexStream, tokio::task::JoinHandle<()>) {
    let hive_peers = tet_core::hive::HivePeers::new();
    let (mesh, mut worker_rx) = tet_core::mesh::TetMesh::new(100, hive_peers.clone());
    let sandbox = Arc::new(
        WasmtimeSandbox::new(
            mesh.clone(),
            Arc::new(tet_core::economy::VoucherManager::new("mcp-test".to_string())),
            false,
            "mcp-test".to_string(),
        )
        .unwrap(),
    );

    let sandbox_clone = sandbox.clone();
    tet_core::mesh_worker::spawn_mesh_worker(sandbox_clone, worker_rx);

    // Boot the JS Evaluator cartridge as an agent to register it in the mesh
    let js_wasm_path = std::env::current_dir()
        .unwrap()
        .join("crates/js-evaluator/target/wasm32-wasip1/release/js_evaluator.wasm");
    
    let js_wasm = std::fs::read(&js_wasm_path).unwrap_or_else(|_| {
        panic!("Missing js_evaluator.wasm. Run cargo component build in crates/js-evaluator");
    });
    
    sandbox.cartridge_manager.precompile("js-evaluator", &js_wasm).unwrap();

    // Create a duplex stream to simulate stdin/stdout
    let (client_stream, server_stream) = tokio::io::duplex(1024 * 1024 * 10);
    
    // We split the server_stream into read/write for the McpServer
    let (server_rx, server_tx) = tokio::io::split(server_stream);

    let mcp_server = McpServer::new(sandbox.clone());
    
    let handle = tokio::spawn(async move {
        // Start the server loop
        if let Err(e) = mcp_server.run(server_rx, server_tx).await {
            eprintln!("MCP Server exited with error: {}", e);
        }
    });

    (client_stream, handle)
}

async fn send_rpc(client_tx: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>, request: Value) {
    let mut msg = serde_json::to_vec(&request).unwrap();
    msg.push(b'\n'); // JSON-RPC over stdio uses newline-delimited JSON
    client_tx.write_all(&msg).await.unwrap();
    client_tx.flush().await.unwrap();
}

async fn read_rpc(client_rx: &mut tokio::io::ReadHalf<tokio::io::DuplexStream>) -> Value {
    let mut buf = vec![0u8; 1024 * 1024];
    let n = client_rx.read(&mut buf).await.unwrap();
    let msg_str = String::from_utf8_lossy(&buf[..n]);
    // Take the first line (in case of multiple)
    let line = msg_str.lines().next().unwrap();
    serde_json::from_str(line).unwrap()
}

// ===========================================================================
// Test 1: MCP Handshake
// ===========================================================================

#[tokio::test]
async fn test_mcp_initialize() {
    let (mut client_stream, _handle) = setup_mock_mcp_server().await;
    let (mut client_rx, mut client_tx) = tokio::io::split(client_stream);

    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        }
    });

    send_rpc(&mut client_tx, init_req).await;
    let response = read_rpc(&mut client_rx).await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["serverInfo"]["name"], "Trytet Engine MCP");
    assert!(response["result"]["capabilities"]["tools"].is_object());
}

// ===========================================================================
// Test 2: MCP Tools List
// ===========================================================================

#[tokio::test]
async fn test_mcp_tools_list() {
    let (mut client_stream, _handle) = setup_mock_mcp_server().await;
    let (mut client_rx, mut client_tx) = tokio::io::split(client_stream);

    let list_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });

    send_rpc(&mut client_tx, list_req).await;
    let response = read_rpc(&mut client_rx).await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    
    let tools = response["result"]["tools"].as_array().expect("Tools must be an array");
    
    // Ensure the js-evaluator cartridge is registered as a tool
    let js_tool = tools.iter().find(|t| t["name"] == "trytet_js_evaluator").expect("JS Evaluator tool missing");
    assert_eq!(js_tool["description"].as_str().unwrap(), "Execute Javascript code in an uncrashable WebAssembly sandbox.");
    assert_eq!(js_tool["inputSchema"]["type"], "object");
    assert!(js_tool["inputSchema"]["properties"]["code"].is_object());
}

// ===========================================================================
// Test 3: MCP Tool Call (Valid JS & Infinite Loop Fuel Trap)
// ===========================================================================

#[tokio::test]
async fn test_mcp_tools_call_execution() {
    let (mut client_stream, _handle) = setup_mock_mcp_server().await;
    let (mut client_rx, mut client_tx) = tokio::io::split(client_stream);

    // 1. Execute Valid JS
    let call_valid = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "trytet_js_evaluator",
            "arguments": {
                "code": "2 + 2"
            }
        }
    });

    send_rpc(&mut client_tx, call_valid).await;
    let res_valid = read_rpc(&mut client_rx).await;

    assert_eq!(res_valid["jsonrpc"], "2.0");
    assert_eq!(res_valid["id"], 3);
    assert!(!res_valid["result"]["isError"].as_bool().unwrap_or(false), "Valid request failed: {}", res_valid);
    let content = res_valid["result"]["content"][0]["text"].as_str().unwrap();
    assert!(content.contains("4"), "Output was: {}", content);

    // 2. Execute Malicious Infinite Loop
    let call_malicious = json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "trytet_js_evaluator",
            "arguments": {
                "code": "while(true) {}"
            }
        }
    });

    send_rpc(&mut client_tx, call_malicious).await;
    let res_malicious = read_rpc(&mut client_rx).await;

    assert_eq!(res_malicious["jsonrpc"], "2.0");
    assert_eq!(res_malicious["id"], 4);
    // MCP conventions: Tool execution errors return isError: true inside the result, NOT a JSON-RPC error
    // so the LLM context window sees the error output naturally.
    assert!(res_malicious["result"]["isError"].as_bool().unwrap_or(false));
    let err_content = res_malicious["result"]["content"][0]["text"].as_str().unwrap();
    assert!(err_content.contains("FuelExhausted") || err_content.contains("OutOfFuel"), "Engine must trap the infinite loop: {}", err_content);
}
