use wasmtime::*;
fn main() {
    let config = Config::new();
    println!("async_support: {:?}", config);
}
