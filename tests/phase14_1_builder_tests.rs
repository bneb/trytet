use ed25519_dalek::SigningKey;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use tet_core::builder::{BuildError, TetArtifact, TetBuilder};
use tet_core::models::manifest::{AgentManifest, ManifestError};

#[tokio::test]
async fn test_identity_constraint() {
    let raw_toml = r#"
        [metadata]
        name = "test-agent"
        version = "1.0.0"

        [constraints]
        max_memory_pages = 256
        fuel_limit = 100000

        [permissions]
        can_egress = []
        can_persist = false
        can_teleport = true
    "#;

    let result = AgentManifest::from_toml(raw_toml);
    assert!(matches!(result, Err(ManifestError::MissingIdentity)));

    let raw_toml_empty = r#"
        [metadata]
        name = "test-agent"
        version = "1.0.0"
        author_pubkey = ""

        [constraints]
        max_memory_pages = 256
        fuel_limit = 100000

        [permissions]
        can_egress = []
        can_persist = false
        can_teleport = true
    "#;
    let result2 = AgentManifest::from_toml(raw_toml_empty);
    assert!(matches!(result2, Err(ManifestError::MissingIdentity)));
}

#[tokio::test]
async fn test_compression_efficiency() {
    let temp_dir = TempDir::new().unwrap();
    let wasm_path = temp_dir.path().join("dummy.wasm");
    fs::write(
        &wasm_path,
        vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
    )
    .unwrap();

    // Create 10MB highly compressible file (all zeroes)
    let vfs_path = temp_dir.path().join("vfs_genesis.tar");
    let ten_mb = vec![0u8; 10 * 1024 * 1024];
    fs::write(&vfs_path, &ten_mb).unwrap();

    let manifest_path = temp_dir.path().join("tet.toml");
    let raw_toml = r#"
        [metadata]
        name = "compression-test"
        version = "1.0.0"
        author_pubkey = "placeholder"

        [constraints]
        max_memory_pages = 256
        fuel_limit = 100000

        [permissions]
        can_egress = []
        can_persist = true
        can_teleport = true
    "#;
    fs::write(&manifest_path, raw_toml).unwrap();

    let out_path = temp_dir.path().join("out.tet");

    let builder = TetBuilder {
        source_wasm: wasm_path,
        manifest_path,
        vfs_path: Some(vfs_path),
        output_path: out_path.clone(),
        signing_key: None,
    };

    let receipt = builder.assemble().await.expect("Build failed");

    // Zstd should crush 10MB of 0s to under a few KB, meaning definitely < 50%
    assert!(
        receipt.size_bytes < (5 * 1024 * 1024),
        "Compression failed to reduce size by 50%"
    );
    assert!(fs::metadata(&out_path).unwrap().len() < (5 * 1024 * 1024));
}

#[tokio::test]
async fn test_signature_integrity() {
    let temp_dir = TempDir::new().unwrap();
    let wasm_path = temp_dir.path().join("dummy.wasm");
    // Minimal wasm struct
    fs::write(
        &wasm_path,
        vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
    )
    .unwrap();

    let manifest_path = temp_dir.path().join("tet.toml");
    let raw_toml = r#"
        [metadata]
        name = "sig-test"
        version = "1.0.0"
        author_pubkey = "placeholder"

        [constraints]
        max_memory_pages = 256
        fuel_limit = 100000

        [permissions]
        can_egress = []
        can_persist = true
        can_teleport = true
    "#;
    fs::write(&manifest_path, raw_toml).unwrap();

    let out_path = temp_dir.path().join("out.tet");

    let builder = TetBuilder {
        source_wasm: wasm_path,
        manifest_path,
        vfs_path: None,
        output_path: out_path.clone(),
        signing_key: None,
    };

    builder.assemble().await.expect("Build failed");

    // Manually manipulate the .tet file to flip a bit in WASM layer
    let mut bad_artifact_bytes = fs::read(&out_path).unwrap();

    // We can parse it, manipulate the blueprint_wasm, then re-serialize to properly corrupt the signature
    let mut artifact: TetArtifact = bincode::deserialize(&bad_artifact_bytes).unwrap();

    // Corrupt one bit
    artifact.blueprint_wasm[2] ^= 0x01;

    let re_serialized = bincode::serialize(&artifact).unwrap();

    let load_result = TetBuilder::verify_and_load(&re_serialized);

    assert!(
        matches!(load_result, Err(BuildError::SignatureMismatch)),
        "Did not trap with SignatureMismatch on bitflip"
    );
}
