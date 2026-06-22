use crate::tet_cli::utils::{get_api_url, pb_style};
use anyhow::Result;
use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;

pub async fn hive_list(client: &Client) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message("Discovering Hive Peers...");

    let res = client
        .get(&format!("{}/v1/hive/peers", get_api_url()))
        .send()
        .await?;

    if res.status().is_success() {
        let body: serde_json::Value = res.json().await?;
        let peers = body["peers"].as_array();
        match peers {
            Some(p) if !p.is_empty() => {
                pb.finish_with_message(format!(
                    "{} Found {} connected Hive Nodes:",
                    "✔".green(),
                    p.len()
                ));
                for peer in p {
                    println!(
                        "  {} [{}]",
                        peer["node_id"].as_str().unwrap_or("?").yellow(),
                        peer["public_addr"].as_str().unwrap_or("?").cyan()
                    );
                }
            }
            _ => {
                pb.finish_with_message(format!(
                    "{} No nodes in local P2P routing table.",
                    "ℹ".blue()
                ));
            }
        }
    } else {
        let raw = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        pb.finish_with_message(format!("{} Hive lookup Failed: {}", "✘".red(), raw));
    }

    Ok(())
}
