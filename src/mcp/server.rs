use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use serde_json::{json, Value};
use crate::sandbox::WasmtimeSandbox;
use crate::mcp::protocol::{JsonRpcRequest, make_response, make_error};

pub struct McpServer {
    sandbox: Arc<WasmtimeSandbox>,
}

impl McpServer {
    pub fn new(sandbox: Arc<WasmtimeSandbox>) -> Self {
        Self { sandbox }
    }

    pub async fn run<R, W>(&self, rx: R, mut tx: W) -> anyhow::Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut reader = BufReader::new(rx);
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;
            if bytes_read == 0 {
                break; // EOF
            }

            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<JsonRpcRequest>(&line) {
                Ok(req) => {
                    let response = self.handle_request(req).await;
                    let mut res_bytes = serde_json::to_vec(&response)?;
                    res_bytes.push(b'\n');
                    tx.write_all(&res_bytes).await?;
                    tx.flush().await?;
                }
                Err(e) => {
                    let err = make_error(Value::Null, -32700, format!("Parse error: {}", e));
                    let mut res_bytes = serde_json::to_vec(&err)?;
                    res_bytes.push(b'\n');
                    tx.write_all(&res_bytes).await?;
                    tx.flush().await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_request(&self, req: JsonRpcRequest) -> Value {
        match req.method.as_str() {
            "initialize" => {
                let result = json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "Trytet Engine MCP",
                        "version": "0.1.0"
                    }
                });
                serde_json::to_value(make_response(req.id, result)).unwrap()
            }
            "tools/list" => {
                let tools = vec![
                    json!({
                        "name": "trytet_js_evaluator",
                        "description": "Execute Javascript code in an uncrashable WebAssembly sandbox.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "code": {
                                    "type": "string",
                                    "description": "The Javascript code to execute."
                                }
                            },
                            "required": ["code"]
                        }
                    })
                ];
                let result = json!({
                    "tools": tools
                });
                serde_json::to_value(make_response(req.id, result)).unwrap()
            }
            "tools/call" => {
                let name = req.params["name"].as_str().unwrap_or("");
                if name == "trytet_js_evaluator" {
                    let code = req.params["arguments"]["code"].as_str().unwrap_or("");
                    
                    let mgr = self.sandbox.cartridge_manager.clone();
                    let (output, is_error) = match mgr.invoke("js-evaluator", code, 5_000_000, 32) {
                        Ok((out, _metrics)) => (out, false),
                        Err(crate::cartridge::CartridgeError::ExecutionError(msg, _)) => {
                            // The cartridge returned an error result (e.g. JS error)
                            // but did not crash the engine.
                            (msg, true)
                        }
                        Err(e) => {
                            // Engine-level trap (OutOfFuel, MemoryExceeded, etc.)
                            (format!("Engine Trap: {:?}", e), true)
                        }
                    };

                    let result = json!({
                        "content": [
                            {
                                "type": "text",
                                "text": output
                            }
                        ],
                        "isError": is_error
                    });

                    serde_json::to_value(make_response(req.id, result)).unwrap()
                } else {
                    serde_json::to_value(make_error(req.id, -32601, "Tool not found".to_string())).unwrap()
                }
            }
            _ => {
                serde_json::to_value(make_error(req.id, -32601, "Method not found".to_string())).unwrap()
            }
        }
    }
}
