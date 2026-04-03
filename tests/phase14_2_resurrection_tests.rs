use tet_core::builder::TetBuilder;
use tet_core::models::manifest::{AgentManifest, Metadata, ResourceConstraints, CapabilityPolicy};
use tet_core::models::ExecutionStatus;
use tet_core::resurrection::{ResurrectionContext, RuntimeError};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

async fn standard_manifest(name: &str, fuel: u64) -> (PathBuf, TempDir) {
    let dir = TempDir::new().unwrap();
    let manifest_path = dir.path().join("tet.toml");
    
    let toml = format!(r#"
        [metadata]
        name = "{}"
        version = "1.0.0"
        author_pubkey = "placeholder"

        [constraints]
        max_memory_pages = 256
        fuel_limit = {}

        [permissions]
        can_egress = []
        can_persist = true
        can_teleport = true
    "#, name, fuel);

    fs::write(&manifest_path, toml).unwrap();
    (manifest_path, dir)
}

#[tokio::test]
async fn test_lazarus_resurrection() {
    let (manifest_path, temp_dir) = standard_manifest("lazarus", 10_000_000).await;
    let out_tet = temp_dir.path().join("lazarus_agent.tet");
    
    let builder = TetBuilder {
        source_wasm: PathBuf::from("tests/fixtures/lazarus.wasm"),
        manifest_path,
        vfs_path: None,
        output_path: out_tet.clone(),
        signing_key: None,
    };
    
    builder.assemble().await.unwrap();

    let raw_bytes = fs::read(&out_tet).unwrap();
    let artifact = TetBuilder::verify_and_load(&raw_bytes).unwrap();
    
    let node_workspace = temp_dir.path().join("node_agent_ws");
    
    let ctx = ResurrectionContext {
        artifact,
        node_workspace: node_workspace.clone(),
    };
    
    let active_agent = ctx.boot(None::<u64>).await.expect("Failed to boot");
    
    assert_eq!(active_agent.result.status, ExecutionStatus::Success);
    
    // Check if the VFS was truly resurrected and populated
    let result_file = node_workspace.join("vfs").join("out.txt");
    assert!(result_file.exists(), "out.txt did not persist into node_workspace VFS");
    let contents = fs::read_to_string(result_file).unwrap();
    assert_eq!(contents, "HELLO");
}

#[tokio::test]
async fn test_straightjacket_constraints() {
    let (manifest_path, temp_dir) = standard_manifest("straightjacket", 100).await;
    let out_tet = temp_dir.path().join("sj_agent.tet");
    
    let builder = TetBuilder {
        source_wasm: PathBuf::from("tests/fixtures/straightjacket.wasm"),
        manifest_path,
        vfs_path: None,
        output_path: out_tet.clone(),
        signing_key: None,
    };
    
    builder.assemble().await.unwrap();

    let raw_bytes = fs::read(&out_tet).unwrap();
    let artifact = TetBuilder::verify_and_load(&raw_bytes).unwrap();
    
    let node_workspace = temp_dir.path().join("node_agent_ws2");
    
    let ctx = ResurrectionContext {
        artifact,
        node_workspace: node_workspace.clone(),
    };
    
    let active_agent = ctx.boot(None::<u64>).await.expect("Failed to boot");
    
    // Assert OutOfFuel because infinite loop uses up the 100 max instructions
    assert_eq!(active_agent.result.status, ExecutionStatus::OutOfFuel);
}

#[tokio::test]
async fn test_tainted_soul_integrity() {
    let (manifest_path, temp_dir) = standard_manifest("tainted", 10_000_000).await;
    let out_tet = temp_dir.path().join("tainted.tet");
    
    let builder = TetBuilder {
        source_wasm: PathBuf::from("tests/fixtures/lazarus.wasm"),
        manifest_path,
        vfs_path: None,
        output_path: out_tet.clone(),
        signing_key: None,
    };
    
    builder.assemble().await.unwrap();

    let mut raw_bytes = fs::read(&out_tet).unwrap();
    
    // Corrupt one bit in the middle of the byte buffer
    let middle = raw_bytes.len() / 2;
    raw_bytes[middle] ^= 0x01;
    
    let verify_result = TetBuilder::verify_and_load(&raw_bytes);
    
    assert!(verify_result.is_err(), "Tainted artifact loaded without triggering security trap!");
    
    match verify_result {
        Err(e) => {
            let es = e.to_string();
            // Could be SignatureMismatch or Serialization/Bincode error based on where the bitflip landed
            assert!(
                es.contains("Signature mismatch") || es.contains("Serialization error"),
                "Expected Integrity Error trap, got: {}", es
            );
        }
        Ok(_) => panic!("Should have failed securely"),
    }
}
