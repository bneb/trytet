use std::fs;

fn main() {
    println!("Mock Base-Tet Python Interpreter booting...");
    println!("Reading /workspace/script.py natively via VFS...");
    if let Ok(content) = fs::read_to_string("/workspace/script.py") {
        println!("Python WASI running: {}", content);
    } else {
        println!("Error: file not found");
    }
}
