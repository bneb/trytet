use super::estimator;
use super::models::{BlockType, ContextError, InputContentBlock, PruningReport, SwarmSession};
use tracing::info;

pub enum EvictionStrategy {
    Fifo,
    LargeBlockFirst,
    Hybrid,
}

pub struct ContextRouter {
    pub max_tokens: usize,
    pub strategy: EvictionStrategy,
}

impl ContextRouter {
    pub fn optimize(&self, session: &mut SwarmSession) -> Result<PruningReport, ContextError> {
        // Hydrate block lengths internally for math
        for block in &mut session.blocks {
            block.block_length = estimator::estimate_block(block);
        }

        let initial_estimate = estimator::estimate_tokens(&session.blocks);

        if initial_estimate <= self.max_tokens {
            return Ok(PruningReport::NoChange);
        }

        info!(
            "ContextRouter pressure detected: {} > max {} (Phase 13.1)",
            initial_estimate, self.max_tokens
        );

        let mut current_estimate;
        let mut blocks_evicted = 0;

        // ToolResult Truncation Rule:
        // If a ToolResult exceeds 50% of the window AND we are over pressure, try truncating it first.
        let fifty_percent = (self.max_tokens as f64 * 0.5) as usize;
        for block in session.blocks.iter_mut() {
            if block.block_type == BlockType::ToolResult && block.block_length > fifty_percent {
                // Determine how much to truncate. Let's slice string to fit under 50% target length
                // Approximation: 1 token = 4 chars, so target chars = fifty_percent * 4.
                let mut target_len = fifty_percent * 4;
                if target_len > block.content.len() {
                    target_len = block.content.len();
                }

                // Summary substitute truncation
                let truncation_notice = "... [TRUNCATED BY CONTEXT ROUTER]";
                if target_len > truncation_notice.len() {
                    let mut sliced =
                        String::from(&block.content[..target_len - truncation_notice.len()]);
                    sliced.push_str(truncation_notice);
                    block.content = sliced;
                    block.block_length = estimator::estimate_block(block);
                }
            }
        }

        // Re-estimate after truncation
        current_estimate = estimator::estimate_tokens(&session.blocks);
        if current_estimate <= self.max_tokens {
            return Ok(PruningReport::Pruned {
                tokens_removed: initial_estimate - current_estimate,
                blocks_evicted,
            });
        }

        // Eviction Loop
        while current_estimate > self.max_tokens {
            // Find candidate to evict
            let candidate_idx = self.find_eviction_candidate(&session.blocks);

            if let Some(idx) = candidate_idx {
                session.blocks.remove(idx);
                blocks_evicted += 1;
                current_estimate = estimator::estimate_tokens(&session.blocks);
            } else {
                // No candidates left (only system prompts remain)
                if current_estimate > self.max_tokens {
                    return Err(ContextError::SystemPromptTooLarge);
                }
                break;
            }
        }

        Ok(PruningReport::Pruned {
            tokens_removed: initial_estimate - current_estimate,
            blocks_evicted,
        })
    }

    fn find_eviction_candidate(&self, blocks: &[InputContentBlock]) -> Option<usize> {
        match self.strategy {
            EvictionStrategy::Fifo => {
                // Find oldest (lowest index) non-persistent block
                blocks.iter().position(|b| !b.is_persistent)
            }
            EvictionStrategy::LargeBlockFirst => {
                // Find the largest non-persistent block
                blocks
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| !b.is_persistent)
                    .max_by_key(|(_, b)| b.block_length)
                    .map(|(i, _)| i)
            }
            EvictionStrategy::Hybrid => {
                // Hybrid: Score = block_length * (1.0 - importance_score)
                blocks
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| !b.is_persistent)
                    .max_by(|(_, a), (_, b)| {
                        let score_a = a.block_length as f32 * (1.0 - a.importance_score);
                        let score_b = b.block_length as f32 * (1.0 - b.importance_score);
                        score_a
                            .partial_cmp(&score_b)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(i, _)| i)
            }
        }
    }
}
