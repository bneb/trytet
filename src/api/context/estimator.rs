use super::models::InputContentBlock;

/// Estimate tokens using a simple heuristic: 4 chars = 1 token.
/// Also calculates the target `T_total` safety adjustment: `sum * 1.15`.
pub fn estimate_tokens(blocks: &[InputContentBlock]) -> usize {
    let mut total_tokens = 0;
    
    for block in blocks {
        total_tokens += std::cmp::max(1, block.content.len().div_ceil(4));
    }
    
    // Safety markup of 15%
    let t_total = (total_tokens as f64 * 1.15).ceil() as usize;
    t_total
}

/// Calculate specific block length via the same heuristic without safety buffer.
pub fn estimate_block(block: &InputContentBlock) -> usize {
    std::cmp::max(1, block.content.len().div_ceil(4))
}
