use crate::engine::TetSandbox;
use crate::mesh::MeshMessage;
use crate::models::{CrashReport, ExecutionStatus, MeshCallResponse, TetExecutionRequest};
use crate::sandbox::WasmtimeSandbox;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Spawns the background worker for the Tet-Mesh.
/// This loops indefinitely, receiving MeshCallRequests from host functions,
/// executing the sub-requests natively via the Sandbox, and passing the results back.
pub fn spawn_mesh_worker(sandbox: Arc<WasmtimeSandbox>, mut rx: mpsc::Receiver<MeshMessage>) {
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match msg {
                MeshMessage::Call { req, reply } => {
                    let target_alias = req.target_alias.clone();

                    let metadata = match sandbox.mesh.resolve(&target_alias).await {
                        Some(m) => m,
                        None => {
                            let _ = reply.send(MeshCallResponse {
                                status: ExecutionStatus::Crash(CrashReport {
                                    error_type: "alias_not_found".into(),
                                    instruction_offset: None,
                                    message: format!(
                                        "Tet-Mesh could not resolve alias: {}",
                                        target_alias
                                    ),
                                }),
                                return_data: vec![],
                                fuel_used: 0,
                            });
                            continue;
                        }
                    };

                    let mut injected_files = HashMap::new();
                    injected_files.insert(
                        "rpc_payload.json".to_string(),
                        String::from_utf8_lossy(&req.payload).to_string(),
                    );

                    let execution_req = TetExecutionRequest {
                        payload: None, // The fork handles the snapshot/wasm inheritance
                        alias: Some(req.target_alias.clone()),
                        env: std::collections::HashMap::new(),
                        injected_files: std::collections::HashMap::new(),
                        allocated_fuel: req.fuel_to_transfer,
                        max_memory_mb: 64,
                        parent_snapshot_id: metadata.snapshot_id,
                        call_depth: req.current_depth + 1,
                        voucher: None,
                        manifest: None,
                        egress_policy: None,
                    };

                    let result = sandbox.execute(execution_req).await;

                    match result {
                        Ok(res) => {
                            println!("CHILD EXECUTED. STATUS: {:?}", res.status);
                            println!("CHILD STDOUT: {:?}", res.telemetry.stdout_lines);
                            println!("CHILD STDERR: {:?}", res.telemetry.stderr_lines);
                            let return_data = res
                                .mutated_files
                                .get("rpc_response.json")
                                .map(|s| s.as_bytes().to_vec())
                                .unwrap_or_default();

                            let _ = reply.send(MeshCallResponse {
                                status: res.status,
                                return_data,
                                fuel_used: res.fuel_consumed,
                            });
                        }
                        Err(e) => {
                            let _ = reply.send(MeshCallResponse {
                                status: ExecutionStatus::Crash(CrashReport {
                                    error_type: "mesh_execution_failed".into(),
                                    instruction_offset: None,
                                    message: e.to_string(),
                                }),
                                return_data: vec![],
                                fuel_used: 0,
                            });
                        }
                    }
                }
            }
        }
    });
}
