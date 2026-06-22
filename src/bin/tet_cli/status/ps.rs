use crate::tet_cli::utils::{get_api_url, pb_style};
use anyhow::Result;
use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;

pub async fn ps_cmd(client: &Client, json_out: bool) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message("Scanning active agents across the Hive...");

    // Fetch metrics to get market vitals
    let metrics_res = client
        .get(&format!("{}/v1/swarm/metrics", get_api_url()))
        .send()
        .await;

    // Fetch topology for agent list
    let topo_res = client
        .get(&format!("{}/v1/topology", get_api_url()))
        .send()
        .await;

    // Fetch peers for market multipliers
    let peers_res = client
        .get(&format!("{}/v1/hive/peers", get_api_url()))
        .send()
        .await;

    let metrics: serde_json::Value = match metrics_res {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or(serde_json::json!({})),
        _ => serde_json::json!({}),
    };

    let topology: Vec<serde_json::Value> = match topo_res {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or_default(),
        _ => vec![],
    };

    let peers: serde_json::Value = match peers_res {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or(serde_json::json!({})),
        _ => serde_json::json!({}),
    };

    if json_out {
        let report = serde_json::json!({
            "agents": topology,
            "peers": peers["peers"],
            "metrics": metrics,
        });
        pb.finish_and_clear();
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    pb.finish_with_message(format!("{} Hive Status", "✔".green()));

    // Agent topology table
    if !topology.is_empty() {
        println!(
            "\n{0: <15} → {1: <15} | {2: <8} | {3: <8} | {4: <12}",
            "Source".bold(),
            "Target".bold(),
            "Calls".bold(),
            "Errors".bold(),
            "Avg µs".bold()
        );
        println!("{:-<70}", "");
        for edge in &topology {
            let source = edge["source"].as_str().unwrap_or("?");
            let target = edge["target"].as_str().unwrap_or("?");
            let calls = edge["call_count"].as_u64().unwrap_or(0);
            let errors = edge["error_count"].as_u64().unwrap_or(0);
            let latency = edge["total_latency_us"].as_u64().unwrap_or(0);
            let avg = if calls > 0 { latency / calls } else { 0 };

            let err_col = if errors > 0 {
                format!("{}", errors).red()
            } else {
                format!("{}", errors).green()
            };

            println!(
                "{0: <15} → {1: <15} | {2: <8} | {3: <8} | {4: <12}",
                source.yellow(),
                target.cyan(),
                calls,
                err_col,
                avg
            );
        }
    } else {
        println!("\n  {} No active agents on this node.", "ℹ".blue());
    }

    // Market vitals
    println!(
        "\n{} Market Multiplier: {}x  |  Thermal: {}°C  |  Warp: {}µs  |  Oracle: {}µs",
        "📊".to_string(),
        metrics["fuel_efficiency_ratio"]
            .as_f64()
            .map(|v| format!("{:.2}", v))
            .unwrap_or("—".into())
            .cyan(),
        "—".yellow(),
        metrics["teleport_warp_us"]
            .as_u64()
            .map(|v| format!("{}", v))
            .unwrap_or("—".into()),
        metrics["oracle_verification_us"]
            .as_u64()
            .map(|v| format!("{}", v))
            .unwrap_or("—".into()),
    );

    Ok(())
}
