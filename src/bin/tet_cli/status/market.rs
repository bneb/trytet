use crate::tet_cli::utils::{get_api_url, pb_style};
use anyhow::Result;
use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;

pub async fn market_list(client: &Client) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message("Querying Global Market Rates...");

    let res = client
        .get(&format!("{}/v1/hive/peers", get_api_url()))
        .send()
        .await?;

    if res.status().is_success() {
        let body: serde_json::Value = res.json().await?;
        let peers_val = body["peers"].as_array();
        match peers_val {
            Some(p) if !p.is_empty() => {
                let mut peers: Vec<_> = p.iter().collect();
                // Sort by price ascending
                peers.sort_by_key(|peer| {
                    peer["price_per_million_fuel"].as_u64().unwrap_or(u64::MAX)
                });

                pb.finish_with_message(format!(
                    "{} Live Market Active ({} providers discovered) 🌐",
                    "✔".green(),
                    p.len()
                ));
                println!(
                    "{0: <36} | {1: <15} | {2: <10} | {3: <10}",
                    "Node ID".bold(),
                    "Price (µFuel)".bold(),
                    "Capacity".bold(),
                    "Score".bold()
                );
                println!("{:-<77}", "");
                for peer in peers {
                    let id = peer["node_id"].as_str().unwrap_or("?");
                    let price = peer["price_per_million_fuel"].as_u64().unwrap_or(0);
                    let capacity = peer["available_capacity_mb"].as_u64().unwrap_or(0);
                    let rep = peer["min_reputation_score"].as_u64().unwrap_or(0);

                    let id_disp = if id.len() > 36 { &id[..36] } else { id };
                    println!(
                        "{0: <36} | {1: <15} | {2: <10} | {3: <10}",
                        id_disp.yellow(),
                        format!("{}", price).cyan(),
                        format!("{}MB", capacity),
                        rep
                    );
                }
            }
            _ => {
                pb.finish_with_message(format!("{} No Market Providers available.", "ℹ".blue()));
            }
        }
    } else {
        let raw = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        pb.finish_with_message(format!("{} Market query Failed: {}", "✘".red(), raw));
    }

    Ok(())
}
