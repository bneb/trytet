#![allow(dead_code)]
use std::path::Path;
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};

fn test_compile() {
    let mut builder = WasiCtxBuilder::new();
    let temp_dir_path = Path::new("/tmp");

    builder
        .preopened_dir(
            temp_dir_path,
            "/workspace",
            DirPerms::all(),
            FilePerms::all(),
        )
        .unwrap();
}
fn main() {}
