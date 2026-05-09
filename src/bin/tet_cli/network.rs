use anyhow::Result;
use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;
use crate::tet_cli::utils::{get_api_url, pb_style};

pub async fn teleport(client: &Client, alias: &str, target_node: &str) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!(
        "Initiating Live Migration for {} to {}...",
        alias.cyan(),
        target_node.yellow()
    ));

    let res = client
        .post(&format!("{}/v1/tet/teleport/{}", get_api_url(), alias))
        .json(&serde_json::json!({ "target_node": target_node }))
        .send()
        .await?;

    if res.status().is_success() {
        let body: serde_json::Value = res.json().await?;
        let msg = body["message"].as_str().unwrap_or("Teleport successful");
        pb.finish_with_message(format!("{} {}", "✔".green(), msg));
    } else {
        let raw = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        pb.finish_with_message(format!("{} Teleport Failed: {}", "✘".red(), raw));
    }

    Ok(())
}

pub async fn bridge(client: &Client, alias: &str, path: &str) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!(
        "Bridging alias {} -> {}",
        alias.cyan(),
        path.green()
    ));

    let route = tet_core::oracle::IngressRoute {
        public_path: path.to_string(),
        target_alias: alias.to_string(),
        method_filter: vec![], // Allow all
    };

    let res = client
        .post(&format!("{}/v1/ingress/register", get_api_url()))
        .json(&route)
        .send()
        .await?;

    if res.status().is_success() {
        pb.finish_with_message(format!(
            "{} Successfully mapped Gateway: {} ↔ Mesh: {}",
            "✔".green(),
            path.yellow(),
            alias.cyan()
        ));
    } else {
        let raw = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        pb.finish_with_message(format!("{} Bridge mapping failed: {}", "✘".red(), raw));
    }

    Ok(())
}
