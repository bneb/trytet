use wasmtime_wasi::{WasiCtxBuilder, DirPerms, FilePerms};
use std::path::Path;

fn test_compile() {
    let mut builder = WasiCtxBuilder::new();
    let temp_dir_path = Path::new("/tmp");

    builder.preopened_dir(
        temp_dir_path,
        "/workspace",
        DirPerms::all(),
        FilePerms::all(),
    ).unwrap();
}
fn main() {}
