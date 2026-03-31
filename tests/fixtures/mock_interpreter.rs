use std::fs;
use std::path::Path;

fn main() {
    let workspace = Path::new("/workspace");
    let in_file = workspace.join("main.txt");
    let out_file = workspace.join("out.txt");

    if !in_file.exists() {
        println!("Error: main.txt not found in /workspace");
        std::process::exit(1);
    }

    match fs::read_to_string(&in_file) {
        Ok(content) => {
            let modified = format!("{content}-MODIFIED");
            println!("Read: {content}");
            if let Err(e) = fs::write(&out_file, &modified) {
                println!("Error writing out.txt: {e}");
                std::process::exit(1);
            }
            println!("Success");
        }
        Err(e) => {
            println!("Error reading main.txt: {e}");
            std::process::exit(1);
        }
    }
}
