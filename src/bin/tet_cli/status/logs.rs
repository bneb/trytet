use crate::tet_cli::utils::{get_api_url, pb_style};
use anyhow::Result;
use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;

pub async fn logs_cmd(client: &Client, alias: Option<&str>, json_out: bool) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    let filter_msg = alias
        .map(|a| format!(" (filtering: {})", a.cyan()))
        .unwrap_or_default();
    pb.set_message(format!("Connecting to TelemetryHub...{}", filter_msg));

    // Connect to the WebSocket telemetry stream
    // In this implementation, we poll the metrics endpoint once and display
    // the current snapshot with human-readable icons. A persistent WebSocket
    // tail would use the /v1/swarm/stream endpoint.
    let res = client
        .get(&format!("{}/v1/swarm/metrics", get_api_url()))
        .send()
        .await;

    match res {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().await.unwrap_or(serde_json::json!({}));

            if json_out {
                pb.finish_and_clear();
                println!("{}", serde_json::to_string_pretty(&body)?);
                return Ok(());
            }

            pb.finish_with_message(format!("{} TelemetryHub Live Snapshot", "✔".green()));

            println!(
                "  🧠 Oracle Fidelity:     {}µs (Ed25519 sign+verify per fetch)",
                body["oracle_verification_us"]
                    .as_u64()
                    .unwrap_or(0)
                    .to_string()
                    .cyan()
            );
            println!(
                "  ✈️  Teleport Warp:       {}µs (bincode serialize round-trip)",
                body["teleport_warp_us"]
                    .as_u64()
                    .unwrap_or(0)
                    .to_string()
                    .cyan()
            );
            println!(
                "  💰 Fuel Efficiency:      {}",
                body["fuel_efficiency_ratio"]
                    .as_f64()
                    .map(|v| format!("{:.4}", v))
                    .unwrap_or("—".into())
                    .yellow()
            );
            println!(
                "  🌡️  Market Evacuation:   {}ms (thermal panic drill)",
                body["market_evacuation_ms"]
                    .as_u64()
                    .unwrap_or(0)
                    .to_string()
                    .cyan()
            );
            println!(
                "  🧬 Mitosis Constant:     {}µs (CoW fork latency)",
                body["mitosis_latency_us"]
                    .as_u64()
                    .unwrap_or(0)
                    .to_string()
                    .cyan()
            );

            if let Some(a) = alias {
                println!("\n  {} Filtering for alias '{}' — connect to /v1/swarm/stream for live WebSocket tail.", "ℹ".blue(), a.yellow());
            }
        }
        Ok(r) => {
            let err = r.text().await.unwrap_or_default();
            pb.finish_with_message(format!("{} TelemetryHub unavailable: {}", "✘".red(), err));
        }
        Err(e) => {
            pb.finish_with_message(format!(
                "{} Cannot reach engine at {} — is it running? ({})",
                "✘".red(),
                get_api_url().yellow(),
                e
            ));
        }
    }

    Ok(())
}
