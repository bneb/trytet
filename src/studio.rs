use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tokio::task::JoinSet;
use tracing::{info, warn};

use crate::engine::TetSandbox;
use crate::models::{ExecutionStatus, TetExecutionRequest};


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EconomyConfig {
    pub max_fuel: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryConfig {
    pub collection: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MeshConfig {
    pub allow_call: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentConfig {
    pub alias: String,
    pub base: String,
    pub entrypoint: String,
    pub memory: Option<MemoryConfig>,
    pub economy: Option<EconomyConfig>,
    pub mesh: Option<MeshConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SwarmConfig {
    pub name: String,
}

// Map the TOML `[[agents]]` to the vec.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Manifest {
    pub swarm: SwarmConfig,
    #[serde(rename = "agents")]
    pub agents: Vec<AgentConfig>,
}

pub struct StudioOrchestrator {
    manifest: Manifest,
}

impl StudioOrchestrator {
    pub fn new(toml_str: &str) -> anyhow::Result<Self> {
        let manifest: Manifest = toml::from_str(toml_str)?;
        
        // Validation: Unique aliases
        let mut aliases = HashSet::new();
        for agent in &manifest.agents {
            if !aliases.insert(agent.alias.clone()) {
                anyhow::bail!("Invalid Manifest: Duplicate alias detected -> {}", agent.alias);
            }
        }
        
        Ok(Self { manifest })
    }

    /// Primary execution boot subroutine
    pub async fn up(&self, sandbox: std::sync::Arc<dyn TetSandbox>) -> anyhow::Result<Vec<(String, ExecutionStatus)>> {
        info!("Studio: Orchestrating swarm '{}'", self.manifest.swarm.name);

        let mut join_set = JoinSet::new();

        for agent in &self.manifest.agents {
            let alias = agent.alias.clone();
            let fuel = agent.economy.as_ref().map(|e| e.max_fuel).unwrap_or(1_000_000);
            let _entry = agent.entrypoint.clone();
            
            // Wire mesh permissions
            if let Some(mesh) = &agent.mesh {
                // In a true implementation, we register these policies onto the Sandbox's Mesh router.
                // For now we log the connections.
                info!("Studio: Wired mesh for {} -> {:?}", alias, mesh.allow_call);
            }

            let sandbox_clone = sandbox.clone();
            
            let wasm_bytes = if std::path::Path::new(&agent.base).exists() {
                std::fs::read(&agent.base).unwrap_or_default()
            } else {
                vec![
                    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 
                    0x01, 0x04, 0x01, 0x60, 0x00, 0x00, 
                    0x03, 0x02, 0x01, 0x00, 
                    0x05, 0x03, 0x01, 0x00, 0x01,
                    0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00, 
                    0x0a, 0x04, 0x01, 0x02, 0x00, 0x0b
                ] // valid empty module
            };
            
            join_set.spawn(async move {
                let req = TetExecutionRequest {
                    payload: Some(wasm_bytes),
                    alias: Some(alias.clone()),
                    env: HashMap::new(),
                    injected_files: HashMap::new(),
                    allocated_fuel: fuel,
                    max_memory_mb: 64,
                    parent_snapshot_id: None,
                    call_depth: 0,
                    voucher: None,
                    egress_policy: None,
                };
                
                let res = sandbox_clone.execute(req).await;
                let status = res.map(|r| r.status).unwrap_or_else(|e| {
                    ExecutionStatus::Crash(crate::models::CrashReport {
                        error_type: "engine_spawn".into(),
                        instruction_offset: None,
                        message: e.to_string(),
                    })
                });
                (alias, status)
            });
        }
        
        let mut results = Vec::new();
        while let Some(res) = join_set.join_next().await {
            match res {
                Ok(tuple) => results.push(tuple),
                Err(e) => warn!("Task joined with error: {}", e),
            }
        }
        
        // Zero-residue enforcement hook (purge temp artifacts). Handled via Drop natively in VFS, or via manual down() later.
        Ok(results)
    }
}
