use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BlockType {
    System,
    User,
    Assistant,
    ToolResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputContentBlock {
    pub block_type: BlockType,
    pub content: String,
    pub is_persistent: bool,
    pub importance_score: f32, // 0.0 to 1.0
    pub block_length: usize,   // Populated by estimator
}

impl InputContentBlock {
    pub fn new(block_type: BlockType, content: String) -> Self {
        let is_persistent = block_type == BlockType::System;
        let importance_score = match block_type {
            BlockType::System => 1.0,
            BlockType::User => 0.8,
            BlockType::Assistant => 0.5,
            BlockType::ToolResult => 0.2, // Lowest priority default
        };
        
        Self {
            block_type,
            content,
            is_persistent,
            importance_score,
            block_length: 0, // Set later or initialize to 0
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmSession {
    pub session_id: String,
    pub blocks: Vec<InputContentBlock>,
}

#[derive(Debug, Clone)]
pub enum PruningReport {
    NoChange,
    Pruned {
        tokens_removed: usize,
        blocks_evicted: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContextError {
    SystemPromptTooLarge,
    EstimatorFailure,
}
