use crate::tet_cli::utils::{get_api_url, pb_style};
use anyhow::Result;
use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;

pub async fn metrics_cmd(client: &Client, json_out: bool) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message("Running Northstar Benchmarking Suite...");

    let res = client
        .get(&format!("{}/v1/swarm/metrics", get_api_url()))
        .send()
        .await;

    match res {
        Ok(r) if r.status().is_success() => {
            let report: serde_json::Value = r.json().await.unwrap_or(serde_json::json!({}));

            if json_out {
                pb.finish_and_clear();
                println!("{}", serde_json::to_string_pretty(&report)?);
                return Ok(());
            }

            pb.finish_with_message(format!("{} Northstar Report", "✔".green()));
            println!();
            println!(
                "  {0: <28} | {1: <15} | {2: <10}",
                "Metric".bold(),
                "Value".bold(),
                "Ceiling".bold()
            );
            println!("  {:-<60}", "");

            let warp = report["teleport_warp_us"].as_u64().unwrap_or(0);
            let warp_ok = if warp < 200_000 {
                "✔".green()
            } else {
                "✘".red()
            };
            println!(
                "  {0: <28} | {1: <15} | {2: <10} {3}",
                "Teleport Warp (µs)",
                warp.to_string().cyan(),
                "< 200,000",
                warp_ok
            );

            let mitosis = report["mitosis_latency_us"].as_u64().unwrap_or(0);
            let mitosis_ok = if mitosis < 15_000 {
                "✔".green()
            } else {
                "✘".red()
            };
            println!(
                "  {0: <28} | {1: <15} | {2: <10} {3}",
                "Mitosis Constant (µs)",
                mitosis.to_string().cyan(),
                "< 15,000",
                mitosis_ok
            );

            let oracle = report["oracle_verification_us"].as_u64().unwrap_or(0);
            let oracle_ok = if oracle < 5_000 {
                "✔".green()
            } else {
                "✘".red()
            };
            println!(
                "  {0: <28} | {1: <15} | {2: <10} {3}",
                "Oracle Fidelity (µs)",
                oracle.to_string().cyan(),
                "< 5,000",
                oracle_ok
            );

            let evac = report["market_evacuation_ms"].as_u64().unwrap_or(0);
            let evac_ok = if evac < 800 {
                "✔".green()
            } else {
                "✘".red()
            };
            println!(
                "  {0: <28} | {1: <15} | {2: <10} {3}",
                "Market Evacuation (ms)",
                evac.to_string().cyan(),
                "< 800",
                evac_ok
            );

            let eff = report["fuel_efficiency_ratio"].as_f64().unwrap_or(0.0);
            println!(
                "  {0: <28} | {1: <15} | {2: <10}",
                "Fuel Efficiency",
                format!("{:.4}", eff).yellow(),
                "higher=better"
            );
        }
        Ok(r) => {
            let err = r.text().await.unwrap_or_default();
            if json_out {
                pb.finish_and_clear();
                println!("{}", serde_json::json!({"error": err}));
            } else {
                pb.finish_with_message(format!("{} Metrics unavailable: {}", "✘".red(), err));
            }
        }
        Err(e) => {
            if json_out {
                pb.finish_and_clear();
                println!("{}", serde_json::json!({"error": e.to_string()}));
            } else {
                pb.finish_with_message(format!(
                    "{} Cannot reach engine at {} — is it running? ({})",
                    "✘".red(),
                    get_api_url().yellow(),
                    e
                ));
            }
        }
    }

    Ok(())
}
