#![no_main]

use libfuzzer_sys::fuzz_target;
use tet_core::economy::{FuelVoucher, VoucherManager};

// Fuzz target: feed arbitrary bytes to FuelVoucher deserialization and
// VoucherManager::verify_and_claim.
// Goal: ensure that voucher validation never panics and always returns a
// clean Err (or Ok for legitimately constructed vouchers).
fuzz_target!(|data: &[u8]| {
    // serde_json::from_slice handles incomplete/truncated input gracefully.
    if let Ok(voucher) = serde_json::from_slice::<FuelVoucher>(data) {
        // Use a fresh VoucherManager for each call so nonce-reuse detection
        // doesn't interfere with the fuzzer's feedback loop.
        let manager = VoucherManager::new("fuzz-provider".to_string());
        let _ = manager.verify_and_claim(&voucher);
    }
});
