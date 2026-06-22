import os

def write_file(path, content):
    with open(path, "w") as f:
        f.write(content.strip() + "\n")

write_file("src/bin/tet_cli/mod.rs", """
pub mod infer;
pub mod memory;
pub mod network;
pub mod registry;
pub mod run;
pub mod snapshot;
pub mod status;
pub mod utils;
""")

write_file("src/bin/tet_cli/utils.rs", """
use indicatif::ProgressStyle;

pub fn get_api_url() -> String {
    std::env::var("TRYTET_API_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
}

pub fn pb_style() -> ProgressStyle {
    ProgressStyle::with_template("{spinner:.cyan} {msg} [{elapsed_precise}]")
        .unwrap()
        .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
}
""")

write_file("src/bin/tet_cli/run.rs", """
use anyhow::{anyhow, Result};
use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;
use std::fs;
use tet_core::models::TetExecutionRequest;
use crate::tet_cli::utils::{get_api_url, pb_style};

pub async fn run_payload(
    client: &Client,
    payload_path: &str,
    alias: &str,
    fuel: u64,
    memory: u64,
) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message("Preparing execution envelope...");

    let payload = fs::read(payload_path)?;
    let mut execution_payload = payload.clone();
    let mut injected_files = std::collections::HashMap::new();

    if payload_path.ends_with(".py") {
        pb.set_message("Base-Tet Python Interpreter detected. Injecting script...");
        // In a real environment, this pulls the WASI CPython Tet.
        // For testing, we mock it by treating the .py as the mock WASM and injecting the script as VFS.
        execution_payload = fs::read("tests/fixtures/python_mock.wasm").unwrap_or_else(|_| vec![]);
        let script_str = String::from_utf8_lossy(&payload).into_owned();
        injected_files.insert("script.py".to_string(), script_str);
    }

    pb.set_message(format!("Booting Tet: {} ...", alias.cyan()));

    let req = TetExecutionRequest {
        payload: Some(execution_payload),
        alias: Some(alias.to_string()),
        env: std::collections::HashMap::new(),
        injected_files,
        allocated_fuel: fuel,
        max_memory_mb: memory as u32,
        parent_snapshot_id: None,
        call_depth: 0,
        voucher: None,
        manifest: None,
        egress_policy: None,
        target_function: None,
    };

    let res = client
        .post(&format!("{}/v1/tet/execute", get_api_url()))
        .json(&req)
        .send()
        .await?;

    if res.status().is_success() {
        pb.finish_with_message(format!("{} Successfully Executed!", "✔".green()));
        let body: serde_json::Value = res.json().await?;
        println!("\\nTet Output:\\n{:#?}", body["telemetry"]);
    } else {
        pb.finish_with_message(format!("{} Execution Failed", "✘".red()));
        let code = res.status();
        let body = res.text().await?;
        println!("\\nError (HTTP {}):\\n{}", code, body);
    }

    Ok(())
}

pub async fn up_artifact(file: &std::path::PathBuf, fuel: Option<u64>) -> anyhow::Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!(
        "Resurrecting Sovereign Agent from '{}'...",
        file.display()
    ));

    let raw_bytes = std::fs::read(file)
        .map_err(|e| anyhow!("Failed to read artifact {}: {}", file.display(), e))?;

    let artifact = match tet_core::builder::TetBuilder::verify_and_load(&raw_bytes) {
        Ok(a) => a,
        Err(e) => {
            pb.finish_with_message(format!("{} Security Violation: {}", "✘".red(), e));
            std::process::exit(1);
        }
    };

    let node_workspace =
        std::env::current_dir()?.join(format!("agent_workspace_{}", uuid::Uuid::new_v4()));

    let ctx = tet_core::resurrection::ResurrectionContext {
        artifact,
        node_workspace,
    };

    pb.set_message("Booting Wasm Sandbox...");

    match ctx.boot(fuel).await {
        Ok(agent) => {
            if agent.result.status == tet_core::models::ExecutionStatus::Success {
                pb.finish_with_message(format!(
                    "{} Resurrection Complete! Agent exited cleanly.",
                    "✔".green()
                ));
            } else if agent.result.status == tet_core::models::ExecutionStatus::OutOfFuel {
                pb.finish_with_message(format!("{} Execution Trapped: OutOfFuel.", "✘".red()));
                std::process::exit(137);
            } else {
                pb.finish_with_message(format!(
                    "{} Execution Terminated: {:?}",
                    "✘".red(),
                    agent.result.status
                ));
                std::process::exit(1);
            }
        }
        Err(e) => {
            pb.finish_with_message(format!("{} Resurrection Failed: {}", "✘".red(), e));
            std::process::exit(1);
        }
    }

    Ok(())
}
""")

write_file("src/bin/tet_cli/registry.rs", """
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
""")

write_file("src/bin/tet_cli/network.rs", """
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
""")

write_file("src/bin/tet_cli/status.rs", """
use anyhow::Result;
use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;
use crate::tet_cli::utils::{get_api_url, pb_style};

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

    pb.finish_with_message(format!("{} Sovereign Hive Status", "✔".green()));

    // Agent topology table
    if !topology.is_empty() {
        println!(
            "\\n{0: <15} → {1: <15} | {2: <8} | {3: <8} | {4: <12}",
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
        println!("\\n  {} No active agents on this node.", "ℹ".blue());
    }

    // Market vitals
    println!(
        "\\n{} Market Multiplier: {}x  |  Thermal: {}°C  |  Warp: {}µs  |  Oracle: {}µs",
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

pub async fn pay_cmd(client: &Client, from: &str, to: &str, amount: u64, json_out: bool) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!(
        "💰 Transferring {} fuel: {} → {}",
        amount.to_string().yellow(),
        from.cyan(),
        to.cyan()
    ));

    let payload = serde_json::json!({
        "source_alias": from,
        "target_alias": to,
        "amount": amount,
    });

    let res = client
        .post(&format!("{}/v1/tet/topup", get_api_url()))
        .json(&payload)
        .send()
        .await;

    match res {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().await.unwrap_or(serde_json::json!({}));
            if json_out {
                pb.finish_and_clear();
                println!("{}", serde_json::to_string_pretty(&body)?);
            } else {
                pb.finish_with_message(format!(
                    "{} Transfer Complete: {} fuel ({} → {})",
                    "✔".green(),
                    amount.to_string().yellow(),
                    from.cyan(),
                    to.cyan()
                ));
            }
        }
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            if json_out {
                pb.finish_and_clear();
                println!(
                    "{}",
                    serde_json::json!({"error": body, "status": status.as_u16()})
                );
            } else {
                pb.finish_with_message(format!(
                    "{} Transfer Failed (HTTP {}): {}",
                    "✘".red(),
                    status,
                    body
                ));
            }
        }
        Err(e) => {
            if json_out {
                pb.finish_and_clear();
                println!("{}", serde_json::json!({"error": e.to_string()}));
            } else {
                pb.finish_with_message(format!("{} Network Error: {}", "✘".red(), e));
            }
        }
    }

    Ok(())
}

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
                println!("\\n  {} Filtering for alias '{}' — connect to /v1/swarm/stream for live WebSocket tail.", "ℹ".blue(), a.yellow());
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
""")

write_file("src/bin/tet_cli/memory.rs", """
use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;
use crate::tet_cli::utils::{get_api_url, pb_style};

pub async fn memory_query(client: &Client, alias: &str, vector: &str) -> anyhow::Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!("Querying semantic memory for {}...", alias.cyan()));

    // Parse the float string (e.g. "[0.1, 0.2]")
    let vec_query: Result<Vec<f32>, _> = serde_json::from_str(vector);
    let vector_data = match vec_query {
        Ok(v) => v,
        Err(_) => {
            pb.finish_with_message(format!(
                "{} Vector must be floating array like [0.1, 0.2]",
                "✘".red()
            ));
            return Ok(());
        }
    };

    let query_payload = tet_core::memory::SearchQuery {
        collection: "default".to_string(), // Default space
        query_vector: vector_data,
        limit: 5,
        min_score: 0.0,
    };

    let res = client
        .post(&format!("{}/v1/tet/memory/{}", get_api_url(), alias))
        .json(&query_payload)
        .send()
        .await?;

    if res.status().is_success() {
        let results: Vec<tet_core::memory::SearchResult> = res.json().await?;
        pb.finish_with_message(format!(
            "{} Retrieved {} Semantic Matches from {}",
            "✔".green(),
            results.len(),
            alias.cyan()
        ));
        for (i, result) in results.iter().enumerate() {
            println!(
                "  [Match {}] ID: {} (Score: {:.4})",
                i,
                result.id.yellow(),
                result.score
            );
        }
    } else {
        let err = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown API Error".into());
        pb.finish_with_message(format!("{} Memory search failed: {}", "✘".red(), err));
    }

    Ok(())
}
""")

write_file("src/bin/tet_cli/infer.rs", """
use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;
use crate::tet_cli::utils::{get_api_url, pb_style};

pub async fn infer_cmd(
    client: &Client,
    alias: &str,
    prompt: &str,
    model: &str,
    temperature: f32,
    max_tokens: u32,
) -> anyhow::Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!(
        "Invoking Sovereign Inference on {}...",
        alias.cyan()
    ));

    let request = tet_core::inference::InferenceRequest {
        model_alias: model.to_string(),
        prompt: prompt.to_string(),
        temperature,
        max_tokens,
        stop_sequences: Vec::new(),
        session_id: None,
        deterministic_seed: 42,
    };

    let res = client
        .post(&format!("{}/v1/tet/infer/{}", get_api_url(), alias))
        .json(&request)
        .send()
        .await?;

    if res.status().is_success() {
        let response: tet_core::inference::InferenceResponse = res.json().await?;
        pb.finish_with_message(format!(
            "{} Inference Complete ({})",
            "✔".green(),
            response.model_alias.cyan()
        ));
        println!("\\n{}", response.text);
        println!(
            "\\n  {} Prompt Tokens: {}, Generated: {}, Fuel Burned: {}",
            "⚡".yellow(),
            response.prompt_tokens,
            response.tokens_generated,
            response.fuel_burned
        );
        println!("  {} Session: {}", "🧠".to_string(), response.session_id);
    } else {
        let err = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown API Error".into());
        pb.finish_with_message(format!("{} Inference failed: {}", "✘".red(), err));
    }

    Ok(())
}
""")

write_file("src/bin/tet_cli/snapshot.rs", """
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
""")
