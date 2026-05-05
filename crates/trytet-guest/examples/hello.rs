//! Hello World. The simplest possible Trytet agent.
//!
//! Build: cargo build --target wasm32-wasip1 --release
//! Run:   tet up target/wasm32-wasip1/release/hello.wasm

use trytet_guest::print;

#[no_mangle]
pub extern "C" fn _start() {
    print("Hello from a Trytet agent.");
    print("I'm running inside a Wasm sandbox with fuel-bounded execution.");
    print("If I loop forever, the host will trap me. I can't crash the system.");
}
