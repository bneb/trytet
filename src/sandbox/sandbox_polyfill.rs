use std::pin::Pin;
use async_trait::async_trait;

use crate::engine::{TetSandbox, TetError};
use crate::models::{
    TetExecutionRequest, TetExecutionResult, SnapshotResponse,
    TopologyEdge, MeshCallRequest, MeshCallResponse
};
use crate::memory::SearchQuery;
use crate::inference::{InferenceRequest, InferenceResponse};
use super::SnapshotPayload;

pub struct WebNativeSandbox;

impl WebNativeSandbox {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TetSandbox for WebNativeSandbox {
    async fn execute(&self, req: TetExecutionRequest) -> Result<TetExecutionResult, TetError> {
        Err(TetError::EngineError("Execution not implemented in browser yet".into()))
    }

    async fn snapshot(&self, tet_id: &str) -> Result<String, TetError> {
        Err(TetError::EngineError("Snapshot not implemented in browser".into()))
    }

    async fn export_snapshot(&self, snapshot_id: &str) -> Result<SnapshotPayload, TetError> {
        Err(TetError::EngineError("Export Snapshot not implemented in browser".into()))
    }

    async fn import_snapshot(&self, payload: SnapshotPayload) -> Result<String, TetError> {
        Err(TetError::EngineError("Import Snapshot not implemented yet".into()))
    }

    async fn fork(
        &self,
        snapshot_id: &str,
        req: TetExecutionRequest,
    ) -> Result<TetExecutionResult, TetError> {
        Err(TetError::EngineError("Fork not implemented yet".into()))
    }

    async fn get_topology(&self) -> Vec<TopologyEdge> {
        vec![]
    }

    async fn send_mesh_call(
        &self,
        req: MeshCallRequest,
    ) -> Result<MeshCallResponse, TetError> {
        Err(TetError::MeshError("Mesh calls not supported on web natively".into()))
    }

    async fn query_memory(&self, alias: &str, query: SearchQuery) -> Result<Vec<crate::memory::SearchResult>, TetError> {
        Err(TetError::EngineError("Memory Query not implemented in browser".into()))
    }
    
    async fn infer(&self, alias: &str, req: InferenceRequest, fuel_allowance: u64) -> Result<InferenceResponse, TetError> {
        Err(TetError::InferenceError("Inference not implemented in browser".into()))
    }
}
