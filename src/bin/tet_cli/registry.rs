use crate::tet_cli::utils::{get_api_url, pb_style};
use anyhow::{anyhow, Result};
use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;
use std::fs;

pub async fn push(path: &std::path::Path, alias: &str) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!(
        "Publishing artifact {} to the Hive...",
        alias.cyan()
    ));

    if !path.exists() {
        return Err(anyhow!("Artifact path {} does not exist", path.display()));
    }
    let wasm_bytes = fs::read(path)?;

    // Send request to local API which handles registry logic
    let client = Client::new();
    let res = client
        .post(&format!("{}/v1/tet/push/{}", get_api_url(), alias))
        .body(wasm_bytes)
        .send()
        .await?;

    if res.status().is_success() {
        pb.finish_with_message(format!(
            "{} Artifact deployed! CID Hash broadcast to DHT.",
            "✔".green()
        ));
    } else {
        let err = res.text().await.unwrap_or_else(|_| "Unknown error".into());
        pb.finish_with_message(format!("{} Push Failed: {}", "✘".red(), err));
    }
    Ok(())
}
