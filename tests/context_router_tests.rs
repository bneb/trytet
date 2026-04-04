use tet_core::api::context::estimator;
use tet_core::api::context::models::{BlockType, InputContentBlock, PruningReport, SwarmSession};
use tet_core::api::context::router::{ContextRouter, EvictionStrategy};

#[test]
fn test_hard_ceiling_override() {
    // 1 token = 4 chars roughly. 10000 tokens = 40,000 chars.
    let system_content = "S".repeat(400); // 100 tokens
    let user_content_1 = "U".repeat(20000); // 5000 tokens
    let user_content_2 = "V".repeat(20000); // 5000 tokens

    let mut session = SwarmSession {
        session_id: "test1".to_string(),
        model_id: Some("LlamaV5".to_string()),
        temperature: Some(0.7),
        blocks: vec![
            InputContentBlock::new(BlockType::System, system_content.clone()),
            InputContentBlock::new(BlockType::User, user_content_1),
            InputContentBlock::new(BlockType::User, user_content_2),
        ],
    };

    let router = ContextRouter {
        max_tokens: 5000,
        strategy: EvictionStrategy::Fifo, // Evict oldest non-persistent first
    };

    let result = router
        .optimize(&mut session)
        .expect("Should successfully prune");

    if let PruningReport::Pruned {
        tokens_removed,
        blocks_evicted,
    } = result
    {
        assert!(blocks_evicted > 0);
        assert!(tokens_removed > 0);

        let new_estimate = estimator::estimate_tokens(&session.blocks);
        assert!(new_estimate <= 5000);

        // Ensure System Prompt was not evicted
        assert_eq!(session.blocks[0].block_type, BlockType::System);
        assert_eq!(session.blocks[0].content, system_content);
    } else {
        panic!("Expected PruningReport::Pruned");
    }
}

#[test]
fn test_toolresult_truncation() {
    // 8000 tokens = 32000 chars
    let huge_json = format!("{{\"data\": \"{}\"}}", "X".repeat(31900));

    let mut session = SwarmSession {
        session_id: "test2".to_string(),
        model_id: Some("LlamaV5".to_string()),
        temperature: Some(0.7),
        blocks: vec![
            InputContentBlock::new(BlockType::System, "SYSTEM".to_string()),
            InputContentBlock::new(BlockType::ToolResult, huge_json),
        ],
    };

    let router = ContextRouter {
        max_tokens: 5000,
        strategy: EvictionStrategy::Fifo,
    };

    let result = router
        .optimize(&mut session)
        .expect("Should apply truncation logic");

    if let PruningReport::Pruned { .. } = result {
        let new_estimate = estimator::estimate_tokens(&session.blocks);
        assert!(
            new_estimate <= 5000,
            "Estimate {} should be <= 5000",
            new_estimate
        );

        assert_eq!(session.blocks.len(), 2, "Should truncate, not evict");
        assert_eq!(session.blocks[1].block_type, BlockType::ToolResult);
        assert!(session.blocks[1]
            .content
            .ends_with("... [TRUNCATED BY CONTEXT ROUTER]"));
    } else {
        panic!("Expected PruningReport::Pruned");
    }
}
