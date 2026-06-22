use std::env;
use std::fs;
fn main() {
    let args: Vec<String> = env::args().collect();
    let input = fs::read_to_string(&args[1]).expect("read input");
    let wasm = wat::parse_str(&input).expect("parse WAT");
    fs::write(&args[2], wasm).expect("write wasm");
}
