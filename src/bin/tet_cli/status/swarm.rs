use crate::tet_cli::utils::{get_api_url, pb_style};
use anyhow::Result;
use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;

pub async fn swarm(client: &Client) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message("Compiling Swarm Topography...");

    let res = client
        .get(&format!("{}/v1/topology", get_api_url()))
        .send()
        .await?;

    if res.status().is_success() {
        let body: serde_json::Value = res.json().await?;
        let edges = body.as_array();
        match edges {
            Some(e) if !e.is_empty() => {
                pb.finish_with_message(format!(
                    "{} Swarm Topology Active ({} edges)",
                    "✔".green(),
                    e.len()
                ));
                println!(
                    "{0: <15} -> {1: <15} | {2: <10} | {3: <12} | {4: <10} | {5: <10}",
                    "Source".bold(),
                    "Target".bold(),
                    "Calls".bold(),
                    "Errors".bold(),
                    "Latency (µs)".bold(),
                    "Bytes".bold()
                );
                println!("{:-<85}", "");

                let mut edges_vec: Vec<_> = e.iter().collect();
                // Sort by total bytes descending
                edges_vec.sort_by_key(|edge| {
                    std::cmp::Reverse(edge["total_bytes"].as_u64().unwrap_or(0))
                });

                for edge in edges_vec {
                    let source = edge["source"].as_str().unwrap_or("?");
                    let target = edge["target"].as_str().unwrap_or("?");
                    let calls = edge["call_count"].as_u64().unwrap_or(0);
                    let errors = edge["error_count"].as_u64().unwrap_or(0);
                    let latency = edge["total_latency_us"].as_u64().unwrap_or(0);
                    let bytes = edge["total_bytes"].as_u64().unwrap_or(0);

                    let avg_latency = if calls > 0 { latency / calls } else { 0 };
                    let err_fmt = if errors > 0 {
                        format!("{}", errors).red()
                    } else {
                        format!("{}", errors).green()
                    };

                    println!(
                        "{0: <15} -> {1: <15} | {2: <10} | {3: <12} | {4: <10} | {5: <10}",
                        source.yellow(),
                        target.cyan(),
                        calls,
                        err_fmt,
                        avg_latency,
                        bytes
                    );
                }
            }
            _ => {
                pb.finish_with_message(format!(
                    "{} No Swarm Telemetry recorded (Sandbox is quiet).",
                    "ℹ".blue()
                ));
            }
        }
    } else {
        let raw = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        pb.finish_with_message(format!(
            "{} Topology compilation Failed: {}",
            "✘".red(),
            raw
        ));
    }

    Ok(())
}
