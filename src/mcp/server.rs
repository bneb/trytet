use crate::engine::TetSandbox;
use crate::mcp::protocol::{make_error, make_response, JsonRpcRequest};
use crate::sandbox::WasmtimeSandbox;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

/// Represents a registered cartridge tool.
#[derive(Clone)]
struct CartridgeDef {
    name: &'static str,
    cid: &'static str,
    fname: &'static str,
    fuel: u64,
    memory_mb: u32,
}

pub struct McpServer {
    sandbox: Arc<WasmtimeSandbox>,
    /// Directories searched for compiled cartridge .wasm files.
    cartridge_dirs: Vec<PathBuf>,
    /// Additional cartridges registered at runtime (beyond the built-in defaults).
    extra_cartridges: RwLock<Vec<CartridgeDef>>,
}

/// Default cartridge definitions shipped with Trytet.
const DEFAULT_CARTRIDGES: &[CartridgeDef] = &[
    CartridgeDef {
        name: "trytet_js_evaluator",
        cid: "js-evaluator",
        fname: "js_evaluator.wasm",
        fuel: 5_000_000,
        memory_mb: 32,
    },
    CartridgeDef {
        name: "trytet_regex_evaluator",
        cid: "regex-evaluator",
        fname: "regex_evaluator.wasm",
        fuel: 1_000_000,
        memory_mb: 16,
    },
    CartridgeDef {
        name: "trytet_jmespath_evaluator",
        cid: "jmespath-cartridge",
        fname: "jmespath_cartridge.wasm",
        fuel: 1_000_000,
        memory_mb: 16,
    },
    CartridgeDef {
        name: "trytet_scraper",
        cid: "scraper-cartridge",
        fname: "scraper_cartridge.wasm",
        fuel: 5_000_000,
        memory_mb: 32,
    },
    CartridgeDef {
        name: "trytet_structured_data",
        cid: "sql-cartridge",
        fname: "sql_cartridge.wasm",
        fuel: 1_000_000,
        memory_mb: 32,
    },
];

impl McpServer {
    /// Create a new MCP server.
    ///
    /// Cartridge search directories are determined by (in order):
    /// 1. `TRYTET_CARTRIDGE_DIR` env var (colon-separated, like PATH)
    /// 2. Directory containing the `tet` binary (for tarball installs)
    /// 3. `<current_dir>` (in-tree development: `<cwd>/crates/<cid>/...`)
    /// 4. `~/.trytet/cartridges/` (user install path)
    /// 5. `dist/cartridges/` (development build artifacts)
    pub fn new(sandbox: Arc<WasmtimeSandbox>) -> Self {
        let mut dirs = Vec::new();

        // 1. Explicit env var (colon-separated)
        if let Ok(var) = std::env::var("TRYTET_CARTRIDGE_DIR") {
            for d in var.split(':') {
                let p = PathBuf::from(d);
                if p.is_dir() {
                    dirs.push(p);
                }
            }
        }

        // 2. Binary-relative (tarball install: tet and cartridges/ are siblings)
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                dirs.push(parent.to_path_buf());
            }
        }

        // 3. In-tree development path (relative to cwd)
        if let Ok(cwd) = std::env::current_dir() {
            dirs.push(cwd);
        }

        // 4. User install path
        if let Some(home) = home::home_dir() {
            dirs.push(home.join(".trytet").join("cartridges"));
        }

        // 5. Development build artifacts
        if let Ok(cwd) = std::env::current_dir() {
            dirs.push(cwd.join("dist").join("cartridges"));
        }

        let server = Self {
            sandbox,
            cartridge_dirs: dirs,
            extra_cartridges: RwLock::new(Vec::new()),
        };

        // Pre-warm the JS evaluator — eliminates the 400ms Cranelift
        // compilation penalty on the first tools/call invocation.
        server.ensure_loaded("js-evaluator", "js_evaluator.wasm");

        server
    }

    /// Register an additional cartridge tool at runtime.
    ///
    /// Call before starting the server. The tool will appear in `tools/list`
    /// alongside the built-in defaults.
    pub fn register_tool(
        &self,
        name: &'static str,
        cartridge_id: &'static str,
        wasm_filename: &'static str,
        fuel: u64,
        memory_mb: u32,
    ) {
        self.extra_cartridges
            .write()
            .expect("RwLock not poisoned")
            .push(CartridgeDef {
                name,
                cid: cartridge_id,
                fname: wasm_filename,
                fuel,
                memory_mb,
            });
    }

    /// All registered cartridges (built-in defaults + runtime additions).
    fn all_cartridges(&self) -> Vec<CartridgeDef> {
        let mut all = DEFAULT_CARTRIDGES.to_vec();
        all.extend(
            self.extra_cartridges
                .read()
                .expect("RwLock not poisoned")
                .iter()
                .cloned(),
        );
        all
    }

    pub async fn handle_http_request(&self, body: &[u8]) -> Vec<u8> {
        let req: JsonRpcRequest = match serde_json::from_slice(body) {
            Ok(r) => r,
            Err(e) => {
                let err = make_error(Value::Null, -32700, format!("Parse error: {}", e));
                return serde_json::to_vec(&err).unwrap_or_default();
            }
        };
        let response = self.handle_request(req).await;
        serde_json::to_vec(&response).unwrap_or_default()
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
            let mut found = false;
            let mut total = 0usize;

            while !found {
                let buf = reader.fill_buf().await?;
                if buf.is_empty() {
                    break;
                }
                let (n, nl) = match buf.iter().position(|&b| b == b'\n') {
                    Some(i) => (i + 1, true),
                    None => (buf.len(), false),
                };
                line.push_str(&String::from_utf8_lossy(&buf[..n]));
                reader.consume(n);
                total += n;
                found = nl;
                if line.len() > 10 * 1024 * 1024 {
                    anyhow::bail!("MCP payload exceeded 10MB limit");
                }
            }
            if total == 0 && line.is_empty() {
                break;
            }
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<JsonRpcRequest>(&line) {
                Ok(req) => {
                    let resp = self.handle_request(req).await;
                    let mut bytes = serde_json::to_vec(&resp)?;
                    bytes.push(b'\n');
                    tx.write_all(&bytes).await?;
                    tx.flush().await?;
                }
                Err(e) => {
                    let err = make_error(Value::Null, -32700, format!("Parse error: {}", e));
                    let mut bytes = serde_json::to_vec(&err)?;
                    bytes.push(b'\n');
                    tx.write_all(&bytes).await?;
                    tx.flush().await?;
                }
            }
        }
        Ok(())
    }

    fn ensure_loaded(&self, cid: &str, fname: &str) {
        let mgr = &self.sandbox.cartridge_manager;
        if mgr.is_cached(cid) {
            return;
        }

        // Search each configured directory, trying <dir>/<fname> and
        // <dir>/crates/<cid>/target/wasm32-wasip1/release/<fname>
        for base in &self.cartridge_dirs {
            for candidate in &[
                base.join(fname),
                base.join("crates")
                    .join(cid)
                    .join("target/wasm32-wasip1/release")
                    .join(fname),
                base.join("crates")
                    .join(cid)
                    .join("target/wasm32-wasip2/release")
                    .join(fname),
            ] {
                if let Ok(wasm) = std::fs::read(candidate) {
                    let _ = mgr.precompile(cid, &wasm);
                    return;
                }
            }
        }
    }

    async fn handle_request(&self, req: JsonRpcRequest) -> Value {
        match req.method.as_str() {
            "initialize" => self.init(req.id),
            "tools/list" => self.list_tools(req.id),
            "tools/call" => self.invoke_tool(req.id, &req.params).await,
            _ => serde_json::to_value(make_error(req.id, -32601, "Method not found".into()))
                .expect("make_error serializes"),
        }
    }

    fn init(&self, id: Value) -> Value {
        serde_json::to_value(make_response(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "Trytet Engine MCP", "version": "0.2.0" }
            }),
        ))
        .expect("response serializes")
    }

    fn list_tools(&self, id: Value) -> Value {
        let mut tools: Vec<Value> = self.all_cartridges().iter().map(|def| {
            let (schema, description) = match def.name {
                "trytet_js_evaluator" => (
                    json!({"type":"object","properties":{"code":{"type":"string","description":"The JavaScript code to execute."}},"required":["code"]}),
                    "Execute JavaScript code in an uncrashable WebAssembly sandbox."
                ),
                "trytet_regex_evaluator" => (
                    json!({"type":"object","properties":{"pattern":{"type":"string","description":"The regex pattern."},"input":{"type":"string","description":"The input text."}},"required":["pattern","input"]}),
                    "Run regex patterns safely in a ReDoS-protected sandbox."
                ),
                "trytet_jmespath_evaluator" => (
                    json!({"type":"object","properties":{"expression":{"type":"string","description":"JMESPath expression."},"json":{"type":"string","description":"JSON data to query."}},"required":["expression","json"]}),
                    "Query JSON data with JMESPath expressions."
                ),
                "trytet_scraper" => (
                    json!({"type":"object","properties":{"url":{"type":"string","description":"URL to fetch."}},"required":["url"]}),
                    "Fetch a URL and return its content."
                ),
                "trytet_structured_data" => (
                    json!({"type":"object","properties":{"data":{"type":"array","description":"JSON array to query."},"filter_key":{"type":"string"},"filter_value":{"type":"string"},"sort_key":{"type":"string"},"sort_desc":{"type":"boolean"},"limit":{"type":"integer"},"offset":{"type":"integer"}},"required":["data"]}),
                    "Filter, sort, and paginate JSON arrays with SQL-like operations."
                ),
                "trytet_execute" => (
                    json!({"type":"object","properties":{"code":{"type":"string","description":"Code to execute."},"language":{"type":"string","description":"javascript or python (default: javascript)"},"fuel":{"type":"integer","description":"Fuel budget (default: 5000000)"},"memory_mb":{"type":"integer","description":"Memory cap in MB (default: 64)"}},"required":["code"]}),
                    "Execute JavaScript or Python code in a fuel-metered WebAssembly sandbox."
                ),
                "trytet_snapshot" => (
                    json!({"type":"object","properties":{"agent_id":{"type":"string","description":"Agent ID to snapshot."}},"required":["agent_id"]}),
                    "Capture the memory and filesystem state of a running agent."
                ),
                "trytet_fork" => (
                    json!({"type":"object","properties":{"snapshot_id":{"type":"string","description":"Snapshot ID to fork from."}},"required":["snapshot_id"]}),
                    "Fork a new agent from a saved snapshot."
                ),
                _ => (json!({}), ""),
            };
            json!({"name": def.name, "description": description, "inputSchema": schema})
        }).collect();

        // Add non-cartridge tools
        tools.push(json!({"name": "trytet_execute", "description": "Execute JavaScript or Python code in a fuel-metered WebAssembly sandbox.", "inputSchema": {"type":"object","properties":{"code":{"type":"string","description":"Code to execute."},"language":{"type":"string","description":"javascript or python (default: javascript)"},"fuel":{"type":"integer","description":"Fuel budget (default: 5000000)"},"memory_mb":{"type":"integer","description":"Memory cap in MB (default: 64)"}},"required":["code"]}}));
        tools.push(json!({"name": "trytet_snapshot", "description": "Capture the memory and filesystem state of a running agent.", "inputSchema": {"type":"object","properties":{"agent_id":{"type":"string","description":"Agent ID to snapshot."}},"required":["agent_id"]}}));
        tools.push(json!({"name": "trytet_fork", "description": "Fork a new agent from a saved snapshot.", "inputSchema": {"type":"object","properties":{"snapshot_id":{"type":"string","description":"Snapshot ID to fork from."}},"required":["snapshot_id"]}}));

        serde_json::to_value(make_response(id, json!({"tools": tools})))
            .expect("tools/list response serializes")
    }

    async fn invoke_tool(&self, id: Value, params: &Value) -> Value {
        let name = params["name"].as_str().unwrap_or("");
        let args = &params["arguments"];

        // Non-cartridge tools: execute, snapshot, fork
        match name {
            "trytet_execute" => return self.invoke_execute(id, args).await,
            "trytet_snapshot" => return self.invoke_snapshot(id, args).await,
            "trytet_fork" => return self.invoke_fork(id, args).await,
            _ => {}
        }

        match self.all_cartridges().iter().find(|def| def.name == name) {
            Some(def) => {
                self.ensure_loaded(def.cid, def.fname);
                let payload = build_payload(def.name, args);
                let mgr = self.sandbox.cartridge_manager.clone();
                let (output, is_error) = match mgr.invoke(
                    def.cid,
                    &payload,
                    def.fuel,
                    def.memory_mb,
                ) {
                    Ok((out, _)) => (out, false),
                    Err(crate::cartridge::CartridgeError::FuelExhausted) => (
                        "Your code ran out of execution fuel. Try breaking it into smaller steps."
                            .into(),
                        true,
                    ),
                    Err(crate::cartridge::CartridgeError::MemoryExceeded) => (
                        "Your code exceeded the memory limit. Try using less memory.".into(),
                        true,
                    ),
                    Err(crate::cartridge::CartridgeError::ExecutionError(msg, _)) => (msg, true),
                    Err(e) => (
                        format!(
                            "Sandbox stopped: {}. This protects the system from runaway code.",
                            e
                        ),
                        true,
                    ),
                };
                serde_json::to_value(make_response(
                    id,
                    json!({
                        "content": [{"type": "text", "text": output}],
                        "isError": is_error
                    }),
                ))
                .expect("response serializes")
            }
            None => {
                serde_json::to_value(make_error(id, -32601, format!("Tool not found: {}", name)))
                    .unwrap()
            }
        }
    }

    async fn invoke_execute(&self, id: Value, args: &Value) -> Value {
        let code = args["code"].as_str().unwrap_or("");
        let language = args["language"].as_str().unwrap_or("javascript");
        let fuel = args["fuel"].as_u64().unwrap_or(5_000_000);
        let memory_mb = args["memory_mb"].as_u64().unwrap_or(64) as u32;

        let (cid, fname, payload) = match language {
            "python" => (
                "python-evaluator",
                "python_evaluator.wasm",
                code.to_string(),
            ),
            _ => ("js-evaluator", "js_evaluator.wasm", code.to_string()),
        };

        self.ensure_loaded(cid, fname);
        let mgr = self.sandbox.cartridge_manager.clone();
        let (output, is_error) = match mgr.invoke(cid, &payload, fuel, memory_mb) {
            Ok((out, metrics)) => (
                format!(
                    "{{\"stdout\":{},\"stderr\":\"\",\"fuel_used\":{},\"memory_kb\":0,\"traps\":[]}}",
                    serde_json::to_string(&out).unwrap_or_else(|_| "null".into()),
                    metrics.fuel_consumed
                ),
                false,
            ),
            Err(crate::cartridge::CartridgeError::FuelExhausted) => (
                "{\"stdout\":\"\",\"stderr\":\"fuel exhausted\",\"fuel_used\":0,\"memory_kb\":0,\"traps\":[\"FuelExhausted\"]}".into(),
                true,
            ),
            Err(crate::cartridge::CartridgeError::MemoryExceeded) => (
                "{\"stdout\":\"\",\"stderr\":\"memory limit exceeded\",\"fuel_used\":0,\"memory_kb\":0,\"traps\":[\"MemoryExceeded\"]}".into(),
                true,
            ),
            Err(e) => (
                format!("{{\"stdout\":\"\",\"stderr\":{},\"fuel_used\":0,\"memory_kb\":0,\"traps\":[\"ExecutionError\"]}}",
                    serde_json::to_string(&e.to_string()).unwrap_or_else(|_| "\"unknown error\"".into())
                ),
                true,
            ),
        };
        serde_json::to_value(make_response(
            id,
            json!({
                "content": [{"type": "text", "text": output}],
                "isError": is_error
            }),
        ))
        .expect("response serializes")
    }

    async fn invoke_snapshot(&self, id: Value, args: &Value) -> Value {
        let agent_id = args["agent_id"].as_str().unwrap_or("");
        if agent_id.is_empty() {
            return serde_json::to_value(make_error(id, -32602, "agent_id is required".into()))
                .unwrap();
        }
        match self.sandbox.snapshot(agent_id).await {
            Ok(snapshot_id) => {
                let content = json!({"snapshot_id": snapshot_id, "memory_kb": 0, "vfs_files": 0});
                serde_json::to_value(make_response(
                    id,
                    json!({
                        "content": [{"type": "text", "text": content.to_string()}],
                        "isError": false
                    }),
                ))
                .expect("response serializes")
            }
            Err(e) => {
                serde_json::to_value(make_error(id, -32603, format!("snapshot failed: {}", e)))
                    .unwrap()
            }
        }
    }

    async fn invoke_fork(&self, id: Value, args: &Value) -> Value {
        let snapshot_id = args["snapshot_id"].as_str().unwrap_or("");
        if snapshot_id.is_empty() {
            return serde_json::to_value(make_error(id, -32602, "snapshot_id is required".into()))
                .unwrap();
        }
        let req = crate::models::TetExecutionRequest {
            payload: None,
            alias: None,
            env: std::collections::HashMap::new(),
            injected_files: std::collections::HashMap::new(),
            allocated_fuel: 5_000_000,
            max_memory_mb: 64,
            parent_snapshot_id: Some(snapshot_id.to_string()),
            target_function: None,
            manifest: None,
            call_depth: 0,
            egress_policy: None,
            voucher: None,
        };
        match self.sandbox.fork(snapshot_id, req).await {
            Ok(result) => {
                let content = json!({"new_agent_id": result.tet_id});
                serde_json::to_value(make_response(
                    id,
                    json!({
                        "content": [{"type": "text", "text": content.to_string()}],
                        "isError": false
                    }),
                ))
                .expect("response serializes")
            }
            Err(e) => {
                serde_json::to_value(make_error(id, -32603, format!("fork failed: {}", e))).unwrap()
            }
        }
    }
}

fn build_payload(tool_name: &str, args: &Value) -> String {
    match tool_name {
        "trytet_js_evaluator" => args["code"].as_str().unwrap_or("").to_string(),
        "trytet_regex_evaluator" => {
            json!({"pattern": args["pattern"], "input": args["input"]}).to_string()
        }
        "trytet_jmespath_evaluator" => {
            json!({"expression": args["expression"], "json": args["json"]}).to_string()
        }
        "trytet_scraper" => json!({"url": args["url"]}).to_string(),
        "trytet_structured_data" => json!({
            "data": args["data"],
            "filter_key": args["filter_key"],
            "filter_value": args["filter_value"],
            "sort_key": args["sort_key"],
            "sort_desc": args["sort_desc"],
            "limit": args["limit"],
            "offset": args["offset"],
        })
        .to_string(),
        _ => "{}".to_string(),
    }
}
