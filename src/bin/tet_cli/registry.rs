use anyhow::{anyhow, Result};
use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;
use std::fs;
use crate::tet_cli::utils::{get_api_url, pb_style};

pub async fn push(path: &std::path::Path, alias: &str) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!("Publishing Sovereign Artifact {} to the Hive...", alias.cyan()));

    if !path.exists() {
        return Err(anyhow!("Artifact path {} does not exist", path.display()));
    }
    let wasm_bytes = fs::read(path)?;

    // We send this request to our local daemon API, which handles the SovereignRegistry logic
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

pub async fn pull(alias: &str, _version: Option<&str>) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!("Querying Hive for Global Alias {}...", alias.cyan()));

    let client = Client::new();
    let res = client
        .get(&format!("{}/v1/tet/pull/{}", get_api_url(), alias))
        .send()
        .await?;

    if res.status().is_success() {
        let _payload = res.bytes().await?;
        // In local flow, the daemon downloads it and saves it. CLI just triggers and awaits.
        pb.finish_with_message(format!(
            "{} Resolved and pulled agent state via mesh P2P securely.",
            "✔".green()
        ));
    } else {
        let err = res.text().await.unwrap_or_else(|_| "Unknown error".into());
        pb.finish_with_message(format!("{} Pull Failed: {}", "✘".red(), err));
    }
    Ok(())
}

pub fn load_auth() -> Result<std::collections::HashMap<String, String>> {
    let path = home::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?.join(".trytet").join("auth.json");
    if path.exists() {
        let data = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&data)?)
    } else {
        Ok(std::collections::HashMap::new())
    }
}

pub async fn login(registry: &str, token: &str) -> Result<()> {
    let mut auth = load_auth()?;
    auth.insert(registry.to_string(), token.to_string());

    let path = home::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?.join(".trytet").join("auth.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string(&auth)?)?;

    println!(
        "{} Scoped authentication token persisted for {}",
        "✔".green(),
        registry.cyan()
    );
    Ok(())
}
