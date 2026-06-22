use std::fs;
use std::path::Path;

fn main() {
    let workspace = Path::new("/workspace");
    let in_file = workspace.join("rpc_payload.json");
    let out_file = workspace.join("rpc_response.json");

    if let Ok(content) = fs::read_to_string(&in_file) {
        let modified = format!("{content}-ECHO");
        println!("Receiver processed: {content}");
        fs::write(&out_file, &modified).unwrap();
    } else {
        println!("Receiver paused, hibernating...");
    }
}
