use anyhow::{anyhow, Result};
use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use tet_core::crypto::AgentWallet;
use tet_core::models::{TetHashes, TetManifest};
use tet_core::sandbox::SnapshotPayload;
use crate::tet_cli::utils::{get_api_url, pb_style};

pub async fn snapshot(client: &Client, alias: &str, tag: &str) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!("Snapshotting {} natively...", alias.cyan()));

    // 1. Tell engine to snapshot the alias
    let res = client
        .post(&format!("{}/v1/tet/snapshot/{}", get_api_url(), alias))
        .send()
        .await?;
    if !res.status().is_success() {
        return Err(anyhow!("Failed engine snapshot"));
    }
    let body: serde_json::Value = res.json().await?;
    let snapshot_id = body["snapshot_id"].as_str().unwrap().to_string();

    pb.set_message("Exporting atomic RAM and VFS payload...");

    // 2. Export raw SnapshotPayload
    let payload_res = client
        .get(&format!("{}/v1/tet/export/{}", get_api_url(), snapshot_id))
        .send()
        .await?;
    if !payload_res.status().is_success() {
        return Err(anyhow!("Failed to export snapshot payload"));
    }
    let payload: SnapshotPayload = payload_res.json().await?;

    pb.set_message("Hashing artifacts & Signing manifest...");

    // 3. Hash components
    let wasm_hash = hex::encode(Sha256::digest(&payload.wasm_bytes));
    let memory_hash = hex::encode(Sha256::digest(&payload.memory_bytes));
    let vfs_hash = hex::encode(Sha256::digest(&payload.fs_tarball));

    // 4. Create Manifest
    let wallet = AgentWallet::load_or_create()?;
    let hashes = TetHashes {
        wasm_hash,
        memory_hash,
        vfs_hash,
    };

    let manifest = TetManifest {
        name: tag.to_string(),
        version: "1.0".into(),
        created_at: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        author_pubkey: wallet.public_key_hex(),
        hashes,
    };

    // 5. Build `.tet` local temporary representation (tar)
    let home_dir = home::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let local_builds = home_dir.join(".trytet").join("builds");
    fs::create_dir_all(&local_builds)?;
    let tet_file = local_builds.join(format!("{}.tet", tag.replace("/", "_")));

    let file = fs::File::create(&tet_file)?;
    let mut builder = tar::Builder::new(file);

    let manifest_bytes = serde_json::to_vec(&manifest)?;
    let mut h1 = tar::Header::new_gnu();
    h1.set_size(manifest_bytes.len() as u64);
    h1.set_cksum();
    builder.append_data(&mut h1, "manifest.json", manifest_bytes.as_slice())?;

    let mut h2 = tar::Header::new_gnu();
    h2.set_size(payload.wasm_bytes.len() as u64);
    h2.set_cksum();
    builder.append_data(&mut h2, "module.wasm", payload.wasm_bytes.as_slice())?;

    let mut h3 = tar::Header::new_gnu();
    h3.set_size(payload.memory_bytes.len() as u64);
    h3.set_cksum();
    builder.append_data(&mut h3, "memory.bin", payload.memory_bytes.as_slice())?;

    let mut h4 = tar::Header::new_gnu();
    h4.set_size(payload.fs_tarball.len() as u64);
    h4.set_cksum();
    builder.append_data(&mut h4, "vfs.tar", payload.fs_tarball.as_slice())?;

    let mut h5 = tar::Header::new_gnu();
    h5.set_size(payload.vector_idx.len() as u64);
    h5.set_cksum();
    builder.append_data(&mut h5, "vector.idx", payload.vector_idx.as_slice())?;

    let mut h6 = tar::Header::new_gnu();
    h6.set_size(payload.inference_state.len() as u64);
    h6.set_cksum();
    builder.append_data(&mut h6, "inference.bin", payload.inference_state.as_slice())?;

    builder.finish()?;

    pb.finish_with_message(format!(
        "{} Captured atomic snapshot to {}",
        "✔".green(),
        tag.cyan()
    ));

    Ok(())
}
