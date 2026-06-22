use std::env;

fn main() {
    // Pre-allocate some memory to simulate a heavy base state (e.g., loaded ASTs or model weights)
    // 5MB of random data
    let mut heavy_state = vec![0u8; 5 * 1024 * 1024];
    for i in 0..heavy_state.len() {
        heavy_state[i] = (i % 256) as u8;
    }
    
    // Check permutation instruction
    let code = env::var("PERMUTATION_CODE").unwrap_or_else(|_| "SUCCESS".to_string());
    
    match code.as_str() {
        "INFINITE_LOOP" => {
            println!("Evaluating permutation... Trapped in logic cycle.");
            loop {
                // Infinite cycle
                let mut _x = 1;
                _x += 1;
            }
        }
        "MEMORY_BOMB" => {
            println!("Evaluating permutation... Attempting massive allocation.");
            let mut vectors = Vec::new();
            loop {
                vectors.push(vec![0u64; 100_000]); // Allocate ~800KB rapidly
            }
        }
        "CRASH" => {
            panic!("Evaluating permutation... Division by zero in generated AST!");
        }
        _ => {
            // SUCCESS case
            println!("Evaluating permutation... Syntactically valid. Tests passed.");
            println!("RESULT_HASH: {:x}", heavy_state.iter().take(10).map(|x| *x as u64).sum::<u64>());
        }
    }
}
