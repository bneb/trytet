use std::fs;

fn main() {
    fs::write("/workspace/out.txt", "HELLO").unwrap();
}
